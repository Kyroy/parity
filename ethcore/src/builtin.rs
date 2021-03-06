// Copyright 2015, 2016 Ethcore (UK) Ltd.
// This file is part of Parity.

// Parity is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity.  If not, see <http://www.gnu.org/licenses/>.

use crypto::sha2::Sha256 as Sha256Digest;
use crypto::ripemd160::Ripemd160 as Ripemd160Digest;
use crypto::digest::Digest;
use std::cmp::min;
use util::{U256, H256, Hashable, FixedHash, BytesRef};
use ethkey::{Signature, recover as ec_recover};
use ethjson;

use std::process::Command;
use std::env;
use std::fs::File;
use std::path::Path;
use std::error::Error;
use std::io::prelude::*;

/// Native implementation of a built-in contract.
pub trait Impl: Send + Sync {
	/// execute this built-in on the given input, writing to the given output.
	fn execute(&self, input: &[u8], output: &mut BytesRef);
}

/// A gas pricing scheme for built-in contracts.
pub trait Pricer: Send + Sync {
	/// The gas cost of running this built-in for the given size of input data.
	fn cost(&self, in_size: usize) -> U256;
}

/// A linear pricing model. This computes a price using a base cost and a cost per-word.
struct Linear {
	base: usize,
	word: usize,
}

impl Pricer for Linear {
	fn cost(&self, in_size: usize) -> U256 {
		U256::from(self.base) + U256::from(self.word) * U256::from((in_size + 31) / 32)
	}
}

/// Pricing scheme and execution definition for a built-in contract.
pub struct Builtin {
	pricer: Box<Pricer>,
	native: Box<Impl>,
}

impl Builtin {
	/// Simple forwarder for cost.
	pub fn cost(&self, s: usize) -> U256 { self.pricer.cost(s) }

	/// Simple forwarder for execute.
	pub fn execute(&self, input: &[u8], output: &mut BytesRef) { self.native.execute(input, output) }
}

impl From<ethjson::spec::Builtin> for Builtin {
	fn from(b: ethjson::spec::Builtin) -> Self {
		let pricer = match b.pricing {
			ethjson::spec::Pricing::Linear(linear) => {
				Box::new(Linear {
					base: linear.base,
					word: linear.word,
				})
			}
		};

		Builtin {
			pricer: pricer,
			native: ethereum_builtin(&b.name),
		}
	}
}

// Ethereum builtin creator.
fn ethereum_builtin(name: &str) -> Box<Impl> {
	match name {
		"customPrecompile" => Box::new(CustomPrecompile) as Box<Impl>,
		"identity" => Box::new(Identity) as Box<Impl>,
		"ecrecover" => Box::new(EcRecover) as Box<Impl>,
		"sha256" => Box::new(Sha256) as Box<Impl>,
		"ripemd160" => Box::new(Ripemd160) as Box<Impl>,
		_ => panic!("invalid builtin name: {}", name),
	}
}

// Ethereum builtins:
//
// - The identity function
// - ec recovery
// - sha256
// - ripemd160

#[derive(Debug)]
struct CustomPrecompile;

#[derive(Debug)]
struct Identity;

#[derive(Debug)]
struct EcRecover;

#[derive(Debug)]
struct Sha256;

#[derive(Debug)]
struct Ripemd160;

fn usize_from_array(arr: &[u8]) -> usize {
	let mut result: usize = 0;
	for i in 0..arr.len() {
		if arr[i] != 0 {
			// TODO panics when too big (pow)
			result += arr[i] as usize * 256usize.pow((arr.len() - 1 - i) as u32);
		}
	}
	return result;
}

enum EthType {
	// fixed sized
	Address,
	Bytes32,
	// dynamic sized
	String,
	Bytes,
}

fn get_args(arg_types: &[EthType], input: Vec<u8>) -> Vec<Vec<u8>> {
	let arg_count = arg_types.len();
	let mut args = Vec::new();
	for i in 0..arg_count {
		args.push(match arg_types[i] {
			EthType::Address | EthType::Bytes32=> {
				let arg_start = 32 * i;
				let arg_end = 32 * (i + 1);
				input[arg_start..arg_end].to_vec()
			},
			EthType::String | EthType::Bytes => {
				let arg_size_start = usize_from_array(&input[32 * i..32 * (i + 1)]);
				let arg_size = usize_from_array(&input[arg_size_start..arg_size_start + 32]);
				let arg_start = arg_size_start + 32;
				let arg_end = arg_start + arg_size;
				input[arg_start..arg_end].to_vec()
			},
		});
	}
	return args;
}

