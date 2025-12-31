/**
 * Zilliqa testnet faucet integration
 */

import { createPublicClient, http, formatEther, type Address } from 'viem';
import { CONFIG, chain, log, sleep } from './config';

export async function requestFromFaucet(address: string): Promise<boolean> {
    log('Requesting testnet ZIL from faucet:', CONFIG.faucetUrl);

    try {
        // Zilliqa testnet faucet uses form-urlencoded POST
        const response = await fetch(CONFIG.faucetUrl, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/x-www-form-urlencoded',
                'User-Agent': 'RexPumpE2ETest/1.0',
            },
            body: new URLSearchParams({ address }).toString(),
        });

        const text = await response.text();
        log('Faucet response:', response.status, text.substring(0, 200));

        if (response.ok) {
            log('Faucet request successful');
            return true;
        }

        return false;
    } catch (error) {
        log('Faucet error:', error);
        return false;
    }
}

export async function waitForBalance(
    address: Address,
    timeoutMs = 120000
): Promise<bigint> {
    log('Waiting for balance...');

    const publicClient = createPublicClient({
        chain,
        transport: http(CONFIG.rpcUrl),
    });

    const startTime = Date.now();

    while (Date.now() - startTime < timeoutMs) {
        const balance = await publicClient.getBalance({ address });

        if (balance > 0n) {
            log('Balance received:', formatEther(balance), 'ZIL');
            return balance;
        }

        log('Balance still 0, waiting 5s...');
        await sleep(5000);
    }

    throw new Error('Timeout waiting for balance');
}

export async function ensureBalance(address: Address, _isNew: boolean): Promise<bigint> {
    const publicClient = createPublicClient({
        chain,
        transport: http(CONFIG.rpcUrl),
    });

    // Always request from faucet on each test run
    await requestFromFaucet(address);

    // Wait a bit for faucet tx to be mined
    await sleep(3000);

    let balance = await publicClient.getBalance({ address });
    log('Current balance:', formatEther(balance), 'ZIL');

    if (balance === 0n) {
        // Wait longer for faucet
        balance = await waitForBalance(address, 60000);
    }

    return balance;
}
