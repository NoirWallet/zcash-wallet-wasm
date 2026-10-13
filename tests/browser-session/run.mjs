import http from 'node:http';
import { readFile } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';
import { chromium } from '@playwright/test';

const root = new URL('./', import.meta.url);
const packageRoot = pathToFileURL(resolve(process.env.SESSION_WASM_DIR ?? '/tmp/zcash-voting-bindings') + '/');
const server = http.createServer(async (req, res) => {
  try {
    const path = new URL(req.url, 'http://localhost').pathname;
    const base = path.startsWith('/pkg/') ? packageRoot
      : path.startsWith('/fixtures/') ? new URL('file:///tmp/task-8-browser-fixtures/') : root;
    // Match a bundler's package-entry resolution for the existing Rayon helper.
    const relative = path === '/' ? 'index.html' : path === '/pkg/' ? 'zcash_wallet_wasm.js' : path.replace(/^\/(pkg\/|fixtures\/)?/, '');
    const file = new URL(relative, base);
    if (!file.href.startsWith(base.href)) throw new Error('invalid path');
    res.setHeader('Cross-Origin-Opener-Policy','same-origin');
    res.setHeader('Cross-Origin-Embedder-Policy','require-corp');
    res.setHeader('Content-Type',file.pathname.endsWith('.wasm')?'application/wasm':file.pathname.endsWith('.js')?'text/javascript':'text/html');
    res.end(await readFile(file));
  } catch { res.writeHead(404).end(); }
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
let browser;
try {
  browser = await chromium.launch({
    ...(process.env.CHROME ? { executablePath: process.env.CHROME } : {}),
    args: ['--no-sandbox'],
  });
  const page = await browser.newPage();
  const errors = [];
  page.on('pageerror', error => errors.push(String(error)));
  page.on('console', message => {
    if (message.type() === 'error') errors.push(message.text());
  });
  await page.goto(`http://127.0.0.1:${server.address().port}/${process.env.SESSION_TEST_PAGE ?? ''}`);
  await page.waitForFunction(() => window.sessionResults?.done, undefined, {
    timeout: Number(process.env.SESSION_TEST_TIMEOUT_MS ?? 60000),
  });
  const results = await page.evaluate(() => window.sessionResults.results);
  console.log(JSON.stringify(results,null,2));
  if(!results.length||results.some(result=>!result.passed))process.exitCode=1;
  if(process.env.SESSION_TEST_PAGE==='release-smoke.html' && errors.length) {
    console.error(JSON.stringify(errors,null,2));
    process.exitCode=1;
  }
} finally {
  await browser?.close();
  server.close();
}
