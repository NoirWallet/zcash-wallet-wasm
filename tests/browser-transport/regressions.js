import init, { request, cancel_request, cancel_batches, network_request } from './pkg/browser_transport_regressions.js';

const results = [];
window.transportResults = { done: false, results };
const originalFetch = window.fetch;
const originalSetTimeout = window.setTimeout;
const originalClearTimeout = window.clearTimeout;

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

async function fixture(name, hostFetch, post, limit, expected) {
  const pending = new Set();
  const transportTimers = new Set();
  let calledWithWindow = false;
  let calls = 0;
  let clears = 0;
  window.setTimeout = function(callback, delay, ...args) {
    // WebDriver may install its own async-script watchdog while the test awaits.
    if (delay !== 100) return originalSetTimeout.call(window, callback, delay, ...args);
    const id = originalSetTimeout.call(window, () => {
      pending.delete(id);
      callback(...args);
    }, delay);
    pending.add(id);
    transportTimers.add(id);
    return id;
  };
  window.clearTimeout = function(id) {
    if (transportTimers.has(id)) clears += 1;
    pending.delete(id);
    originalClearTimeout.call(window, id);
  };
  window.fetch = function(req) {
    calls += 1;
    calledWithWindow = this === window;
    return hostFetch(req);
  };
  let watchdog;
  try {
    const result = await Promise.race([
      request(post, limit).then(value => ({ value }), error => ({ error })),
      new Promise((_, reject) => {
        watchdog = originalSetTimeout.call(window, () => reject(new Error('transport never settled')), 1000);
      }),
    ]);
    assert(calledWithWindow, 'Fetch must receive this=window');
    assert(calls === 1, 'transport must dispatch exactly once');
    assert(clears === 1, 'transport must clear its timeout exactly once');
    assert(pending.size === 0, 'transport timeout was not cleared');
    if (typeof expected === 'string') {
      assert(result.error === expected, `expected ${expected}, got ${JSON.stringify(result)}`);
    } else {
      assert(JSON.stringify(result.value) === JSON.stringify(expected), `unexpected response ${JSON.stringify(result)}`);
    }
    results.push({ name, passed: true });
  } catch (error) {
    results.push({ name, passed: false, error: String(error) });
  } finally {
    originalClearTimeout.call(window, watchdog);
    for (const id of pending) originalClearTimeout.call(window, id);
    window.fetch = originalFetch;
    window.setTimeout = originalSetTimeout;
    window.clearTimeout = originalClearTimeout;
  }
}

function bufferResponse(length) {
  const buffer = new Uint8Array(0);
  Object.defineProperty(buffer, 'byteLength', { value: length });
  const response = new Response();
  Object.defineProperty(response, 'body', { value: { getReader() {
    return { read: () => Promise.resolve({ done: true, value: buffer }), cancel() {}, releaseLock() {} };
  } } });
  return Promise.resolve(response);
}

