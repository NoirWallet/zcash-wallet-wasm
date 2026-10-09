const DATABASE = 'zcash-voting-testnet';
const request = req => new Promise((resolve,reject) => {
  req.onsuccess=()=>resolve(req.result); req.onerror=()=>reject(req.error);
});
const complete = tx => new Promise((resolve,reject) => {
  tx.oncomplete=()=>resolve(); tx.onabort=tx.onerror=()=>reject(new Error('PERSISTENCE_FAILED'));
});

export class VotingStorage {
  static async open() {
    const req=indexedDB.open(DATABASE,1);
    req.onupgradeneeded=()=>req.result.createObjectStore('sessions',{keyPath:'key'});
    return new VotingStorage(await request(req));
  }
  constructor(db) { this.db=db; this.generation=0; this.bound=new Set(); }
  async claim() {
    if(this.release) return;
    if(!navigator.locks) throw new Error('STORAGE_OWNERSHIP_UNAVAILABLE');
    let acquired,failed;
    const ready=new Promise((resolve,reject)=>{acquired=resolve;failed=reject;});
    this.lockDone=navigator.locks.request(DATABASE,{ifAvailable:true},async lock=>{
      if(!lock) { failed(new Error('STORAGE_IN_USE')); return; }
      await new Promise(resolve=>{this.release=resolve;acquired();});
    });
    this.lockDone.catch(failed);
    await ready;
  }
  assertOwner(generation=this.generation) {
    if(this.poisoned || !this.release || generation!==this.generation) throw new Error('PERSISTENCE_FAILED');
  }
  async read(key) {
    const tx=this.db.transaction('sessions','readonly'),done=complete(tx);
    const result=await request(tx.objectStore('sessions').get(key));
    await done; return result;
  }
  async saveWallet(bytes,metadata) {
    this.assertOwner();
    const tx=this.db.transaction('sessions','readwrite'),done=complete(tx);
    tx.objectStore('sessions').put({key:'wallet',...metadata,bytes:bytes.slice().buffer});
    await done;
  }
  async bindSession(key) {
    this.assertOwner();
    if(this.bound.has(key)) throw new Error('PERSISTENCE_FAILED');
    this.bound.add(key);
    const generation=this.generation;
    const existing=await this.read(key);
    const offset=BigInt(existing?.revision ?? 0);
    this.assertOwner(generation);
    return async (bytes,revision) => {
      this.assertOwner(generation);
      if(typeof revision!=='bigint' || revision<=0n) throw new Error('PERSISTENCE_FAILED');
      const snapshot=bytes.slice().buffer;
      const tx=this.db.transaction('sessions','readwrite'),done=complete(tx);
      const store=tx.objectStore('sessions');
      const pending=store.get(key);
      // Keep the compare and write inside the same live IDB transaction.
      pending.onsuccess=()=>{
        if(!this.release || generation!==this.generation || BigInt(pending.result?.revision ?? 0)>=offset+revision) {
          tx.abort(); return;
        }
        store.put({key,revision:(offset+revision).toString(),bytes:snapshot});
      };
      try {await done;} catch(error) {this.poisoned=true;throw error;}
    };
  }
  async clear() {
    this.assertOwner();
    this.generation++;
    const tx=this.db.transaction('sessions','readwrite'),done=complete(tx);
    tx.objectStore('sessions').clear();
    await done;
    await this.close();
  }
  async close() {
    this.generation++;
    const release=this.release;
    this.release=undefined;
    release?.();
    await this.lockDone;
    this.bound.clear();
  }
}
