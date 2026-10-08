import init, { initThreadPool, WebWallet } from './pkg/zcash_wallet_wasm.js';

window.sessionResults = { done: false, results: [] };
const results = window.sessionResults.results;
const assert = (condition, message) => { if (!condition) throw new Error(message); };
async function test(name, run) {
  try { await run(); results.push({ name, passed: true }); }
  catch (error) { results.push({ name, passed: false, error: String(error) }); }
}
const policy = { trusted: 1, untrusted: 2 };
const create = (...args) => new WebWallet('test', 'https://example.invalid', ...args);
async function balance(wallet) {
  const summary = await wallet.get_wallet_summary();
  assert(summary, 'missing funded wallet summary');
  try {
    const balances = summary.account_balances;
    assert(balances.length === 1, 'fixture must have one account');
    return balances[0][1];
  } finally { summary.free(); }
}
function expectBalance(actual, spendable, pending) {
  assert(Number(actual.unshielded_balance) === spendable, `spendable: ${actual.unshielded_balance}`);
  assert(Number(actual.unshielded_pending_balance) === pending, `pending: ${actual.unshielded_pending_balance}`);
  assert(Number(actual.unshielded_total_balance) === 200000, 'policy changed total funds');
}

try {
  await init();
  await initThreadPool(2);
  await test('omitted, undefined and null policies preserve legacy empty-wallet construction', async () => {
    for (const args of [[], [undefined], [null], [undefined, undefined], [undefined, null]]) {
      const wallet = create(...args);
      try { assert((await wallet.db_to_bytes()).length > 0, 'empty database export'); }
      finally { wallet.free(); }
    }
  });
  await test('two-field policies accept symmetric and maximum uint32 counts', async () => {
    for (const count of [1, 4294967295]) {
      const wallet = create(undefined, { trusted: count, untrusted: count });
      try { assert((await wallet.db_to_bytes()).length > 0, 'valid policy rejected'); }
      finally { wallet.free(); }
    }
  });
  await test('invalid policies fail at construction without any network request', async () => {
    const invalid = [
      {}, [], true, 1, 'default',
      { ...policy, trusted: 0 }, { ...policy, untrusted: 0 },
      { ...policy, trusted: 3 }, { ...policy, trusted: -1 },
      { ...policy, trusted: 1.5 }, { ...policy, untrusted: NaN },
      { ...policy, untrusted: Infinity }, { ...policy, untrusted: 4294967296 },
      { ...policy, trusted: '1' }, { ...policy, trusted: null },
      { ...policy, trusted: undefined }, { ...policy, untrusted: undefined },
      { ...policy, allowZeroConfShielding: true },
      { ...policy, allowZeroConfShielding: false },
      { ...policy, allow_zero_conf_shielding: true },
      { ...policy, unexpected: 1 },
      Object.defineProperty({ ...policy }, 'allowZeroConfShielding', { value: false }),
    ];
    const original = window.fetch;
    let requests = 0;
    window.fetch = () => { requests++; throw new Error('unexpected network access'); };
    try {
      for (const value of invalid) {
        let wallet, error;
        try { wallet = create(undefined, value); }
        catch (caught) { error = caught; }
        finally { wallet?.free(); }
        assert(error instanceof Error, `accepted invalid policy: ${JSON.stringify(value)}`);
      }
      assert(requests === 0, 'invalid policy reached the network');
    } finally { window.fetch = original; }
  });
  const response = await fetch('fixtures/confirmations-wallet.bin');
  assert(response.ok, 'generate the confirmations policy browser fixture first');
  const bytes = new Uint8Array(await response.arrayBuffer());
  await test('restored wallets use default policy when the fourth argument is absent or null', async () => {
    for (const args of [[bytes], [bytes, undefined], [bytes, null]]) {
      const wallet = create(...args);
      try { expectBalance(await balance(wallet), 0, 200000); }
      finally { wallet.free(); }
    }
  });
  await test('custom policy changes spendability and must be resupplied on restore', async () => {
    const supplied = { ...policy };
    const wallet = create(bytes, supplied);
    try {
      supplied.untrusted = 100;
      expectBalance(await balance(wallet), 200000, 0);
      const saved = await wallet.db_to_bytes();
      const restored = create(saved, policy);
      const defaulted = create(saved);
      try {
        expectBalance(await balance(restored), 200000, 0);
        expectBalance(await balance(defaulted), 0, 200000);
      } finally { restored.free(); defaulted.free(); }
    } finally { wallet.free(); }
  });
} catch (error) {
  results.push({ name: 'harness', passed: false, error: String(error) });
} finally { window.sessionResults.done = true; }