fn write_to_file(filename: String, content: &[u8]) -> bool {
	let path = Path::new(&filename);
	let display = path.display();
	let mut file = match File::create(&path) {
		Ok(file) => file,
		Err(why) => {
			println!("Could not create file {}: {}", display, why.description());
			return false
		},
	};
	match file.write_all(content) {
		Ok(_) => {}, //println!("successfully wrote to {}", display),
		Err(why) => {
			println!("couldn't write to {}: {}", display, why.description());
			return false
		},
	}
	return true;
}

impl Impl for CustomPrecompile {
	fn execute(&self, input: &[u8], output: &mut BytesRef) {
		// TODO check method signature
		// TODO handle unwrap panics?
		let output_len = output.len();
		// set all bits of output to 0 => false
		for i in 0..output_len {
			output.write(i, &[0]);
		}

		let pepper_dir = match env::var("PEPPER_DIR") {
			Ok(dir) => dir,
			_ => return,
		};

		let args = get_args(&[EthType::String, EthType::Bytes32, EthType::String, EthType::String, EthType::String], input[4..].to_vec());

		let ref arg_name = args.get(0).unwrap();
		let name = String::from_utf8(arg_name.to_vec()).unwrap();

		let ref arg_id = args.get(1).unwrap();
		// use format for not shorten string with "..." (later)
		let id = format!("{:?}", H256::from_slice(arg_id));

		let ref arg_inputs = args.get(2).unwrap();
		let ref arg_outputs = args.get(3).unwrap();
		let ref arg_proof = args.get(4).unwrap();
		if !write_to_file(format!("{}/prover_verifier_shared/{}.inputs", pepper_dir, id), arg_inputs) ||
			!write_to_file(format!("{}/prover_verifier_shared/{}.outputs", pepper_dir, id), arg_outputs) ||
			!write_to_file(format!("{}/prover_verifier_shared/{}.proof", pepper_dir, id), arg_proof) {
			println!("Failed to write a file, return false");
			return;
		}

		// run pequin verifier
		let executable = format!("bin/pepper_verifier_{}", name);
		let vkey = format!("{}.vkey", name);
		let inputs = format!("{}.inputs", id);
		let outputs = format!("{}.outputs", id);
		let proof = format!("{}.proof", id);
		let cmd = Command::new(executable).current_dir(pepper_dir)
			.arg("verify").arg(vkey).arg(inputs).arg(outputs).arg(proof).output();
		match cmd {
			Ok(result) => {
				if result.status.success() {
					if String::from_utf8_lossy(&result.stdout).contains("VERIFICATION SUCCESSFUL") {
						output.write(output_len - 1, &[1]);
					}
				}
			},
			_ => return,
		}
	}
}

impl Impl for Identity {
	fn execute(&self, input: &[u8], output: &mut BytesRef) {
		output.write(0, input);
	}
}

impl Impl for EcRecover {
	fn execute(&self, i: &[u8], output: &mut BytesRef) {
		println!(">>>> EcRecover called!!");
		let len = min(i.len(), 128);

		let mut input = [0; 128];
		input[..len].copy_from_slice(&i[..len]);

		let hash = H256::from_slice(&input[0..32]);
		let v = H256::from_slice(&input[32..64]);
		let r = H256::from_slice(&input[64..96]);
		let s = H256::from_slice(&input[96..128]);

		let bit = match v[31] {
			27 | 28 if &v.0[..31] == &[0; 31] => v[31] - 27,
			_ => return,
		};

		let s = Signature::from_rsv(&r, &s, bit);
		if s.is_valid() {
			if let Ok(p) = ec_recover(&s, &hash) {
				let r = p.sha3();
				output.write(0, &[0; 12]);
				output.write(12, &r[12..r.len()]);
			}
		}
	}
}

impl Impl for Sha256 {
	fn execute(&self, input: &[u8], output: &mut BytesRef) {
		println!(">>>> Sha256 called!!");
		let mut sha = Sha256Digest::new();
		sha.input(input);

		let mut out = [0; 32];
		sha.result(&mut out);

		output.write(0, &out);
	}
}

impl Impl for Ripemd160 {
	fn execute(&self, input: &[u8], output: &mut BytesRef) {
		println!(">>>> Ripemd160 called!!");
		let mut sha = Ripemd160Digest::new();
		sha.input(input);

		let mut out = [0; 32];
		sha.result(&mut out[12..32]);

		output.write(0, &out);
	}
}

#[cfg(test)]
mod tests {
	use super::{Builtin, Linear, ethereum_builtin, Pricer};
	use ethjson;
	use util::{U256, BytesRef};

