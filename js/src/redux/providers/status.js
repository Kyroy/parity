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

import { statusBlockNumber, statusCollection, statusLogs } from './statusActions';

export default class Status {
  constructor (store, api) {
    this._api = api;
    this._store = store;
  }

  start () {
    this._subscribeBlockNumber();
    this._pollStatus();
    this._pollLogs();
  }

  _subscribeBlockNumber () {
    this._api.subscribe('eth_blockNumber', (error, blockNumber) => {
      if (error) {
        return;
      }

      this._store.dispatch(statusBlockNumber(blockNumber));
    });
  }

  _pollStatus = () => {
    const nextTimeout = (timeout = 1000) => setTimeout(this._pollStatus, timeout);

    Promise
      .all([
        this._api.web3.clientVersion(),
        this._api.eth.coinbase(),
        this._api.ethcore.defaultExtraData(),
        this._api.ethcore.extraData(),
        this._api.ethcore.gasFloorTarget(),
        this._api.eth.hashrate(),
        this._api.ethcore.minGasPrice(),
        this._api.ethcore.netChain(),
        this._api.ethcore.netPeers(),
        this._api.ethcore.netPort(),
        this._api.ethcore.nodeName(),
        this._api.ethcore.rpcSettings(),
        this._api.eth.syncing()
      ])
      .then(([clientVersion, coinbase, defaultExtraData, extraData, gasFloorTarget, hashrate, minGasPrice, netChain, netPeers, netPort, nodeName, rpcSettings, syncing]) => {
        const isTest = netChain === 'morden' || netChain === 'testnet';

        nextTimeout();
        this._store.dispatch(statusCollection({
          clientVersion,
          coinbase,
          defaultExtraData,
          extraData,
          gasFloorTarget,
          hashrate,
          minGasPrice,
          netChain,
          netPeers,
          netPort,
          nodeName,
          rpcSettings,
          syncing,
          isTest
        }));
      })
      .catch((error) => {
        console.error('_pollStatus', error);
        nextTimeout();
      });
  }

  _pollLogs = () => {
    const nextTimeout = (timeout = 1000) => setTimeout(this._pollLogs, timeout);
    const { devLogsEnabled } = this._store.getState().nodeStatus;

    if (!devLogsEnabled) {
      nextTimeout();
      return;
    }

    Promise
      .all([
        this._api.ethcore.devLogs(),
        this._api.ethcore.devLogsLevels()
      ])
      .then(([devLogs, devLogsLevels]) => {
        nextTimeout();
        this._store.dispatch(statusLogs({
          devLogs: devLogs.slice(-1024),
          devLogsLevels
        }));
      })
      .catch((error) => {
        console.error('_pollLogs', error);
        nextTimeout();
      });
  }
}
