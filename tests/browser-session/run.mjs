import http from 'node:http';
import { readFile } from 'node:fs/promises';
import { spawn } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';

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
const driver = spawn(process.env.CHROMEDRIVER ?? 'chromedriver',['--port=0'],{stdio:['ignore','pipe','inherit']});
const port = await new Promise((resolve,reject)=>{
  let output='';
  driver.stdout.on('data',chunk=>{ output+=chunk; const match=output.match(/started successfully on port (\d+)/); if(match)resolve(Number(match[1])); });
  driver.on('error',reject); driver.on('exit',code=>reject(new Error(`ChromeDriver exited ${code}`)));
});
async function command(path, body, method='POST') {
  const response=await fetch(`http://127.0.0.1:${port}${path}`,{method,headers:{'Content-Type':'application/json'},...(body===undefined?{}:{body:JSON.stringify(body)})});
  const {value}=await response.json();
  if(!response.ok)throw new Error(JSON.stringify(value));
  return value;
}
let session;
try {
  const capabilities=process.env.WASM_BINDGEN_TEST_WEBDRIVER_JSON?JSON.parse(await readFile(process.env.WASM_BINDGEN_TEST_WEBDRIVER_JSON,'utf8')):{'goog:chromeOptions':{args:['--headless','--no-sandbox']}};
  capabilities['goog:chromeOptions'].args.push('--headless');
  capabilities['goog:loggingPrefs'] = { browser: 'ALL' };
  session=(await command('/session',{capabilities:{alwaysMatch:capabilities}})).sessionId;
  await command(`/session/${session}/timeouts`,{script:Number(process.env.SESSION_TEST_TIMEOUT_MS ?? 60000)});
  await command(`/session/${session}/url`,{url:`http://127.0.0.1:${server.address().port}/`});
  const results=await command(`/session/${session}/execute/async`,{script:`const done=arguments[arguments.length-1]; const timer=setInterval(()=>{if(window.sessionResults?.done){clearInterval(timer);done(window.sessionResults.results);}},20);`,args:[]}).catch(async error => {
    console.error(await command(`/session/${session}/log`, {type:'browser'}));
    throw error;
  });
  console.log(JSON.stringify(results,null,2));
  if(!results.length||results.some(result=>!result.passed))process.exitCode=1;
} finally {
  if(session)await command(`/session/${session}`,undefined,'DELETE');
  driver.kill();server.close();
}