	#[test]
	fn identity() {
		let f = ethereum_builtin("identity");

		let i = [0u8, 1, 2, 3];

		let mut o2 = [255u8; 2];
		f.execute(&i[..], &mut BytesRef::Fixed(&mut o2[..]));
		assert_eq!(i[0..2], o2);

		let mut o4 = [255u8; 4];
		f.execute(&i[..], &mut BytesRef::Fixed(&mut o4[..]));
		assert_eq!(i, o4);

		let mut o8 = [255u8; 8];
		f.execute(&i[..], &mut BytesRef::Fixed(&mut o8[..]));
		assert_eq!(i, o8[..4]);
		assert_eq!([255u8; 4], o8[4..]);
	}

	#[test]
	fn sha256() {
		use rustc_serialize::hex::FromHex;
		let f = ethereum_builtin("sha256");

		let i = [0u8; 0];

		let mut o = [255u8; 32];
		f.execute(&i[..], &mut BytesRef::Fixed(&mut o[..]));
		assert_eq!(&o[..], &(FromHex::from_hex("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855").unwrap())[..]);

		let mut o8 = [255u8; 8];
		f.execute(&i[..], &mut BytesRef::Fixed(&mut o8[..]));
		assert_eq!(&o8[..], &(FromHex::from_hex("e3b0c44298fc1c14").unwrap())[..]);

		let mut o34 = [255u8; 34];
		f.execute(&i[..], &mut BytesRef::Fixed(&mut o34[..]));
		assert_eq!(&o34[..], &(FromHex::from_hex("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855ffff").unwrap())[..]);

		let mut ov = vec![];
		f.execute(&i[..], &mut BytesRef::Flexible(&mut ov));
		assert_eq!(&ov[..], &(FromHex::from_hex("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855").unwrap())[..]);
	}

	#[test]
	fn ripemd160() {
		use rustc_serialize::hex::FromHex;
		let f = ethereum_builtin("ripemd160");

		let i = [0u8; 0];

		let mut o = [255u8; 32];
		f.execute(&i[..], &mut BytesRef::Fixed(&mut o[..]));
		assert_eq!(&o[..], &(FromHex::from_hex("0000000000000000000000009c1185a5c5e9fc54612808977ee8f548b2258d31").unwrap())[..]);

		let mut o8 = [255u8; 8];
		f.execute(&i[..], &mut BytesRef::Fixed(&mut o8[..]));
		assert_eq!(&o8[..], &(FromHex::from_hex("0000000000000000").unwrap())[..]);

		let mut o34 = [255u8; 34];
		f.execute(&i[..], &mut BytesRef::Fixed(&mut o34[..]));
		assert_eq!(&o34[..], &(FromHex::from_hex("0000000000000000000000009c1185a5c5e9fc54612808977ee8f548b2258d31ffff").unwrap())[..]);
	}

