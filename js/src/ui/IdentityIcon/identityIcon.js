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

import React, { Component, PropTypes } from 'react';
import { connect } from 'react-redux';
import { bindActionCreators } from 'redux';
import ContractIcon from 'material-ui/svg-icons/action/code';

import styles from './identityIcon.css';

class IdentityIcon extends Component {
  static contextTypes = {
    api: PropTypes.object.isRequired
  }

  static propTypes = {
    address: PropTypes.string,
    button: PropTypes.bool,
    className: PropTypes.string,
    center: PropTypes.bool,
    padded: PropTypes.bool,
    inline: PropTypes.bool,
    tiny: PropTypes.bool,
    images: PropTypes.object.isRequired
  }

  state = {
    iconsrc: ''
  }

  componentDidMount () {
    this.updateIcon(this.props.address, this.props.images);
  }

  componentWillReceiveProps (newProps) {
    const sameAddress = newProps.address === this.props.address;
    const sameImages = Object.keys(newProps.images).length === Object.keys(this.props.images).length;

    if (sameAddress && sameImages) {
      return;
    }

    this.updateIcon(newProps.address, newProps.images);
  }

  updateIcon (_address, images) {
    const { api } = this.context;
    const { button, inline, tiny } = this.props;

    if (images[_address]) {
      this.setState({ iconsrc: `${api.dappsUrl}${images[_address]}` });
      return;
    }

    let scale = 7;
    if (tiny) {
      scale = 2;
    } else if (button) {
      scale = 3;
    } else if (inline) {
      scale = 4;
    }

    this.setState({
      iconsrc: api.util.createIdentityImg(_address, scale)
    });
  }

  render () {
    const { address, button, className, center, inline, padded, tiny } = this.props;
    const { iconsrc } = this.state;
    const classes = [
      styles.icon,
      tiny ? styles.tiny : '',
      button ? styles.button : '',
      center ? styles.center : styles.left,
      inline ? styles.inline : '',
      padded ? styles.padded : '',
      className
    ].join(' ');

    let size = '56px';
    if (tiny) {
      size = '16px';
    } else if (button) {
      size = '24px';
    } else if (inline) {
      size = '32px';
    }

    if (!address) {
      return (
        <ContractIcon
          className={ classes }
          style={ { width: size, height: size, background: '#eee' } } />
      );
    }

    return (
      <img
        className={ classes }
        src={ iconsrc }
        width={ size }
        height={ size } />
    );
  }
}

function mapStateToProps (state) {
  const { images } = state;

  return { images };
}

function mapDispatchToProps (dispatch) {
  return bindActionCreators({}, dispatch);
}

export default connect(
  mapStateToProps,
  mapDispatchToProps
)(IdentityIcon);
