/** Owner-approved private presentation cache. No auth, review token, draft or receipt is stored. */
import type {BoardAsset, BoardSnapshot} from './CharacterBoard';

type Row={key:string;scope:string;kind:'snapshot'|'asset';bytes:number;touched:number;value:BoardSnapshot|BoardAsset;hash?:string};
type Metadata=Omit<Row,'value'|'hash'>;
const DATABASE='agbrio.character-board.v1',STORE='presentation',META='lru';
export const BOARD_CACHE_LIMIT=80*1024*1024;
export const digest=async(value:string|Uint8Array)=>[...new Uint8Array(await crypto.subtle.digest('SHA-256',typeof value==='string'?new TextEncoder().encode(value):new Uint8Array(value)))].map(b=>b.toString(16).padStart(2,'0')).join('');
export async function verifyBoardAsset(asset:BoardAsset,id:string){
 if(asset.assetId!==id||!/^image\/(png|jpeg|gif|webp)$|^text\/plain;charset=utf-8$/.test(asset.mime)||!/^[a-f0-9]{64}$/.test(asset.contentHash)||!/^[a-f0-9]{64}$/.test(asset.sourceHash)||asset.data.length>12*1024*1024)throw Error('BOARD_IMAGE_IDENTITY_MISMATCH');
 if(await digest(Uint8Array.from(atob(asset.data),c=>c.charCodeAt(0)))!==asset.contentHash)throw Error('BOARD_IMAGE_CONTENT_MISMATCH');
 return asset;
}
export class BoardPersistentCache{
 private scope:string|null=null;private epoch=0;private database?:Promise<IDBDatabase>;private serial:Promise<unknown>=Promise.resolve();
 constructor(private limit=BOARD_CACHE_LIMIT){}
 generation(){return this.epoch;}
 private open(){
  return this.database??=new Promise<IDBDatabase>((resolve,reject)=>{
   if(typeof indexedDB==='undefined')return reject(Error('IDB_UNAVAILABLE'));
   let ended=false;const timer=setTimeout(()=>{ended=true;reject(Error('IDB_TIMEOUT'));},1000);
   const request=indexedDB.open(DATABASE,2);request.onupgradeneeded=()=>{
    if(request.result.objectStoreNames.contains(STORE))request.result.deleteObjectStore(STORE);
    request.result.createObjectStore(STORE,{keyPath:'key'});request.result.createObjectStore(META,{keyPath:'key'});
   };
   request.onerror=()=>{ended=true;clearTimeout(timer);reject(request.error);};request.onblocked=()=>{ended=true;clearTimeout(timer);reject(Error('IDB_BLOCKED'));};
   request.onsuccess=()=>{clearTimeout(timer);if(ended){request.result.close();return;}request.result.onversionchange=()=>{request.result.close();this.database=undefined;};resolve(request.result);};
  }).catch(e=>{this.database=undefined;throw e;});
 }
 private run<T>(work:(db:IDBDatabase)=>Promise<T>,fallback:T):Promise<T>{
  const result=this.serial.then(()=>this.open()).then(work).catch(()=>fallback);this.serial=result;return result;
 }
 private transaction<T>(db:IDBDatabase,work:(store:IDBObjectStore,answer:(value:T)=>void,metadata:IDBObjectStore)=>void):Promise<T>{
  return new Promise((resolve,reject)=>{let result:T;const tx=db.transaction([STORE,META],'readwrite');
   const timer=setTimeout(()=>{try{tx.abort();}catch{}reject(Error('IDB_TRANSACTION_TIMEOUT'));},1500);
   tx.oncomplete=()=>{clearTimeout(timer);resolve(result);};tx.onerror=()=>{clearTimeout(timer);reject(tx.error);};tx.onabort=()=>{clearTimeout(timer);reject(tx.error);};work(tx.objectStore(STORE),v=>{result=v;},tx.objectStore(META));
  });
 }
 setScope(scope:string|null){
  if(this.scope===scope&&scope!==null)return;this.scope=scope;this.epoch++;
  void this.run(db=>this.transaction<void>(db,(store,answer,metadata)=>{
   metadata.put({key:'__scope',scope});const all=metadata.getAll();all.onsuccess=()=>{for(const row of all.result as Metadata[])if(row.key!=='__scope'&&(!scope||row.scope!==scope)){store.delete(row.key);metadata.delete(row.key);}answer();};
  }),undefined);
 }
 clear(){this.scope=null;this.epoch++;return this.run(db=>this.transaction<void>(db,(store,answer,metadata)=>{store.clear();metadata.clear();answer();}),undefined);}
 private async read(kind:Row['kind'],id:string){
  const scope=this.scope,epoch=this.epoch;if(!scope)return null;
  return this.run(db=>this.transaction<Row|null>(db,(store,answer,metadata)=>{const owner=metadata.get('__scope');owner.onsuccess=()=>{
   if(owner.result?.scope!==scope){answer(null);return;}const get=store.get(`${scope}:${kind}:${id}`);get.onsuccess=()=>{
   const row=get.result as Row|undefined;if(this.epoch!==epoch||!row||row.scope!==scope){answer(null);return;}
   row.touched=Date.now();const {value:_value,hash:_hash,...meta}=row;metadata.put(meta);answer(row);
  };};}),null);
 }
 private async put(kind:Row['kind'],id:string,value:Row['value'],hash?:string){
  const scope=this.scope,epoch=this.epoch;if(!scope)return;
  const bytes=new TextEncoder().encode(JSON.stringify(value)).length;if(bytes>this.limit)return;
  await this.run(db=>this.transaction<void>(db,(store,answer,metadata)=>{
   if(epoch!==this.epoch||scope!==this.scope){answer();return;}
   const row:Row={key:`${scope}:${kind}:${id}`,scope,kind,bytes,touched:Date.now(),value,hash};
   const owner=metadata.get('__scope');owner.onsuccess=()=>{if(owner.result?.scope!==scope){answer();return;}
   // LRU scans only small metadata, never copies the entire80MiB of image bytes.
   const all=metadata.getAll();all.onsuccess=()=>{
    if(epoch!==this.epoch){answer();return;}
    const existing=(all.result as Metadata[]).filter(r=>r.key!=='__scope');
    const rows=existing.filter(r=>r.key!==row.key&&r.bytes>0&&r.bytes<=this.limit&&!(kind==='snapshot'&&r.kind==='snapshot'));
    for(const r of existing)if(!rows.includes(r)){store.delete(r.key);metadata.delete(r.key);}
    let size=rows.reduce((n,r)=>n+r.bytes,bytes);rows.sort((a,b)=>a.touched-b.touched);
    for(const old of rows){if(size<=this.limit)break;store.delete(old.key);metadata.delete(old.key);size-=old.bytes;}
    store.put(row);const {value:_value,hash:_hash,...meta}=row;metadata.put(meta);answer();
   };};
  }),undefined);
 }
 async snapshot(){
  const scope=this.scope,epoch=this.epoch;const row=await this.read('snapshot','latest');if(!row)return null;
  try{const value=row.value as BoardSnapshot;if(typeof value.html!=='string'||value.html.length>16*1024*1024||!Array.isArray(value.assets)||value.assets.length>4096||await digest(JSON.stringify(value))!==row.hash)throw Error('BAD_SNAPSHOT');return epoch===this.epoch&&scope===this.scope?value:null;}catch{return null;}
 }
 async saveSnapshot(value:BoardSnapshot){const epoch=this.epoch;if(!this.scope||!value.html)return;const hash=await digest(JSON.stringify(value));if(epoch===this.epoch)await this.put('snapshot','latest',value,hash);}
 async asset(id:string,variant:string){const epoch=this.epoch;const row=await this.read('asset',`${id}:${variant}`);if(!row)return null;try{const value=await verifyBoardAsset(row.value as BoardAsset,id);return epoch===this.epoch?value:null;}catch{return null;}}
 async saveAsset(asset:BoardAsset,variant:string){if(variant==='original')return;const epoch=this.epoch;await verifyBoardAsset(asset,asset.assetId);if(epoch===this.epoch)await this.put('asset',`${asset.assetId}:${variant}`,asset);}
}
export const boardCache=new BoardPersistentCache();
