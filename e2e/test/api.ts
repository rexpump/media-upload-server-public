/**
 * RexPump Metadata API client
 */

import { type PrivateKeyAccount } from 'viem';
import { CONFIG, type MetadataResponse, type ApiResponse, log } from './config';

function buildSignMessage(chainId: number, tokenAddress: string, timestamp: number): string {
    return `RexPump Metadata Update\nChain: ${chainId}\nToken: ${tokenAddress.toLowerCase()}\nTimestamp: ${timestamp}`;
}

export interface PostMetadataOptions {
    description?: string;
    socialNetworks?: Array<{ name: string; link: string }>;
    imageLight?: Buffer;
    imageDark?: Buffer;
}

export async function postMetadata(
    account: PrivateKeyAccount,
    tokenAddress: string,
    options: PostMetadataOptions
): Promise<ApiResponse<MetadataResponse>> {
    const timestamp = Math.floor(Date.now() / 1000);
    const message = buildSignMessage(CONFIG.chainId, tokenAddress, timestamp);
    const signature = await account.signMessage({ message });

    log('POST /api/rexpump/metadata');
    log('  Token:', tokenAddress);
    log('  Owner:', account.address);

    const formData = new FormData();
    formData.append('chain_id', CONFIG.chainId.toString());
    formData.append('token_address', tokenAddress);
    formData.append('token_owner', account.address);
    formData.append('timestamp', timestamp.toString());
    formData.append('signature', signature);

    if (options.description !== undefined || options.socialNetworks !== undefined) {
        formData.append('metadata', JSON.stringify({
            description: options.description || '',
            social_networks: options.socialNetworks || [],
        }));
    }

    if (options.imageLight) {
        formData.append('image_light', new Blob([options.imageLight]), 'light.webp');
    }

    if (options.imageDark) {
        formData.append('image_dark', new Blob([options.imageDark]), 'dark.webp');
    }

    const response = await fetch(`${CONFIG.apiBaseUrl}/api/rexpump/metadata`, {
        method: 'POST',
        body: formData,
    });

    const text = await response.text();

    try {
        const json = JSON.parse(text);
        log('  Response:', response.status, response.ok ? 'OK' : json.error);
        return {
            status: response.status,
            ok: response.ok,
            data: response.ok ? json : null,
            error: response.ok ? null : json,
        };
    } catch {
        return {
            status: response.status,
            ok: response.ok,
            data: null,
            error: { error: 'parse_error', message: text },
        };
    }
}

export async function getMetadata(tokenAddress: string): Promise<ApiResponse<MetadataResponse>> {
    log('GET /api/rexpump/metadata/', CONFIG.chainId, '/', tokenAddress);

    const response = await fetch(
        `${CONFIG.apiBaseUrl}/api/rexpump/metadata/${CONFIG.chainId}/${tokenAddress}`
    );

    const text = await response.text();

    try {
        const json = JSON.parse(text);
        log('  Response:', response.status, response.ok ? 'OK' : json.error);
        return {
            status: response.status,
            ok: response.ok,
            data: response.ok ? json : null,
            error: response.ok ? null : json,
        };
    } catch {
        return {
            status: response.status,
            ok: response.ok,
            data: null,
            error: { error: 'parse_error', message: text },
        };
    }
}
