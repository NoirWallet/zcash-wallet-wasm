import { chromium, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';
const origin=process.argv[2]??'http://127.0.0.1:8000';
const fixtureDir=process.env.VOTING_FIXTURE_DIR??'/tmp/task-8-browser-fixtures';
const metadata=JSON.parse(await readFile(`${fixtureDir}/metadata.json`,'utf8'));
const browser=await chromium.launch({...(process.env.CHROME?{executablePath:process.env.CHROME}:{}),args:['--no-sandbox']});
try {
  const page=await browser.newPage();
  for(const file of ['wallet.bin','voting.bin']) await page.route(`**/synthetic-fixture/${file}`,route=>route.fulfill({path:`${fixtureDir}/${file}`,contentType:'application/octet-stream'}));
  await page.goto(origin);
  const result=await page.evaluate(async metadata=>{
    const api=await import('/pkg-voting/zcash_wallet_wasm.js');await api.default();await api.initThreadPool(2);
    const {VotingStorage}=await import('/voting_storage.mjs');
    const bytes=async name=>new Uint8Array(await (await fetch(`/synthetic-fixture/${name}`)).arrayBuffer());
    const walletBytes=await bytes('wallet.bin'),votingBytes=await bytes('voting.bin');
    const storage=await VotingStorage.open();await storage.claim();
    await storage.saveWallet(walletBytes,{accountId:metadata.accountId,accountIndex:0,birthday:3771641,lwd:'https://zcash-testnet.chainsafe.dev'});
    const wallet=new api.WebWallet('test','https://zcash-testnet.chainsafe.dev',walletBytes);
    const probe=new api.WebVotingSession(wallet,metadata.accountId,undefined,()=>Promise.resolve());
    const key=probe.storageKey;probe.free();
    // Seed an earlier durable process revision; the reopened Rust counter starts at one.
    await new Promise((resolve,reject)=>{const tx=storage.db.transaction('sessions','readwrite');tx.objectStore('sessions').put({key,revision:'10',bytes:votingBytes.buffer});tx.oncomplete=resolve;tx.onabort=reject;});
    const checkpoint=await storage.bindSession(key);
    const session=new api.WebVotingSession(wallet,metadata.accountId,votingBytes,checkpoint);
    await session.clearRound(metadata.roundId);
    const saved=await storage.read(key),restoredWallet=await storage.read('wallet');
    const current=session.exportState();
    const equal=current.length===saved.bytes.byteLength&&current.every((byte,index)=>byte===new Uint8Array(saved.bytes)[index]);
    const walletEqual=walletBytes.every((byte,index)=>byte===new Uint8Array(restoredWallet.bytes)[index]);
    session.free();wallet.free();await storage.close();
    return {equal,walletEqual,revision:saved.revision,walletLength:walletBytes.length,votingLength:current.length};
  },metadata);
  expect(result.equal).toBe(true);expect(result.walletEqual).toBe(true);expect(result.revision).toBe('11');
  await page.goto(origin+'/test_voting.html');
  await expect(page.locator('#wallet-status')).toHaveText('Wallet restored',{timeout:120000});
  await expect(page.locator('#mnemonic')).toHaveValue('');
  await page.reload();await expect(page.locator('#wallet-status')).toHaveText('Wallet restored',{timeout:120000});
  page.once('dialog',dialog=>dialog.accept());await page.locator('#clear').click();
  await expect(page.locator('#wallet-status')).toHaveText('Local state cleared');
  await page.reload();await expect(page.locator('#initialize')).toBeEnabled({timeout:120000});await expect(page.locator('#sync')).toBeDisabled();
  console.log(JSON.stringify({actualWasmStorage:true,...result}));
} finally {await browser.close();}
