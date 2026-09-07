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
  if (initThreadPool) await initThreadPool(2);
  console.log('WASM initialized');
  if (!WebVotingSession) {
    await test('default wallet initializes Rayon and resolves an async read', async () => {
      const wallet = new WebWallet('test', 'https://example.invalid');
      assert((await wallet.db_to_bytes()).length > 0, 'empty wallet export');
      wallet.free();
    });
  } else {
  await test('owned proof execution on the initialized global pool and sequential executor', async () => {
    assert(await bindings.votingTestProofExecutor(), 'executor result or worker mismatch');
  });
  await test('browser PIR binds actual root metadata and uses supported clocks through private request generation', async () => {
    const original = window.fetch;
    const layout = {pir_depth:19,tier0_layers:11,tier1_layers:8,poly_len:2048};
    const root = {zcash_network:'test',nullifier_pool:'ironwood',dataset_version:2,circuit_root:'00'.repeat(32),pir_root:'00'.repeat(32),num_ranges:1,pir_layout:layout,pir_depth:19,tier1_rows:2048,tier1_row_bytes:24576,height:100};
    let hits = [];
    window.fetch = async request => {
      const path = new URL(request.url).pathname;
      hits.push(path);
      assert(request.credentials === 'omit' && request.redirect === 'error', 'unsafe Fetch options');
      if (path === '/root') return new Response(JSON.stringify(root));
      if (path === '/tier0') return new Response(new Uint8Array(2047 * 32 + 2048 * 64));
      if (path === '/params/tier1') return new Response(JSON.stringify({num_items:2048,item_size_bits:24576*8,poly_len:2048}));
      assert(request.method === 'POST' && (await request.arrayBuffer()).byteLength > 0, 'missing encoded private query');
      return new Response(new Uint8Array([0]), {status:200});
    };
    try {
      for (const [field,value] of [['zcash_network','main'],['nullifier_pool','orchard'],['dataset_version',1],['pir_layout',{...layout,poly_len:4096}],['circuit_root','01'.repeat(32)],['height',101]]) {
        const saved=root[field]; root[field]=value; hits=[];
        await errorCode(()=>bindings.votingTestPir('https://pir.example',false),'PIR_SNAPSHOT_MISMATCH');
        assert(JSON.stringify(hits) === '["/root"]', 'mismatch reached data or query endpoint');
        root[field]=saved;
      }
      hits=[];
      assert(await bindings.votingTestPir('https://pir.example',true), 'malformed proof response accepted');
      assert(hits.includes('/tier1/query'), 'private request path was not exercised');
    } finally { window.fetch=original; }
  });
  await test('PIR query fallback is ordered, independently bound, and cannot mask corruption', async () => {
    const original = window.fetch;
    const layout = {pir_depth:19,tier0_layers:11,tier1_layers:8,poly_len:2048};
    const root = {zcash_network:'test',nullifier_pool:'ironwood',dataset_version:2,circuit_root:'00'.repeat(32),pir_root:'00'.repeat(32),num_ranges:1,pir_layout:layout,pir_depth:19,tier1_rows:2048,tier1_row_bytes:24576,height:100};
    try {
      for (const mode of ['transport','status','all-fail','identity','malformed','oversized','mixed']) {
        const hits = [];
        const queryCounts = new Map();
        window.fetch = async request => {
          const {hostname, pathname} = new URL(request.url);
          hits.push(`${hostname}${pathname}`);
          assert(request.credentials === 'omit' && request.redirect === 'error', 'unsafe fallback Fetch options');
          if (pathname === '/root') return new Response(JSON.stringify(mode === 'identity' && hostname === 'second.example' ? {...root,height:101} : root));
          if (pathname === '/tier0') return new Response(new Uint8Array(2047 * 32 + 2048 * 64));
          if (pathname === '/params/tier1') return new Response(JSON.stringify({num_items:2048,item_size_bits:24576*8,poly_len:2048}));
          for (const path of ['/root','/tier0','/params/tier1']) assert(hits.includes(`${hostname}${path}`), 'private query before endpoint handshake');
          assert(request.method === 'POST' && (await request.arrayBuffer()).byteLength > 0, 'missing encoded fallback query');
          const count = (queryCounts.get(hostname) || 0) + 1;
          queryCounts.set(hostname, count);
          if (hostname === 'first.example') {
            if (mode === 'transport') throw new TypeError('unavailable');
            if (mode === 'oversized') return new Response('', {headers:{'Content-Length':String(64*1024*1024+1)}});
            if (mode === 'malformed' || (mode === 'mixed' && count === 2)) return new Response(new Uint8Array([0]));
            return new Response('server diagnostic must not escape', {status:503});
          }
          return mode === 'all-fail' ? new Response('', {status:502}) : new Response(new Uint8Array([0]));
        };
        await errorCode(()=>bindings.votingTestPirFallback(['https://first.example','https://second.example'],false), 'PIR_SNAPSHOT_MISMATCH');
        assert(queryCounts.get('first.example') === 2, 'first query batch not completed');
        const stopAtFirst = ['malformed','oversized','mixed'].includes(mode);
        assert(hits.includes('second.example/root') !== stopAtFirst, 'incorrect protocol failure fallback');
        if (mode === 'identity') {
          assert(!hits.includes('second.example/tier0') && !queryCounts.has('second.example'), 'identity mismatch reached private query');
        } else if (!stopAtFirst) {
          assert(queryCounts.get('second.example') === 2, 'validated replacement not queried');
        }
        const replacement = hits.indexOf('second.example/root');
        if (replacement >= 0) assert(hits.slice(replacement).every(hit=>hit.startsWith('second.example/')), 'endpoint order changed');
      }
    } finally { window.fetch = original; }
  });
  await test('PIR public assets complete and permanent errors dominate in either response order', async () => {
    const original = window.fetch;
    const layout = {pir_depth:19,tier0_layers:11,tier1_layers:8,poly_len:2048};
    const root = {zcash_network:'test',nullifier_pool:'ironwood',dataset_version:2,circuit_root:'00'.repeat(32),pir_root:'00'.repeat(32),num_ranges:1,pir_layout:layout,pir_depth:19,tier1_rows:2048,tier1_row_bytes:24576,height:100};
    const paths = ['/tier0','/params/tier1'];
    const valid = index => index === 0 ? new Uint8Array(2047*32+2048*64) : JSON.stringify({num_items:2048,item_size_bits:24576*8,poly_len:2048});
    const cases = [['valid','valid'],['unavailable','unavailable'],['valid','unavailable'],['unavailable','valid']];
    for (const permanent of ['malformed','oversized','protocol']) cases.push([permanent,'unavailable'],['unavailable',permanent]);
    try {
      for (const outcomes of cases) for (const slow of [0,1]) {
        const completed = [], roots = [];
        window.fetch = async request => {
          const {hostname,pathname} = new URL(request.url);
          assert(request.method === 'GET', 'asset handshake dispatched private query');
          if (pathname === '/root') { roots.push(hostname); return new Response(JSON.stringify(root)); }
          const index = paths.indexOf(pathname);
          assert(index >= 0, 'unexpected public path');
          if (hostname === 'second.example') return new Response(valid(index));
          if (index === slow) await new Promise(resolve=>setTimeout(resolve,10));
          completed.push(index);
          switch (outcomes[index]) {
            case 'unavailable': return new Response('diagnostic must not escape',{status:503});
            case 'malformed': return new Response(new Uint8Array([0]));
            case 'oversized': return new Response('',{headers:{'Content-Length':String(64*1024*1024+1)}});
            case 'protocol': {
              if (index === 1) return new Response(JSON.stringify({num_items:0,item_size_bits:24576*8,poly_len:2048}));
              const bytes=valid(index); bytes[0]=1; return new Response(bytes);
            }
            default: return new Response(valid(index));
          }
        };
        const permanent = outcomes.some(value=>!['valid','unavailable'].includes(value));
        const connect=()=>bindings.votingTestPirFallback(['https://first.example','https://second.example'],true);
        if (permanent) await errorCode(connect,'PIR_SNAPSHOT_MISMATCH');
        else assert(await connect(), 'valid public fallback failed');
        assert(JSON.stringify(completed)===JSON.stringify([1-slow,slow]), 'did not complete both public loads');
        assert(roots.length === (permanent || outcomes.every(value=>value==='valid') ? 1 : 2), 'masked permanent public asset failure');
      }
    } finally { window.fetch=original; }
  });
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
      await errorCode(()=>s.prepareRound(metadata.roundId),'ROUND_UNAUTHENTICATED');
      await errorCode(()=>s.clearRound('bad round id'),'ROUND_UNAUTHENTICATED');
      assert(!fetched,'untrusted round reached network');
    } finally { window.fetch=original; s.free(); }
  });
  wallet.free();
  }
} catch (error) { results.push({name:'harness',passed:false,error:String(error)}); }
window.sessionResults.done = true;
