import http from 'node:http';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';
import { gzipSync } from 'node:zlib';

const root = new URL('./', import.meta.url);
const mime = { html: 'text/html', js: 'text/javascript', wasm: 'application/wasm' };
let streamClosed = 0;
let streamBytes = 0;
const server = http.createServer(async (req, res) => {
  try {
    const url = new URL(req.url, 'http://localhost');
    if (url.pathname === '/compressed') {
      const body = gzipSync(Buffer.alloc(512 * 1024));
      res.setHeader('Content-Encoding', 'gzip');
      res.setHeader('Content-Length', body.length);
      res.end(body);
      return;
    }
    if (url.pathname === '/stream') {
      res.setHeader('Content-Type', 'application/json; charset=utf-8');
      const interval = setInterval(() => { streamBytes += 1024; res.write(Buffer.alloc(1024)); }, 10);
      res.on('close', () => { clearInterval(interval); streamClosed++; });
      return;
    }
    if (url.pathname === '/stream-stats') {
      res.setHeader('Content-Type', 'application/json');
      res.end(JSON.stringify({ closed: streamClosed, bytes: streamBytes }));
      return;
    }
    if (url.pathname === '/metadata') {
      const contentType = url.searchParams.get('type');
      if (contentType) res.setHeader('Content-Type', contentType);
      res.end('ok');
      return;
    }
    const path = url.pathname === '/' ? 'index.html' : url.pathname.slice(1);
    const file = new URL(path, root);
    if (!file.href.startsWith(root.href)) throw new Error('invalid path');
    res.setHeader('Content-Type', mime[path.split('.').pop()] ?? 'application/octet-stream');
    res.end(await readFile(fileURLToPath(file)));
  } catch {
    res.writeHead(404).end();
  }
});
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
if (process.env.TRANSPORT_PLAYWRIGHT === '1') {
  const { chromium } = await import('@playwright/test');
  const browser = await chromium.launch({args:['--no-sandbox']});
  try {
    const page = await browser.newPage();
    await page.goto(`http://127.0.0.1:${server.address().port}/`);
    await page.waitForFunction(() => window.transportResults?.done, null, {timeout:30000});
    const results = await page.evaluate(() => window.transportResults.results);
    console.log(JSON.stringify(results,null,2));
    if (!results.length || results.some(result => !result.passed)) process.exitCode = 1;
  } finally {await browser.close();await new Promise(resolve=>server.close(resolve));}
  process.exit(process.exitCode ?? 0);
}
const driver = spawn(process.env.CHROMEDRIVER ?? 'chromedriver', ['--port=0'], { stdio: ['ignore', 'pipe', 'inherit'] });
const driverPort = await new Promise((resolve, reject) => {
  let output = '';
  driver.stdout.on('data', chunk => {
    output += chunk;
    const match = output.match(/started successfully on port (\d+)/);
    if (match) resolve(Number(match[1]));
  });
  driver.on('error', reject);
  driver.on('exit', code => reject(new Error(`ChromeDriver exited ${code}`)));
});
async function command(path, body, method = 'POST') {
  const response = await fetch(`http://127.0.0.1:${driverPort}${path}`, {
    method, headers: { 'Content-Type': 'application/json' },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  const { value } = await response.json();
  if (!response.ok) throw new Error(JSON.stringify(value));
  return value;
}
let session;
try {
  const capabilities = process.env.WASM_BINDGEN_TEST_WEBDRIVER_JSON
    ? JSON.parse(await readFile(process.env.WASM_BINDGEN_TEST_WEBDRIVER_JSON, 'utf8'))
    : { 'goog:chromeOptions': { args: ['--headless', '--no-sandbox'] } };
  capabilities['goog:chromeOptions'].args.push('--headless');
  session = (await command('/session', { capabilities: { alwaysMatch: capabilities } })).sessionId;
  await command(`/session/${session}/timeouts`, { script: 30000 });
  await command(`/session/${session}/url`, { url: `http://127.0.0.1:${server.address().port}/` });
  const results = await command(`/session/${session}/execute/async`, {
    script: `const done = arguments[arguments.length - 1];
      const interval = setInterval(() => {
        if (window.transportResults?.done) {
          clearInterval(interval);
          done(window.transportResults.results);
        }
      }, 20);`, args: [],
  });
  console.log(JSON.stringify(results, null, 2));
  if (results.length === 0 || results.some(result => !result.passed)) process.exitCode = 1;
} finally {
  if (session) await command(`/session/${session}`, undefined, 'DELETE');
  driver.kill();
  server.close();
}
