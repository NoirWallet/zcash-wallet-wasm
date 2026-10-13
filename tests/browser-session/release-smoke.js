import init, * as bindings from './pkg/zcash_wallet_wasm.js';

window.sessionResults = { done: false, results: [] };
const results = window.sessionResults.results;

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

async function test(name, run) {
  try {
    await run();
    results.push({ name, passed: true });
  } catch (error) {
    results.push({ name, passed: false, error: String(error) });
  }
}

await test('release package initializes shared memory and wallet API', async () => {
  const exports = await init();
  assert(crossOriginIsolated, 'host must provide cross-origin isolation');
  assert(exports.memory.buffer instanceof SharedArrayBuffer, 'WASM memory must be shared');
  assert(typeof bindings.initThreadPool === 'function', 'missing initThreadPool');
  assert(typeof bindings.WebWallet === 'function', 'missing WebWallet');
  assert(typeof bindings.WebVotingSession === 'function', 'missing WebVotingSession');
  assert(!Object.keys(bindings).some((name) => name.startsWith('votingTest')), 'test export found');

  await bindings.initThreadPool(2);
  const fixture = async name => new Uint8Array(await (await fetch(`/fixtures/${name}`)).arrayBuffer());
  const metadata = await (await fetch('/fixtures/metadata.json')).json();
  const wallet = new bindings.WebWallet('test', 'https://example.invalid', await fixture('wallet.bin'));
  try {
    assert((await wallet.db_to_bytes()).length > 0, 'empty wallet serialization');
    const session = new bindings.WebVotingSession(
      wallet, metadata.accountId, await fixture('voting.bin'), async () => {}
    );
    try {
      const saved = session.exportState();
      assert(saved.length > 0, 'empty voting state');
      const reopened = new bindings.WebVotingSession(wallet, metadata.accountId, saved, async () => {});
      try {
        assert(reopened.storageKey === session.storageKey, 'voting storage key changed on reopen');
        assert(reopened.exportState().length > 0, 'reopened voting state is empty');
      } finally {
        reopened.free();
      }
    } finally {
      session.free();
    }
  } finally {
    wallet.free();
  }
});

window.sessionResults.done = true;
