/**
 * Shared configuration and types for E2E tests
 */

import * as path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

export const CONFIG = {
    chainId: 33101,
    rpcUrl: 'https://api.zq2-testnet.zilliqa.com',
    faucetUrl: 'https://faucet.testnet.zilliqa.com/',
    apiBaseUrl: process.env.API_URL || 'https://media.rexpump.fun',
    walletFile: path.join(__dirname, '../logs/wallet.json'),
    imageFile: path.join(__dirname, '../../assets/default_token.webp'),
    bytecodeFile: path.join(__dirname, '../foundry/out/TestMemecoin.sol/TestMemecoin.json'),
};

export const chain = {
    id: CONFIG.chainId,
    name: 'Zilliqa Testnet',
    nativeCurrency: { name: 'ZIL', symbol: 'ZIL', decimals: 18 },
    rpcUrls: {
        default: { http: [CONFIG.rpcUrl] },
    },
} as const;

export interface MetadataResponse {
    chain_id: number;
    token_address: string;
    description: string;
    social_networks: Array<{ name: string; link: string }>;
    image_light_url: string | null;
    image_dark_url: string | null;
    created_at: string;
    updated_at: string;
}

export interface ApiResponse<T> {
    status: number;
    ok: boolean;
    data: T | null;
    error: { error: string; message: string } | null;
}

export interface TestResult {
    name: string;
    passed: boolean;
    error?: string;
    duration: number;
}

// Utility functions
export function sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
}

export function log(message: string, ...args: unknown[]): void {
    console.log(`[${new Date().toISOString()}] ${message}`, ...args);
}

export function logSection(title: string): void {
    console.log('\n' + '='.repeat(60));
    console.log(title);
    console.log('='.repeat(60));
}
