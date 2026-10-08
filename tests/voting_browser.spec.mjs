import { test, expect } from '@playwright/test';
import { PNG } from 'pngjs';
import { readFile } from 'node:fs/promises';
async function fixture(page) {
    await page.route('**/pkg/zcash_wallet_wasm.js', route => route.fulfill({ path: 'tests/voting_fake_wasm.mjs', contentType: 'text/javascript' }));
    await page.goto('/test_voting.html');
    await expect(page.locator('#initialize')).toBeEnabled();
}
test('partial delegation remains explicitly continuable after keyless Resume and reload', async ({ page }) => {
    const source = await readFile('tests/voting_fake_wasm.mjs', 'utf8');
    const continuation = `
export class WebVotingSession extends BaseSession {
  async roundStatus() {
    const stage=this.bytes?.[0];
    return {roundId:'accepted',phase:stage===33?'delegation_confirmed':stage>=31?'delegation_pending':'round_prepared',completedBundles:stage>=32?1:0,totalBundles:2,confirmedVotes:0,totalVotes:0,confirmedHelpers:0,requiredHelpers:0,nextAction:stage===33?'submit_ballot':'submit_delegation'};
  }
  async submitDelegation(id,key,fingerprint) {
    key.consume();fingerprint.consume();
    this.bytes=new Uint8Array([this.bytes?.[0]===32?33:31]);
    await this.persist(this.bytes,++this.revision);
  }
  async resumeRound() {this.bytes=new Uint8Array([32]);await this.persist(this.bytes,++this.revision);}
}`;
    await page.route('**/pkg/zcash_wallet_wasm.js', route => route.fulfill({ contentType: 'text/javascript', body: source.replace('export class WebVotingSession', 'class BaseSession') + continuation }));
    await page.goto('/test_voting.html');
    await expect(page.locator('#initialize')).toBeEnabled();
    await page.locator('#mnemonic').fill(await page.evaluate(() => crypto.randomUUID()));
    await page.locator('#initialize').click();
    await page.locator('#discover').click();
    await page.getByRole('button', { name: 'Community round' }).click();
    await page.locator('#mnemonic').fill(await page.evaluate(() => crypto.randomUUID()));
    page.once('dialog', dialog => dialog.accept());
    await page.locator('#delegate').click();
    await expect(page.locator('#progress')).toContainText('delegation_pending');
    await expect(page.locator('#mnemonic')).toHaveValue('');
    await page.locator('#resume').click();
    await expect(page.locator('#progress')).toContainText('Bundles 1/2');
    await expect(page.locator('#delegate')).toBeEnabled();
    await expect(page.locator('#vote')).toBeDisabled();
    await page.reload();
    await expect(page.locator('#wallet-status')).toContainText('Wallet restored');
    await page.locator('#discover').click();
    await page.getByRole('button', { name: 'Community round' }).click();
    await expect(page.locator('#delegate')).toBeEnabled();
    await page.locator('#mnemonic').fill(await page.evaluate(() => crypto.randomUUID()));
    page.once('dialog', dialog => dialog.accept());
    await page.locator('#delegate').click();
    await expect(page.locator('#progress')).toContainText('delegation_confirmed');
    await expect(page.locator('#vote')).toBeEnabled();
});
test('persistence failure freezes mutation until reload and does not show durable success', async ({ page }) => {
    const source = await readFile('tests/voting_fake_wasm.mjs', 'utf8');
    await page.route('**/pkg/zcash_wallet_wasm.js', route => route.fulfill({ contentType: 'text/javascript', body: source.replace('await this.persist(this.bytes,++this.revision); return', "throw Object.assign(new Error('checkpoint rejected'),{name:'PERSISTENCE_FAILED'}); return") }));
    await page.goto('/test_voting.html');
    await expect(page.locator('#initialize')).toBeEnabled();
    await page.locator('#mnemonic').fill(await page.evaluate(() => crypto.randomUUID()));
    await page.locator('#initialize').click();
    await expect(page.locator('#wallet-status')).toContainText('Wallet ready');
    await page.locator('#discover').click();
    await page.getByRole('button', { name: 'Community round' }).click();
    await page.locator('#eligibility').click();
    await page.locator('#prepare').click();
    await expect(page.locator('#operation-status')).toContainText('PERSISTENCE_FAILED: reload required');
    await expect(page.locator('[data-mutation]:enabled')).toHaveCount(0);
    await expect(page.locator('#progress')).not.toContainText('round_prepared');
});
test('delegation consumes temporary keys and ballot requires explicit review including skip', async ({ page }) => {
    await fixture(page);
    await page.locator('#mnemonic').fill(await page.evaluate(() => crypto.randomUUID()));
    await page.locator('#initialize').click();
    await expect(page.locator('#wallet-status')).toContainText('Wallet ready');
    await page.locator('#discover').click();
    await page.getByRole('button', { name: 'Community round' }).click();
    await page.locator('#eligibility').click();
    await page.locator('#prepare').click();
    await expect(page.locator('#delegate')).toBeEnabled();
    await page.locator('#mnemonic').fill(await page.evaluate(() => crypto.randomUUID()));
    page.once('dialog', dialog => dialog.accept());
    await page.locator('#delegate').click();
    await expect(page.locator('#progress')).toContainText('delegation_confirmed');
    await expect(page.locator('#operation-status')).toBeEmpty();
    await expect(page.locator('#mnemonic')).toHaveValue('');
    await page.locator('#vote').click();
    await expect(page.locator('#operation-status')).toHaveText('BALLOT_INCOMPLETE');
    await page.getByRole('radio', { name: 'Skip' }).check();
    page.once('dialog', dialog => { expect(dialog.message()).toContain('Community allocation: Skip'); return dialog.dismiss(); });
    await page.locator('#vote').click();
    await expect(page.locator('#progress')).toContainText('delegation_confirmed');
    page.once('dialog', dialog => dialog.accept());
    await page.locator('#vote').click();
    await expect(page.locator('#progress')).toContainText('shares_confirmed');
});
test('wallet initialization, sync, selection and reload use durable bytes and clear needs confirmation', async ({ page }) => {
    await fixture(page);
    // Generated runtime credential never appears in source or assertion output.
    await page.locator('#mnemonic').fill(await page.evaluate(() => crypto.randomUUID()));
    await page.locator('#initialize').click();
    await expect(page.locator('[data-mutation]:enabled')).toHaveCount(0);
    await expect(page.locator('#wallet-status')).toContainText('Wallet ready');
    await expect(page.locator('#wallet-status')).toContainText('Scanned height: unavailable');
    await expect(page.locator('#wallet-status')).toContainText('Balance: unavailable');
    await expect(page.locator('#mnemonic')).toHaveValue('');
    await expect(page.locator('#passphrase')).toHaveValue('');
    await page.locator('#sync').click();
    await expect(page.locator('#wallet-status')).toContainText('Sync complete');
    await expect(page.locator('#wallet-status')).toContainText('Scanned height: 3771700');
    await expect(page.locator('#wallet-status')).toContainText('Balance: 120000000 zats');
    await expect(page.locator('#wallet-status')).toContainText('Spendable: 100000000 zats');
    await expect(page.locator('#wallet-status')).toContainText('Chain tip: 3771800');
    await page.locator('#discover').click();
    await expect(page.locator('#rejected')).toContainText('ROUND_UNAUTHENTICATED');
    await expect(page.locator('#rejected button')).toHaveCount(0);
    await page.getByRole('button', { name: 'Community round' }).click();
    await expect(page.getByRole('radio', { name: 'Skip' })).toBeVisible();
    await page.locator('#eligibility').click();
    await expect(page.locator('#eligibility-status')).toContainText('100000000');
    await page.locator('#prepare').click();
    await expect(page.locator('#progress')).toContainText('round_prepared');
    await page.screenshot({ path: `test-results/voting-ballot-${test.info().project.name}.png`, fullPage: true });
    const stored = await page.evaluate(async () => {
        const { VotingStorage } = await import('/voting_storage.mjs');
        const db = await VotingStorage.open();
        const wallet = await db.read('wallet'), voting = await db.read('voting:test:fixture-owner');
        return { wallet: [...new Uint8Array(wallet.bytes)], voting: [...new Uint8Array(voting.bytes)], keys: Object.keys(wallet).sort(), local: localStorage.length };
    });
    expect(stored).toEqual({ wallet: [13, 14], voting: [21, 22], keys: ['accountId', 'accountIndex', 'birthday', 'bytes', 'key', 'lwd'], local: 0 });
    await page.reload();
    await expect(page.locator('#wallet-status')).toContainText('Wallet restored');
    await expect(page.locator('#wallet-status')).toContainText('Scanned height: 3771700');
    await page.locator('#wallet-refresh').click();
    await expect(page.locator('#wallet-status')).toContainText('Balance: 120000000 zats');
    await expect(page.locator('#mnemonic')).toHaveValue('');
    await page.locator('#discover').click();
    await page.getByRole('button', { name: 'Community round' }).click();
    await expect(page.locator('#progress')).toContainText('round_prepared');
    page.once('dialog', dialog => dialog.dismiss());
    await page.locator('#clear').click();
    await expect(page.locator('#wallet-status')).toContainText('Wallet status');
    page.once('dialog', dialog => dialog.accept());
    await page.locator('#clear').click();
    await expect(page.locator('#wallet-status')).toContainText('Local state cleared');
    await page.reload();
    await expect(page.locator('#initialize')).toBeEnabled();
    await expect(page.locator('#sync')).toBeDisabled();
});
test('defaults are usable and secret input is runtime-only', async ({ page }) => {
    await page.goto('/test_voting.html');
    await expect(page.locator('#initialize')).toBeEnabled({ timeout: 50000 });
    await expect(page.getByRole('heading', { name: 'Zcash voting' })).toBeVisible();
    await expect(page.locator('#lwd')).toHaveValue('https://zcash-testnet.chainsafe.dev');
    await expect(page.locator('#birthday')).toHaveValue('3771641');
    await expect(page.locator('#mnemonic')).toHaveAttribute('type', 'password');
    await expect(page.locator('#passphrase')).toHaveAttribute('type', 'password');
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    const controls = await page.locator('input, button, select').evaluateAll(nodes => nodes
        .filter(node => node.getClientRects().length)
        .map(node => { const r = node.getBoundingClientRect(); return { x: r.x, y: r.y, right: r.right, bottom: r.bottom }; }));
    for (let i = 0; i < controls.length; i++)
        for (let j = i + 1; j < controls.length; j++) {
            const a = controls[i], b = controls[j];
            expect(a.right <= b.x || b.right <= a.x || a.bottom <= b.y || b.bottom <= a.y).toBe(true);
        }
    const screenshot = await page.screenshot({ path: `test-results/voting-${test.info().project.name}.png`, fullPage: true });
    const png = PNG.sync.read(screenshot);
    const colors = new Set();
    for (let i = 0; i < png.data.length; i += 400)
        colors.add(png.data.subarray(i, i + 3).toString('hex'));
    expect(colors.size).toBeGreaterThan(20);
});
test('IndexedDB checkpoints reject stale and competing state, restore offsets, and clear atomically', async ({ page }) => {
    await page.goto('/test_voting.html');
    const result = await page.evaluate(async () => {
        const { VotingStorage } = await import('/voting_storage.mjs');
        const first = await VotingStorage.open();
        await first.claim();
        const checkpoint = await first.bindSession('owner-fixture');
        await checkpoint(new Uint8Array([7, 8]), 2n);
        let stale = false, competing = false, poisoned = false;
        try {
            await checkpoint(new Uint8Array([1]), 1n);
        }
        catch {
            stale = true;
        }
        try {
            await checkpoint(new Uint8Array([2]), 3n);
        }
        catch {
            poisoned = true;
        }
        const second = await VotingStorage.open();
        try {
            await second.claim();
        }
        catch {
            competing = true;
        }
        const saved = await first.read('owner-fixture');
        await first.close();
        await second.claim();
        const reopened = await second.bindSession('owner-fixture');
        await reopened(new Uint8Array([9]), 1n);
        const updated = await second.read('owner-fixture');
        await second.clear();
        let afterClear = false;
        try {
            await reopened(new Uint8Array([10]), 2n);
        }
        catch {
            afterClear = true;
        }
        const gone = await second.read('owner-fixture');
        await second.close();
        return { stale, competing, poisoned, saved: [...new Uint8Array(saved.bytes)], revision: updated.revision, updated: [...new Uint8Array(updated.bytes)], afterClear, gone };
    });
    expect(result).toEqual({ stale: true, competing: true, poisoned: true, saved: [7, 8], revision: '3', updated: [9], afterClear: true, gone: undefined });
});
test('origin ownership survives a competing tab and clear invalidates an in-flight checkpoint', async ({ page, context }) => {
    await page.goto('/');
    await page.evaluate(async () => { const { VotingStorage } = await import('/voting_storage.mjs'); window.owner = await VotingStorage.open(); await window.owner.claim(); });
    const other = await context.newPage();
    await other.goto('/');
    expect(await other.evaluate(async () => { const { VotingStorage } = await import('/voting_storage.mjs'); window.owner = await VotingStorage.open(); try {
        await window.owner.claim();
        return false;
    }
    catch {
        return true;
    } })).toBe(true);
    await page.close();
    await expect.poll(() => other.evaluate(async () => (await navigator.locks.query()).held.length)).toBe(0);
    await other.reload();
    const result = await other.evaluate(async () => {
        const { VotingStorage } = await import('/voting_storage.mjs');
        window.owner = await VotingStorage.open();
        await window.owner.claim();
        const checkpoint = await window.owner.bindSession('clear-race');
        const pending = checkpoint(new Uint8Array([1]), 1n);
        const clearing = window.owner.clear();
        await Promise.allSettled([pending, clearing]);
        return { gone: (await window.owner.read('clear-race')) === undefined, locked: !!window.owner.release };
    });
    expect(result).toEqual({ gone: true, locked: false });
});
