import init, { request } from './pkg/browser_transport_regressions.js';

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
  const buffer = new ArrayBuffer(0);
  Object.defineProperty(buffer, 'byteLength', { value: length });
  const response = new Response();
  response.arrayBuffer = () => Promise.resolve(buffer);
  return Promise.resolve(response);
}

try {
  await init();
  for (const post of [false, true]) {
    const kind = post ? 'POST' : 'GET';
    const failed = post ? 'Ambiguous' : 'Definite';
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
