import init, { WebWallet, generate_mnemonic, initThreadPool } from './pkg/zcash_wallet_wasm.js'

;['log', 'warn', 'error', 'info'].forEach((level) => {
  const orig = console[level].bind(console)
  console[level] = (...a) => {
    try {
      postMessage({ type: 'console', level, msg: a.map((x) => (typeof x === 'string' ? x : JSON.stringify(x))).join(' ').slice(0, 500) })
    } catch (_) {}
    orig(...a)
  }
})

let ready = false

async function ensureReady(threads) {
  if (ready) return
  await init()
  await initThreadPool(threads)
  ready = true
}

// Journal-path A/B: prepare_sync_batch (baseline) vs prepare_sync_batch_optimized.
// Both download + scan one batch into the in-memory DB and return journal bytes
// (Uint8Array) or undefined when fully synced. We ignore the journal bytes (no
// persistence) and measure pure download+scan wall-clock, mirroring production's
// journal sync. get_wallet_summary is avoided (it panics in this build), so the
// batch return value alone drives the loop and maxBatches bounds the window.
async function syncWindow(apiName, { network, url, mnemonic, birthday, batchSize, accounts, maxBatches }) {
  const wallet = new WebWallet(network, url, null)
  for (let i = 0; i < accounts; i++) {
    await wallet.create_account('bench' + i, mnemonic, i, birthday, '')
  }

  const t0 = performance.now()
  let batches = 0
  const HARD_MAX = 2000
  const MAX_MS = 90 * 60 * 1000

  while (true) {
    const bt = performance.now()
    const journal = await wallet[apiName](batchSize)
    const hasMore = journal != null
    const batchMs = performance.now() - bt
    batches++
    if (batches % 10 === 0 || batchMs > 4000) {
      postMessage({
        type: 'progress',
        variant: apiName,
        batches,
        batchMs: +batchMs.toFixed(0),
        elapsedS: +((performance.now() - t0) / 1000).toFixed(1)
      })
    }
    if (!hasMore) break
    if (maxBatches && batches >= maxBatches) break
    if (batches >= HARD_MAX) break
    if (performance.now() - t0 > MAX_MS) break
  }

  const elapsedMs = performance.now() - t0
  const blocks = batches * batchSize
  wallet.free?.()
  return { elapsedMs, blocks, batches }
}

const median = (xs) => {
  const s = [...xs].sort((a, b) => a - b)
  const m = Math.floor(s.length / 2)
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2
}

async function runRange(label, rangeBlocks, ctx) {
  const { network, url, batchSize, accounts, trials, tip } = ctx
  const birthday = tip - rangeBlocks
  const maxBatches = Math.ceil(rangeBlocks / batchSize)
  // Fresh mnemonic per range so neither variant benefits from a warm DB.
  const mnemonic = generate_mnemonic(24)
  const opts = { network, url, mnemonic, birthday, batchSize, accounts, maxBatches }

  postMessage({ type: 'log', msg: `=== ${label} (${rangeBlocks} blocks, birthday ${birthday}, maxBatches ${maxBatches}) ===` })

  const baseTimes = []
  const optTimes = []
  let lastBase = null
  let lastOpt = null
  for (let i = 0; i < trials; i++) {
    postMessage({ type: 'log', msg: `${label} trial ${i + 1}/${trials}: baseline` })
    lastBase = await syncWindow('prepare_sync_batch', opts)
    baseTimes.push(lastBase.elapsedMs)
    postMessage({ type: 'log', msg: `  baseline ${(lastBase.elapsedMs / 1000).toFixed(1)}s (${lastBase.batches} batches)` })

    postMessage({ type: 'log', msg: `${label} trial ${i + 1}/${trials}: optimized` })
    lastOpt = await syncWindow('prepare_sync_batch_optimized', opts)
    optTimes.push(lastOpt.elapsedMs)
    postMessage({ type: 'log', msg: `  optimized ${(lastOpt.elapsedMs / 1000).toFixed(1)}s (${lastOpt.batches} batches)` })
  }

  const baseMed = median(baseTimes)
  const optMed = median(optTimes)
  const result = {
    label,
    rangeBlocks,
    birthday,
    maxBatches,
    batchSize,
    accounts,
    trials,
    baseTimesS: baseTimes.map((x) => +(x / 1000).toFixed(2)),
    optTimesS: optTimes.map((x) => +(x / 1000).toFixed(2)),
    baseMedianS: +(baseMed / 1000).toFixed(2),
    optMedianS: +(optMed / 1000).toFixed(2),
    baseBatches: lastBase.batches,
    optBatches: lastOpt.batches,
    speedup: +(baseMed / optMed).toFixed(4),
    savedPct: +((1 - optMed / baseMed) * 100).toFixed(2),
    baselineBps: +(lastBase.blocks / (baseMed / 1000)).toFixed(1),
    optimizedBps: +(lastOpt.blocks / (optMed / 1000)).toFixed(1)
  }
  postMessage({ type: 'rangeResult', result })
  return result
}

