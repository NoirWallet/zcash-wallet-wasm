import init, * as bindings from './pkg/zcash_wallet_wasm.js';
const { initThreadPool, WebWallet, WebVotingSession } = bindings;

window.sessionResults = { done: false, results: [] };
const results = window.sessionResults.results;
const assert = (condition, message) => { if (!condition) throw new Error(message); };
async function test(name, run) {
  window.sessionResults.current = name;
  console.log(`Starting: ${name}`);
  try { await run(); results.push({ name, passed: true }); }
  catch (error) { results.push({ name, passed: false, error: String(error) }); }
}
async function errorCode(action, expected) {
  try { await action(); throw new Error('operation unexpectedly succeeded'); }
  catch (error) {
    assert(error instanceof Error, 'not a JavaScript Error');
    assert(error.name === expected, `wrong name: ${error.name}`);
    assert(!error.message.includes('HOST_PRIVATE'), 'private host error leaked');
  }
}
try {
  const exports = await init();
  assert(crossOriginIsolated, 'host must serve COOP/COEP headers');
  assert(exports.memory.buffer instanceof SharedArrayBuffer, 'WASM memory must be shared');
  await initThreadPool(2);
  console.log('WASM initialized');
  if (!WebVotingSession) {
    await test('default wallet initializes Rayon and resolves an async read', async () => {
      const wallet = new WebWallet('test', 'https://example.invalid');
      assert((await wallet.db_to_bytes()).length > 0, 'empty wallet export');
      wallet.free();
    });
  } else {
  const bytes = async name => new Uint8Array(await (await fetch(`fixtures/${name}`)).arrayBuffer());
  const metadata = await (await fetch('fixtures/metadata.json')).json();
  const walletBytes = await bytes('wallet.bin');
  const votingBytes = await bytes('voting.bin');
  console.log('Constructing synthetic wallet');
  const wallet = new WebWallet('test', 'https://example.invalid', walletBytes);
  console.log('Synthetic wallet constructed');
  const session = (callback, restored = votingBytes) => new WebVotingSession(wallet, metadata.accountId, restored, callback);
  await test('synchronous constructor and exact hashed storage key', () => {
    let calls = 0;
    const s = session(() => { calls++; });
    assert(!(s instanceof Promise), 'constructor was asynchronous');
    assert(s.storageKey === metadata.storageKey, 'storage key mismatch');
    assert(/^zcash-voting:testnet:[a-f0-9]{64}$/.test(s.storageKey), 'unsafe storage key');
    assert(calls === 0, 'constructor checkpointed');
    s.free();
  });
  await test('account, network, owner and corrupt snapshot errors are safe', async () => {
    await errorCode(() => new WebVotingSession(wallet, 999, undefined, () => {}), 'NOT_ELIGIBLE');
    const main = new WebWallet('main', 'https://example.invalid');
    await errorCode(() => new WebVotingSession(main, 0, undefined, () => {}), 'NETWORK_MISMATCH');
    const other = new WebWallet('test','https://example.invalid',await bytes('other-wallet.bin'));
    await errorCode(() => new WebVotingSession(other,metadata.accountId,votingBytes,()=>{}),'RECOVERY_CONFLICT');
    await errorCode(() => session(()=>{},new Uint8Array([1,2,3])),'RECOVERY_CONFLICT');
    main.free(); other.free();
  });
  await test('exportState owns an independent byte copy', () => {
    const s = session(()=>{});
    const first = s.exportState();
    const second = s.exportState();
    first.fill(0);
    assert(second[0] === 83 && s.exportState()[0] === 83, 'export aliases database');
    s.free();
  });
  await test('roundStatus exposes only the stable camelCase progress DTO', async () => {
    const s = session(()=>{});
    const status = await s.roundStatus(metadata.roundId);
    assert(JSON.stringify(Object.keys(status).sort()) === JSON.stringify([
      'roundId','phase','completedBundles','totalBundles','confirmedVotes','totalVotes',
      'confirmedHelpers','requiredHelpers','nextAction',
    ].sort()),'unexpected status fields');
    assert(status.roundId === metadata.roundId && status.phase === 'expired', 'durable status mismatch');
    s.free();
  });
  for (const kind of ['throw', 'reject', 'value', 'resolve', 'thenable', 'thenable-reject', 'then-getter-throw']) {
    await test(`checkpoint ${kind} uses copied bytes and BigInt revisions`, async () => {
      const revisions = [];
      const snapshots = [];
      const s = session((snapshot, revision) => {
        assert(snapshot instanceof Uint8Array && snapshot[0] === 83, 'invalid SQLite bytes');
        assert(typeof revision === 'bigint','revision is not BigInt');
        revisions.push(revision); snapshots.push(snapshot);
        snapshot.fill(0);
        if (kind === 'throw') throw new Error('HOST_PRIVATE');
        if (kind === 'reject') return Promise.reject(new Error('HOST_PRIVATE'));
        if (kind === 'resolve') return Promise.resolve();
        if (kind === 'thenable') return { then(resolve) { resolve(); } };
        if (kind === 'thenable-reject') return { then(_, reject) { reject(new Error('HOST_PRIVATE')); } };
        if (kind === 'then-getter-throw') return { get then() { throw new Error('HOST_PRIVATE'); } };
        return 7;
      });
      for (let i=0;i<2;i++) {
        if (['throw','reject','thenable-reject','then-getter-throw'].includes(kind)) await errorCode(()=>s.clearRound(metadata.roundId),'PERSISTENCE_FAILED');
        else await s.clearRound(metadata.roundId);
      }
      assert(revisions[0] === 1n && revisions[1] === 2n,'revision reused or did not start at one');
      assert(snapshots[0].buffer !== snapshots[1].buffer,'checkpoint buffers alias');
      assert(s.exportState()[0] === 83,'callback mutated SQLite');
      await errorCode(()=>s.roundStatus(metadata.roundId),'ROUND_UNAUTHENTICATED');
      s.free();
    });
  }
  await test('pending checkpoint rejects second mutation but allows status and export', async () => {
    let resolve;
    let calls = 0;
    const s = session(() => { calls++; return new Promise(done => { resolve = done; }); });
    const clearing = s.clearRound(metadata.roundId);
    await new Promise(done=>setTimeout(done,0));
    await errorCode(()=>s.clearRound(metadata.roundId),'OPERATION_IN_PROGRESS');
    assert(s.exportState()[0] === 83,'export unavailable during mutation');
    await errorCode(()=>s.roundStatus(metadata.roundId),'ROUND_UNAUTHENTICATED');
    assert(calls === 1,'overlap invoked checkpoint');
    resolve(); await clearing;
    s.free();
  });
  await test('invalid and unknown round IDs fail closed without Fetch', async () => {
    let fetched = false;
    const original = window.fetch;
    window.fetch = () => { fetched=true; throw new Error('HOST_PRIVATE'); };
    const s = session(()=>{});
    try {
      await errorCode(()=>s.checkEligibility(metadata.roundId),'ROUND_UNAUTHENTICATED');
      await errorCode(()=>s.clearRound('bad round id'),'ROUND_UNAUTHENTICATED');
      assert(!fetched,'untrusted round reached network');
    } finally { window.fetch=original; s.free(); }
  });
  wallet.free();
  }
} catch (error) { results.push({name:'harness',passed:false,error:String(error)}); }
window.sessionResults.done = true;