	#[test]
	fn ecrecover() {
		use rustc_serialize::hex::FromHex;
		/*let k = KeyPair::from_secret(b"test".sha3()).unwrap();
		let a: Address = From::from(k.public().sha3());
		println!("Address: {}", a);
		let m = b"hello world".sha3();
		println!("Message: {}", m);
		let s = k.sign(&m).unwrap();
		println!("Signed: {}", s);*/

		let f = ethereum_builtin("ecrecover");

		let i = FromHex::from_hex("47173285a8d7341e5e972fc677286384f802f8ef42a5ec5f03bbfa254cb01fad000000000000000000000000000000000000000000000000000000000000001b650acf9d3f5f0a2c799776a1254355d5f4061762a237396a99a0e0e3fc2bcd6729514a0dacb2e623ac4abd157cb18163ff942280db4d5caad66ddf941ba12e03").unwrap();

		let mut o = [255u8; 32];
		f.execute(&i[..], &mut BytesRef::Fixed(&mut o[..]));
		assert_eq!(&o[..], &(FromHex::from_hex("000000000000000000000000c08b5542d177ac6686946920409741463a15dddb").unwrap())[..]);

		let mut o8 = [255u8; 8];
		f.execute(&i[..], &mut BytesRef::Fixed(&mut o8[..]));
		assert_eq!(&o8[..], &(FromHex::from_hex("0000000000000000").unwrap())[..]);

		let mut o34 = [255u8; 34];
		f.execute(&i[..], &mut BytesRef::Fixed(&mut o34[..]));
		assert_eq!(&o34[..], &(FromHex::from_hex("000000000000000000000000c08b5542d177ac6686946920409741463a15dddbffff").unwrap())[..]);

		let i_bad = FromHex::from_hex("47173285a8d7341e5e972fc677286384f802f8ef42a5ec5f03bbfa254cb01fad000000000000000000000000000000000000000000000000000000000000001a650acf9d3f5f0a2c799776a1254355d5f4061762a237396a99a0e0e3fc2bcd6729514a0dacb2e623ac4abd157cb18163ff942280db4d5caad66ddf941ba12e03").unwrap();
		let mut o = [255u8; 32];
		f.execute(&i_bad[..], &mut BytesRef::Fixed(&mut o[..]));
		assert_eq!(&o[..], &(FromHex::from_hex("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff").unwrap())[..]);

		let i_bad = FromHex::from_hex("47173285a8d7341e5e972fc677286384f802f8ef42a5ec5f03bbfa254cb01fad000000000000000000000000000000000000000000000000000000000000001b000000000000000000000000000000000000000000000000000000000000001b0000000000000000000000000000000000000000000000000000000000000000").unwrap();
		let mut o = [255u8; 32];
		f.execute(&i_bad[..], &mut BytesRef::Fixed(&mut o[..]));
		assert_eq!(&o[..], &(FromHex::from_hex("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff").unwrap())[..]);

		let i_bad = FromHex::from_hex("47173285a8d7341e5e972fc677286384f802f8ef42a5ec5f03bbfa254cb01fad000000000000000000000000000000000000000000000000000000000000001b0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001b").unwrap();
		let mut o = [255u8; 32];
		f.execute(&i_bad[..], &mut BytesRef::Fixed(&mut o[..]));
		assert_eq!(&o[..], &(FromHex::from_hex("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff").unwrap())[..]);

		let i_bad = FromHex::from_hex("47173285a8d7341e5e972fc677286384f802f8ef42a5ec5f03bbfa254cb01fad000000000000000000000000000000000000000000000000000000000000001bffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff000000000000000000000000000000000000000000000000000000000000001b").unwrap();
		let mut o = [255u8; 32];
		f.execute(&i_bad[..], &mut BytesRef::Fixed(&mut o[..]));
		assert_eq!(&o[..], &(FromHex::from_hex("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff").unwrap())[..]);

		let i_bad = FromHex::from_hex("47173285a8d7341e5e972fc677286384f802f8ef42a5ec5f03bbfa254cb01fad000000000000000000000000000000000000000000000000000000000000001b000000000000000000000000000000000000000000000000000000000000001bffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff").unwrap();
		let mut o = [255u8; 32];
		f.execute(&i_bad[..], &mut BytesRef::Fixed(&mut o[..]));
		assert_eq!(&o[..], &(FromHex::from_hex("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff").unwrap())[..]);

		// TODO: Should this (corrupted version of the above) fail rather than returning some address?
	/*	let i_bad = FromHex::from_hex("48173285a8d7341e5e972fc677286384f802f8ef42a5ec5f03bbfa254cb01fad000000000000000000000000000000000000000000000000000000000000001b650acf9d3f5f0a2c799776a1254355d5f4061762a237396a99a0e0e3fc2bcd6729514a0dacb2e623ac4abd157cb18163ff942280db4d5caad66ddf941ba12e03").unwrap();
		let mut o = [255u8; 32];
		f.execute(&i_bad[..], &mut BytesRef::Fixed(&mut o[..]));
		assert_eq!(&o[..], &(FromHex::from_hex("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff").unwrap())[..]);*/
	}

	#[test]
	#[should_panic]
	fn from_unknown_linear() {
		let _ = ethereum_builtin("foo");
	}

	#[test]
	fn from_named_linear() {
		let pricer = Box::new(Linear { base: 10, word: 20 });
		let b = Builtin {
			pricer: pricer as Box<Pricer>,
			native: ethereum_builtin("identity"),
		};

		assert_eq!(b.cost(0), U256::from(10));
		assert_eq!(b.cost(1), U256::from(30));
		assert_eq!(b.cost(32), U256::from(30));
		assert_eq!(b.cost(33), U256::from(50));

		let i = [0u8, 1, 2, 3];
		let mut o = [255u8; 4];
		b.execute(&i[..], &mut BytesRef::Fixed(&mut o[..]));
		assert_eq!(i, o);
	}

	#[test]
	fn from_json() {
		let b = Builtin::from(ethjson::spec::Builtin {
			name: "identity".to_owned(),
			pricing: ethjson::spec::Pricing::Linear(ethjson::spec::Linear {
				base: 10,
				word: 20,
			})
		});

		assert_eq!(b.cost(0), U256::from(10));
		assert_eq!(b.cost(1), U256::from(30));
		assert_eq!(b.cost(32), U256::from(30));
		assert_eq!(b.cost(33), U256::from(50));

		let i = [0u8, 1, 2, 3];
		let mut o = [255u8; 4];
		b.execute(&i[..], &mut BytesRef::Fixed(&mut o[..]));
		assert_eq!(i, o);
	}
}