// Concurrency probe: fire K parallel download-only requests over disjoint
// ranges and measure aggregate throughput. If bps scales with K, the node
// supports concurrent fetching (so client-side prefetch will help); if flat,
// the node/connection is the throughput limiter.
async function runProbe({ network, url, threads, blocksPer = 1000, concurrencies = [1, 2, 4, 8], trials = 2 }) {
  await ensureReady(threads || 4)
  const wallet = new WebWallet(network, url, null)
  const tip = Number(await wallet.get_latest_block())
  let cursor = tip - 5000 // stay below tip; walk downward, never reuse a window
  postMessage({ type: 'log', msg: `probe: tip ${tip}  blocksPer ${blocksPer}  K=${concurrencies.join(',')}  trials ${trials}` })

  const median = (xs) => { const s = [...xs].sort((a, b) => a - b); const m = Math.floor(s.length / 2); return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2 }
  const rows = []
  for (const K of concurrencies) {
    const bpsTrials = []
    for (let t = 0; t < trials; t++) {
      const starts = []
      for (let i = 0; i < K; i++) { cursor -= blocksPer; starts.push(cursor) }
      const t0 = performance.now()
      const counts = await Promise.all(starts.map((s) => wallet.bench_download_range(s, blocksPer)))
      const sec = (performance.now() - t0) / 1000
      const blocks = counts.reduce((a, b) => a + b, 0)
      bpsTrials.push(blocks / sec)
      postMessage({ type: 'log', msg: `  K=${K} trial ${t + 1}: ${blocks} blocks in ${sec.toFixed(1)}s = ${(blocks / sec).toFixed(0)} bps` })
    }
    const bps = median(bpsTrials)
    rows.push({ K, bps: +bps.toFixed(0), bpsTrials: bpsTrials.map((x) => +x.toFixed(0)) })
    postMessage({ type: 'probeRow', row: rows[rows.length - 1], base: rows[0].bps })
  }
  wallet.free?.()
  postMessage({ type: 'probeReport', tip, blocksPer, rows, baseBps: rows[0].bps })
}

// Batch-size sweep: for each batchSize, sync a fixed block window via the timed
// journal batch, accumulating per-stage timings. Reveals how much of per-batch
// cost is fixed overhead (setup) vs block-proportional (download+scan), and
// whether larger batches amortize the fixed cost into a big total speedup.
async function runSweep({ network, url, threads, rangeBlocks, batchSizes, accounts = 1 }) {
  await ensureReady(threads || (navigator.hardwareConcurrency || 4))
  const probe = new WebWallet(network, url, null)
  const tip = Number(await probe.get_latest_block())
  probe.free?.()
  postMessage({ type: 'log', msg: `sweep: tip ${tip}  range ${rangeBlocks}  batchSizes ${batchSizes.join(',')}  accounts ${accounts}` })

  const rows = []
  for (const bs of batchSizes) {
    const birthday = tip - rangeBlocks
    const maxBatches = Math.ceil(rangeBlocks / bs)
    const mnemonic = generate_mnemonic(24)
    const wallet = new WebWallet(network, url, null)
    for (let i = 0; i < accounts; i++) await wallet.create_account('sweep' + i, mnemonic, i, birthday, '')

    let batches = 0, setup = 0, download = 0, scan = 0
    const t0 = performance.now()
    while (batches < maxBatches) {
      const json = await wallet.bench_prepare_sync_batch_timed(bs)
      if (json == null) break
      const s = JSON.parse(json)
      setup += s.setup; download += s.download; scan += s.scan
      batches++
      if (batches % 5 === 0) postMessage({ type: 'log', msg: `  bs=${bs} batch ${batches}/${maxBatches} (setup ${s.setup} dl ${s.download} scan ${s.scan})` })
    }
    const totalS = (performance.now() - t0) / 1000
    wallet.free?.()
    const blocks = batches * bs
    const row = {
      batchSize: bs, batches, totalS: +totalS.toFixed(1), bps: +(blocks / totalS).toFixed(0),
      avgSetup: +(setup / batches).toFixed(0), avgDownload: +(download / batches).toFixed(0), avgScan: +(scan / batches).toFixed(0),
      sumSetupS: +(setup / 1000).toFixed(1), sumDownloadS: +(download / 1000).toFixed(1), sumScanS: +(scan / 1000).toFixed(1)
    }
    rows.push(row)
    postMessage({ type: 'sweepRow', row })
  }
  postMessage({ type: 'sweepReport', tip, rangeBlocks, rows })
}

