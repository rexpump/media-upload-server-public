// SPDX-License-Identifier: MIT
pragma solidity ^0.8.26;

import {Script, console} from "forge-std/Script.sol";
import {TestMemecoin} from "../src/TestMemecoin.sol";

contract DeployScript is Script {
    function run() external returns (address tokenAddress) {
        // Generate random name/symbol for each deploy
        string memory name = string.concat("TestToken_", vm.toString(block.timestamp));
        string memory symbol = string.concat("TT", vm.toString(block.timestamp % 10000));
        
        vm.startBroadcast();
        
        TestMemecoin token = new TestMemecoin(name, symbol);
        tokenAddress = address(token);
        
        console.log("=== Deployed TestMemecoin ===");
        console.log("Address:", tokenAddress);
        console.log("Name:", name);
        console.log("Symbol:", symbol);
        console.log("Creator:", token.creator());
        
        vm.stopBroadcast();
        
        return tokenAddress;
    }
}
