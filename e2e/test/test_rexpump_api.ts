/**
 * RexPump Metadata API E2E Test Suite - Main Entry Point
 * 
 * Runs all tests in sequence. For testing individual modules:
 *   bun run test:setup    - Wallet + faucet + deploy
 *   bun run test:crud     - Basic CRUD tests
 *   bun run test:images   - Image upload/replace tests  
 *   bun run test:ratelimit - Rate limit tests (takes 60s+)
 */

import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';

import { CONFIG, logSection, log, sleep } from './config';
import { loadOrCreateWallet } from './wallet';
import { ensureBalance } from './faucet';
import { deployTestMemecoin } from './deploy';
import { postMetadata, getMetadata } from './api';
import { runTest, printSummary, resetResults } from './runner';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

// Shared state between tests
interface TestContext {
    tokenAddress: string;
    testImage: Buffer | null;
    imageLightUrl: string | null;
}

// ============================================================================
// Test Suites
// ============================================================================

async function testSetup(ctx: TestContext) {
    logSection('Setup: Deploy Token');

    const { account, isNew } = loadOrCreateWallet();
    await ensureBalance(account.address, isNew);

    ctx.tokenAddress = await deployTestMemecoin(account);
    ctx.testImage = fs.existsSync(CONFIG.imageFile)
        ? fs.readFileSync(CONFIG.imageFile)
        : null;

    if (!ctx.testImage) {
        log('Warning: Test image not found at', CONFIG.imageFile);
    }

    return account;
}

async function testCrud(ctx: TestContext, account: ReturnType<typeof loadOrCreateWallet>['account']) {
    logSection('CRUD Tests');

    await runTest('Create metadata (JSON only)', async () => {
        const result = await postMetadata(account, ctx.tokenAddress, {
            description: 'Test token description',
            socialNetworks: [{ name: 'telegram', link: 'https://t.me/test' }],
        });

        if (!result.ok) {
            throw new Error(`Expected 200, got ${result.status}: ${JSON.stringify(result.error)}`);
        }

        if (result.data?.description !== 'Test token description') {
            throw new Error(`Description mismatch: ${result.data?.description}`);
        }
    });

    await runTest('Get metadata', async () => {
        const result = await getMetadata(ctx.tokenAddress);

        if (!result.ok) {
            throw new Error(`Expected 200, got ${result.status}`);
        }

        if (result.data?.description !== 'Test token description') {
            throw new Error(`Description mismatch: ${result.data?.description}`);
        }
    });
}

async function testImages(ctx: TestContext, account: ReturnType<typeof loadOrCreateWallet>['account']) {
    logSection('Image Tests');

    // Need to wait for rate limit
    await runTest('Wait 60s for cooldown', async () => {
        log('Sleeping 61 seconds for rate limit cooldown...');
        await sleep(61000);
    });

    await runTest('Add image_light', async () => {
        if (!ctx.testImage) throw new Error('No test image available');

        const result = await postMetadata(account, ctx.tokenAddress, {
            imageLight: ctx.testImage,
        });

        if (!result.ok) {
            throw new Error(`Expected 200, got ${result.status}: ${JSON.stringify(result.error)}`);
        }

        if (!result.data?.image_light_url) {
            throw new Error('image_light_url should be set');
        }

        ctx.imageLightUrl = result.data.image_light_url;
        log('image_light_url:', ctx.imageLightUrl);
    });

    await runTest('Verify image persisted', async () => {
        const result = await getMetadata(ctx.tokenAddress);

        if (!result.ok || !result.data?.image_light_url) {
            throw new Error('image_light_url should be set after upload');
        }
    });
}

async function testRateLimit(ctx: TestContext, account: ReturnType<typeof loadOrCreateWallet>['account']) {
    logSection('Rate Limit Tests');

    await runTest('Rate limit check (immediate retry)', async () => {
        const result = await postMetadata(account, ctx.tokenAddress, {
            description: 'Should fail due to rate limit',
        });

        if (result.status !== 429) {
            throw new Error(`Expected 429, got ${result.status}`);
        }

        log('Correctly rejected with 429');
    });

    await runTest('Wait 60s for cooldown', async () => {
        log('Sleeping 61 seconds...');
        await sleep(61000);
    });

    await runTest('Update description only (images unchanged)', async () => {
        const result = await postMetadata(account, ctx.tokenAddress, {
            description: 'Updated description',
        });

        if (!result.ok) {
            throw new Error(`Expected 200, got ${result.status}: ${JSON.stringify(result.error)}`);
        }

        // Check image is still there
        if (ctx.imageLightUrl && result.data?.image_light_url !== ctx.imageLightUrl) {
            throw new Error(`image_light_url changed unexpectedly`);
        }

        if (result.data?.description !== 'Updated description') {
            throw new Error(`Description not updated`);
        }
    });
}

// ============================================================================
// Main Entry Point
// ============================================================================

async function main() {
    logSection('RexPump Metadata API E2E Tests');
    log('API Base URL:', CONFIG.apiBaseUrl);
    log('Chain ID:', CONFIG.chainId);
    log('RPC URL:', CONFIG.rpcUrl);

    const ctx: TestContext = {
        tokenAddress: '',
        testImage: null,
        imageLightUrl: null,
    };

    resetResults();

    // Run all test suites
    const account = await testSetup(ctx);
    await testCrud(ctx, account);
    await testImages(ctx, account);
    await testRateLimit(ctx, account);

    // Print summary
    const { passed, failed } = printSummary();

    // Save results
    const logFile = path.join(__dirname, `../logs/run_${Date.now()}.json`);
    try {
        fs.writeFileSync(logFile, JSON.stringify({
            timestamp: new Date().toISOString(),
            wallet: account.address,
            tokenAddress: ctx.tokenAddress,
            config: {
                apiBaseUrl: CONFIG.apiBaseUrl,
                chainId: CONFIG.chainId,
            },
        }, null, 2));
        log('Results saved to:', logFile);
    } catch {
        // logs dir might be gitignored, that's ok
    }

    process.exit(failed > 0 ? 1 : 0);
}

main().catch((error) => {
    console.error('Fatal error:', error);
    process.exit(1);
});
