// SPDX-License-Identifier: MIT
pragma solidity ^0.8.26;

import {Script, console} from "forge-std/Script.sol";
import {TestMemecoin} from "../src/TestMemecoin.sol";

contract DeployScript is Script {
    function run() external returns (address tokenAddress) {
        vm.startBroadcast();
        
        // TestMemecoin has hardcoded name/symbol for Zilliqa EVM compatibility
        TestMemecoin token = new TestMemecoin();
        tokenAddress = address(token);
        
        console.log("=== Deployed TestMemecoin ===");
        console.log("Address:", tokenAddress);
        console.log("Name:", token.name());
        console.log("Symbol:", token.symbol());
        console.log("Creator:", token.creator());
        
        vm.stopBroadcast();
        
        return tokenAddress;
    }
}