try {
  await init();
  await fixture('streams body without arrayBuffer allocation', () => {
    const response = new Response(new Uint8Array([1, 2, 3, 4]));
    response.arrayBuffer = () => { throw new Error('whole body allocation'); };
    return Promise.resolve(response);
  }, false, 4, [200, [1, 2, 3, 4]]);
  let cancelledSignal;
  let cancellationTimer;
  let cancellationClears = 0;
  window.setTimeout = function(callback, delay, ...args) {
    const id = originalSetTimeout.call(window, callback, delay, ...args);
    if (delay === 100) cancellationTimer = id;
    return id;
  };
  window.clearTimeout = function(id) {
    if (id === cancellationTimer) cancellationClears++;
    originalClearTimeout.call(window, id);
  };
  window.fetch = req => {
    cancelledSignal = req.signal;
    return new Promise(() => {});
  };
  await cancel_request();
  results.push({ name: 'dropping Rust receiver aborts actual Fetch', passed: cancelledSignal?.aborted === true });
  results.push({ name: 'dropping Rust receiver clears its timer exactly once', passed: cancellationClears === 1 });
  window.fetch = originalFetch;
  window.setTimeout = originalSetTimeout;
  window.clearTimeout = originalClearTimeout;
  let active = 0;
  let maximum = 0;
  let dispatched = 0;
  window.fetch = req => {
    dispatched++;
    active++;
    maximum = Math.max(maximum, active);
    req.signal.addEventListener('abort', () => active--, { once: true });
    return new Promise(() => {});
  };
  await cancel_batches();
  results.push({ name: 'three replacing batches never exceed 16 actual live Fetch calls', passed: maximum === 16 && active === 0 && dispatched === 48 });
  window.fetch = originalFetch;
  for (const type of [null, 'text/html', 'application/json; charset=utf-8']) {
    try {
      const url = new URL('/metadata', location.href);
      if (type) url.searchParams.set('type', type);
      const response = await network_request(url.href, 4);
      assert((response[2] ?? null) === type, `wrong Content-Type ${response[2]}`);
      results.push({ name: `actual HTTP metadata ${type}`, passed: true });
    } catch (error) { results.push({ name: `actual HTTP metadata ${type}`, passed: false, error: String(error) }); }
  }
  try {
    let error;
    try { await network_request(new URL('/stream', location.href).href, 4096); }
    catch (caught) { error = caught; }
    assert(error === 'ResponseTooLarge { limit: 4096 }', `unbounded stream result ${error}`);
    await new Promise(resolve => originalSetTimeout.call(window, resolve, 50));
    const stats = await (await originalFetch('/stream-stats')).json();
    assert(stats.closed === 1 && stats.bytes < 32768, `stream was not promptly cancelled: ${JSON.stringify(stats)}`);
    results.push({ name: 'actual HTTP unbounded body stops and closes connection', passed: true });
  } catch (error) { results.push({ name: 'actual HTTP unbounded body stops and closes connection', passed: false, error: String(error) }); }
  try {
    let error;
    try { await network_request(new URL('/compressed', location.href).href, 4096); }
    catch (caught) { error = caught; }
    assert(error === 'ResponseTooLarge { limit: 4096 }', `compressed body bypassed streaming limit: ${error}`);
    results.push({ name: 'actual HTTP small encoded Content-Length cannot bypass decoded body bound', passed: true });
  } catch (error) { results.push({ name: 'actual HTTP small encoded Content-Length cannot bypass decoded body bound', passed: false, error: String(error) }); }
  for (const post of [false, true]) {
    const kind = post ? 'POST' : 'GET';
    const failed = post ? 'Ambiguous' : 'Definite';
    await fixture(`${kind} misleading small Content-Length`, () => Promise.resolve(new Response(new Uint8Array(32), { headers: { 'Content-Length': '1' } })), post, 4,
      post ? 'Ambiguous' : 'ResponseTooLarge { limit: 4 }');
    await fixture(`${kind} full byteLength above u32`, () => bufferResponse(4294967296), post, 4,
      post ? 'Ambiguous' : 'ResponseTooLarge { limit: 4 }');
    for (const length of [NaN, Infinity, -1, 0.5, '4']) {
      await fixture(`${kind} invalid byteLength ${String(length)}`, () => bufferResponse(length), post, 4, failed);
    }
    await fixture(`${kind} unrepresentable Vec length`, () => bufferResponse(2147483648), post, 4294967295,
      post ? 'Ambiguous' : 'ResponseTooLarge { limit: 4294967295 }');
  }
  for (const post of [false, true]) {
    const kind = post ? 'POST' : 'GET';
    const failed = post ? 'Ambiguous' : 'Definite';
    await fixture(`${kind} synchronous Fetch throw`, () => { throw new Error('host fetch failed'); }, post, 4, failed);
    await fixture(`${kind} non-Promise Fetch result`, () => ({}), post, 4, failed);
    await fixture(`${kind} rejected Fetch promise`, () => Promise.reject(new Error('rejected')), post, 4, failed);
    await fixture(`${kind} timeout`, () => new Promise(() => {}), post, 4, failed);
    await fixture(`${kind} status and exact-limit body`, () => Promise.resolve(new Response(new Uint8Array([1, 2, 3, 4]), { status: 503 })),
      post, 4, [503, [1, 2, 3, 4]]);
  }
} catch (error) {
  results.push({ name: 'fixture setup', passed: false, error: String(error) });
} finally {
  window.transportResults.done = true;
}
