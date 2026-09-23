import { chromium } from '@playwright/test';
const origin=process.argv[2]??'http://127.0.0.1:8000';
const packages=process.argv.slice(3);
if(!packages.length) packages.push('pkg-proof-sequential','pkg-proof');
const browser=await chromium.launch({...(process.env.CHROME?{executablePath:process.env.CHROME}:{}),args:['--no-sandbox']});
try {
  for(const name of packages) {
    const page=await browser.newPage();
    await page.goto(origin+'/');
    const started=Date.now();
    // Proof outputs and fixture recovery bytes never cross the JS boundary.
    const result=await Promise.race([
      page.evaluate(async name=>{
        const wasm=await import(`/${name}/zcash_wallet_wasm.js`);await wasm.default();
        const parallel=typeof wasm.initThreadPool==='function';
        if(parallel) await wasm.initThreadPool(2);
        if(typeof wasm.votingTestOwnedProof!=='function') throw new Error('owned proof export missing');
        return {parallel,proved:await wasm.votingTestOwnedProof()};
      },name),
      new Promise((_,reject)=>{const timer=setTimeout(()=>reject(new Error('owned proof exceeded 30 minute bound')),1800000);timer.unref();}),
    ]);
    if(!result.proved||result.parallel!==!name.endsWith('-sequential')) throw new Error('incorrect proof executor artifact');
    console.log(JSON.stringify({package:name,...result,elapsedMs:Date.now()-started}));
    await page.close();
  }
} finally {await browser.close();}