// C+D comparison: at a fixed batch size, sync the same window two ways and
// compare per-stage timings (subtree/setup/download/scan).
//   variant 'perbatch' : every batch runs full session setup (refresh=true)
//   variant 'once'     : sync_session_setup() once, then refresh=false batches
// Isolates how much wall-clock the hoisted setup round-trips actually cost.
async function runSetupCompare({ network, url, threads, rangeBlocks, batchSize, accounts = 1, mnemonic: userMnemonic, accountIndex = 0 }) {
  await ensureReady(threads || (navigator.hardwareConcurrency || 4))
  const probe = new WebWallet(network, url, null)
  const tip = Number(await probe.get_latest_block())
  probe.free?.()
  const bs = batchSize || 5000
  const realSeed = !!(userMnemonic && userMnemonic.trim())
  postMessage({ type: 'log', msg: `cd: tip ${tip}  range ${rangeBlocks}  bs ${bs}  accounts ${accounts}  seed ${realSeed ? 'real' : 'random'}` })

  const rows = []
  for (const variant of ['perbatch', 'once']) {
    const birthday = tip - rangeBlocks
    const maxBatches = Math.ceil(rangeBlocks / bs)
    const mnemonic = realSeed ? userMnemonic.trim() : generate_mnemonic(24)
    const wallet = new WebWallet(network, url, null)
    const baseIdx = realSeed ? accountIndex : 0
    for (let i = 0; i < accounts; i++) await wallet.create_account('cd' + i, mnemonic, baseIdx + i, birthday, '')

    const once = variant === 'once'
    let batches = 0, subtree = 0, setup = 0, download = 0, scan = 0
    const t0 = performance.now()
    if (once) await wallet.sync_session_setup()
    while (batches < maxBatches) {
      const refresh = !once
      const json = await wallet.bench_prepare_sync_batch_timed(bs, refresh)
      if (json == null) break
      const s = JSON.parse(json)
      subtree += s.subtree; setup += s.setup; download += s.download; scan += s.scan
      batches++
      if (batches % 5 === 0) postMessage({ type: 'log', msg: `  ${variant} batch ${batches}/${maxBatches} (sub ${s.subtree} setup ${s.setup} dl ${s.download} scan ${s.scan})` })
    }
    const totalS = (performance.now() - t0) / 1000

    // Confirm the real seed detected transactions in this window.
    let txCount = -1
    try {
      const hist = await wallet.get_transaction_history(baseIdx)
      txCount = Array.isArray(hist) ? hist.length : (hist?.length ?? -1)
    } catch (e) {
      postMessage({ type: 'log', msg: `  ${variant} tx history error: ${e?.message || e}` })
    }

    wallet.free?.()
    const blocks = batches * bs
    const row = {
      variant, batchSize: bs, batches, totalS: +totalS.toFixed(1), bps: +(blocks / totalS).toFixed(0),
      txCount,
      sumSubtreeS: +(subtree / 1000).toFixed(1), sumSetupS: +(setup / 1000).toFixed(1),
      sumDownloadS: +(download / 1000).toFixed(1), sumScanS: +(scan / 1000).toFixed(1)
    }
    rows.push(row)
    postMessage({ type: 'cdRow', row })
  }
  postMessage({ type: 'cdReport', tip, rangeBlocks, batchSize: bs, rows })
}

onmessage = async (e) => {
  if (e.data && e.data.mode === 'cd') {
    try { await runSetupCompare(e.data) } catch (err) { postMessage({ type: 'error', msg: String(err?.message || err), stack: String(err?.stack || '') }) }
    return
  }
  if (e.data && e.data.mode === 'probe') {
    try { await runProbe(e.data) } catch (err) { postMessage({ type: 'error', msg: String(err?.message || err), stack: String(err?.stack || '') }) }
    return
  }
  if (e.data && e.data.mode === 'sweep') {
    try { await runSweep(e.data) } catch (err) { postMessage({ type: 'error', msg: String(err?.message || err), stack: String(err?.stack || '') }) }
    return
  }
  const { network, url, ranges, batchSize = 1000, threads, accounts = 1, trials = 1 } = e.data
  try {
    await ensureReady(threads)
    postMessage({ type: 'log', msg: `worker ready (${threads} threads, journal A/B)` })

    const probe = new WebWallet(network, url, null)
    const tip = Number(await probe.get_latest_block())
    probe.free?.()
    postMessage({ type: 'log', msg: `tip ${tip}  accounts ${accounts}  batchSize ${batchSize}  trials ${trials}` })

    const ctx = { network, url, batchSize, accounts, trials, tip }
    const report = []
    for (const r of ranges) {
      const res = await runRange(r.label, r.blocks, ctx)
      report.push(res)
    }
    postMessage({ type: 'report', tip, accounts, batchSize, trials, report })
  } catch (err) {
    postMessage({ type: 'error', msg: String(err?.message || err), stack: String(err?.stack || '') })
  }
}
