/**
 * Test runner utilities
 */

import { type TestResult, log } from './config';

const testResults: TestResult[] = [];

export async function runTest(name: string, fn: () => Promise<void>): Promise<boolean> {
    log(`\n>>> Test: ${name}`);
    const start = Date.now();

    try {
        await fn();
        const duration = Date.now() - start;
        testResults.push({ name, passed: true, duration });
        log(`✓ PASSED (${duration}ms)`);
        return true;
    } catch (error) {
        const duration = Date.now() - start;
        const message = error instanceof Error ? error.message : String(error);
        testResults.push({ name, passed: false, error: message, duration });
        log(`✗ FAILED: ${message}`);
        return false;
    }
}

export function getResults(): TestResult[] {
    return testResults;
}

export function printSummary(): { passed: number; failed: number } {
    let passed = 0;
    let failed = 0;

    console.log('\n' + '='.repeat(60));
    console.log('Test Results Summary');
    console.log('='.repeat(60));

    for (const result of testResults) {
        const status = result.passed ? '✓' : '✗';
        console.log(`${status} ${result.name} (${result.duration}ms)`);
        if (result.passed) passed++;
        else {
            failed++;
            if (result.error) console.log(`    Error: ${result.error}`);
        }
    }

    console.log(`\n${passed} passed, ${failed} failed`);

    return { passed, failed };
}

export function resetResults(): void {
    testResults.length = 0;
}
