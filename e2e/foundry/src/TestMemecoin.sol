// SPDX-License-Identifier: MIT
pragma solidity ^0.8.26;

/**
 * @title TestMemecoin
 * @notice Minimal contract with creator() for testing RexPump metadata API
 * @dev Compiled with evm_version = shanghai for Zilliqa EVM
 *      Using hardcoded name/symbol for faster deployment
 */
contract TestMemecoin {
    address public creator;
    string public name;
    string public symbol;
    
    constructor() {
        creator = msg.sender;
        name = "TestToken";
        symbol = "TT";
    }
}
