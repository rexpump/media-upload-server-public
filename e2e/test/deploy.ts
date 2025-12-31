/**
 * TestMemecoin deployment utilities
 * 
 * Note: Zilliqa EVM only supports legacy transactions (type 0)
 */

import * as fs from 'fs';
import {
    createWalletClient,
    createPublicClient,
    http,
    type PrivateKeyAccount,
    type Address,
} from 'viem';
import { CONFIG, chain, log, logSection } from './config';

export async function deployTestMemecoin(account: PrivateKeyAccount): Promise<Address> {
    logSection('Deploying TestMemecoin');

    const publicClient = createPublicClient({
        chain,
        transport: http(CONFIG.rpcUrl),
    });

    const walletClient = createWalletClient({
        chain,
        transport: http(CONFIG.rpcUrl),
    });

    // Load compiled bytecode
    if (!fs.existsSync(CONFIG.bytecodeFile)) {
        throw new Error(
            `Bytecode not found at ${CONFIG.bytecodeFile}\n` +
            'Run: cd e2e/foundry && forge build --skip script'
        );
    }

    const artifact = JSON.parse(fs.readFileSync(CONFIG.bytecodeFile, 'utf-8'));
    const bytecode = artifact.bytecode.object as `0x${string}`;
    const abi = artifact.abi;

    log('Deploying TestMemecoin (no constructor args, legacy tx)...');

    try {
        // Get current gas price for legacy tx
        const gasPrice = await publicClient.getGasPrice();
        log('Gas price:', gasPrice.toString());

        // Deploy contract with legacy transaction for Zilliqa EVM
        const hash = await walletClient.sendTransaction({
            account,
            chain,
            data: bytecode,
            gas: 500000n,
            gasPrice, // Legacy tx uses gasPrice instead of maxFeePerGas
        });

        log('Deploy tx hash:', hash);

        // Wait for confirmation
        const receipt = await publicClient.waitForTransactionReceipt({ hash });

        log('Receipt status:', receipt.status);
        log('Gas used:', receipt.gasUsed.toString());

        if (!receipt.contractAddress) {
            throw new Error(`Contract deployment failed - status: ${receipt.status}, no address in receipt`);
        }

        const tokenAddress = receipt.contractAddress;
        log('TestMemecoin deployed at:', tokenAddress);

        // Verify creator()
        const creator = await publicClient.readContract({
            address: tokenAddress,
            abi,
            functionName: 'creator',
        });

        log('creator() returns:', creator);

        if ((creator as string).toLowerCase() !== account.address.toLowerCase()) {
            throw new Error(`creator() mismatch: expected ${account.address}, got ${creator}`);
        }

        return tokenAddress;
    } catch (error: any) {
        log('Deploy error type:', error?.name);
        log('Deploy error message:', error?.message);
        log('Deploy error cause:', JSON.stringify(error?.cause, null, 2));
        throw error;
    }
}
