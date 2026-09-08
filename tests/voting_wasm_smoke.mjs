import { chromium } from '@playwright/test';
const origin=process.argv[2]??'http://127.0.0.1:8000';
const lwd=process.argv[3]??'https://zcash-testnet.chainsafe.dev';
const browser=await chromium.launch({...(process.env.CHROME?{executablePath:process.env.CHROME}:{}),args:['--no-sandbox']});
let deadline;
try {
  const page=await browser.newPage();
  page.on('requestfailed',request=>{const url=new URL(request.url());console.log(JSON.stringify({networkFailure:{host:url.host,path:url.pathname,error:request.failure()?.errorText}}));});
  await page.goto(origin+'/');
  const observation=await Promise.race([page.evaluate(async lwd=>{
    const wasm=await import('/pkg-voting/zcash_wallet_wasm.js');await wasm.default();
    if(typeof wasm.initThreadPool==='function') await wasm.initThreadPool(2);
    if(typeof wasm.votingTestOwnedProof!=='undefined') throw new Error('test fixture leaked into release');
    const nativeFetch=globalThis.fetch;
    const signed=new Set();const requests=[];
    globalThis.fetch=async (input,options)=>{
      const url=new URL(input instanceof Request?input.url:String(input),location.href);
      const method=options?.method??(input instanceof Request?input.method:'GET');
      const isLwd=url.origin===new URL(lwd).origin;
      const allowed=isLwd
        ? /\/(GetLatestBlock|GetTreeState)$/.test(url.pathname)
        : method==='GET' && (url.pathname.endsWith('.json')||url.pathname.endsWith('/shielded-vote/v1/rounds/active'));
      if(!allowed) throw Object.assign(new Error('non-read-only request blocked'),{name:`BLOCKED_${method}_${url.pathname}`});
      requests.push({host:url.host,path:url.pathname,method});
      const response=await nativeFetch(input,options);
      requests[requests.length-1].status=response.status;
      requests[requests.length-1].contentType=response.headers.get('content-type');
      if(!isLwd&&url.pathname.endsWith('.json')) {
        const config=await response.clone().json();
        if(config.config_version && config.rounds) for(const [id,auth] of Object.entries(config.rounds)) {
          if(auth.auth_version===2 && auth.signatures?.length) signed.add(id.toLowerCase());
        }
      }
      return response;
    };
    let wallet,session,stage='height';
    try {
      wallet=new wasm.WebWallet('test',lwd,undefined);
      const height=await wallet.get_latest_height();
      stage='account';
      const account=await wallet.create_account('Read-only smoke',wasm.generate_mnemonic(24),0,height,'');
      session=new wasm.WebVotingSession(wallet,account,undefined,()=>{throw new Error('read-only work checkpointed');});
      stage='discovery';
      const discovery=await session.discoverRounds();
      if(!discovery.accepted.every(round=>signed.has(round.roundId))) throw new Error('unsigned accepted round');
      return {height,accepted:discovery.accepted.map(({roundId,name})=>({roundId,name})),rejected:discovery.rejected.map(({roundId,code})=>({roundId,code})),configFingerprint:discovery.configFingerprint,requests};
    } catch(error) {throw new Error(`Read-only smoke failed at ${stage}: ${String(error?.name??'unknown')}; ${JSON.stringify(requests)}`);}
    finally {globalThis.fetch=nativeFetch;session?.free();wallet?.free();}
  },lwd),new Promise((_,reject)=>{deadline=setTimeout(()=>reject(new Error('Read-only smoke exceeded five minute bound')),300000);})]);
  console.log(JSON.stringify(observation,null,2));
} finally {clearTimeout(deadline);await browser.close();}
