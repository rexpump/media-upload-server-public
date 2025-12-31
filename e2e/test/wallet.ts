/**
 * Wallet management utilities
 */

import * as fs from 'fs';
import * as path from 'path';
import { type PrivateKeyAccount } from 'viem';
import { privateKeyToAccount, generatePrivateKey } from 'viem/accounts';
import { CONFIG, log } from './config';

export interface WalletData {
    address: string;
    privateKey: string;
    createdAt: string;
}

export function loadOrCreateWallet(): { account: PrivateKeyAccount; isNew: boolean } {
    // Try to load existing wallet
    try {
        if (fs.existsSync(CONFIG.walletFile)) {
            const data: WalletData = JSON.parse(fs.readFileSync(CONFIG.walletFile, 'utf-8'));
            log('Loaded existing wallet:', data.address);
            return {
                account: privateKeyToAccount(data.privateKey as `0x${string}`),
                isNew: false,
            };
        }
    } catch (e) {
        log('Failed to load wallet, generating new one');
    }

    // Generate new wallet
    const privateKey = generatePrivateKey();
    const account = privateKeyToAccount(privateKey);

    const data: WalletData = {
        address: account.address,
        privateKey,
        createdAt: new Date().toISOString(),
    };

    // Ensure logs directory exists
    const logsDir = path.dirname(CONFIG.walletFile);
    if (!fs.existsSync(logsDir)) {
        fs.mkdirSync(logsDir, { recursive: true });
    }

    fs.writeFileSync(CONFIG.walletFile, JSON.stringify(data, null, 2));
    log('Generated new wallet:', account.address);
    log('Private key saved to:', CONFIG.walletFile);

    return { account, isNew: true };
}
