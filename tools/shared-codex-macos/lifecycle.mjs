// Internal candidate only. The native product admission remains UNSUPPORTED.
// Protocol: https://learn.chatgpt.com/docs/app-server
import {spawn} from 'node:child_process';
import {createHash,randomUUID} from 'node:crypto';
import {open,mkdir,lstat,readFile,rename,unlink,readdir,realpath} from 'node:fs/promises';
import path from 'node:path';
import {setTimeout as pause} from 'node:timers/promises';

const fail=code=>{throw Error(code);};
const readMethods=new Set(['thread/loaded/list','thread/read','model/list']);
export const realPackageVerified=false; // No env/config/remote switch can override native admission.

// Owns one child's pipe handles, never adopts a PID or scans other sessions.
export class StdioSession {
 constructor(child,{timeout=5000}={}){
  this.child=child;this.timeout=timeout;this.pending=new Map();this.next=0;
  this.revision=0;this.questions=new Set();this.closed=false;this.eofSent=false;this.buffer='';
  child.stdin.on('error',()=>{this.poisoned=true;this.abort();});
  child.once('error',()=>{if(!child.pid)this.closed=true;this.poisoned=true;this.abort();});
  child.once('exit',(code,signal)=>{this.exit={code,signal};});
  child.once('close',()=>{this.closed=true;this.abort();});
  child.stdout.setEncoding('utf8');child.stdout.on('data',chunk=>this.receive(chunk));
  // Native stderr is deliberately neither retained nor projected to callers.
  child.stderr?.resume();
 }
 abort(){for(const p of this.pending.values()){clearTimeout(p.timer);p.reject(Error('SHARED_MAC_TRANSPORT_UNCONFIRMED'));}this.pending.clear();}
 receive(chunk){
  if(this.poisoned)return;
  this.buffer+=chunk;
  if(Buffer.byteLength(this.buffer)>1024*1024){this.buffer='';this.poisoned=true;this.abort();return;}
  for(;;){const at=this.buffer.indexOf('\n');if(at<0)break;const line=this.buffer.slice(0,at);this.buffer=this.buffer.slice(at+1);let v;
   try{v=JSON.parse(line);}catch{this.buffer='';this.poisoned=true;this.abort();return;}
   if(!v||typeof v!=='object'||Array.isArray(v)){this.poisoned=true;this.abort();return;}
   if(v.method){
    this.revision++;
    if(v.id!==undefined)this.questions.add(v.id); // No automatic approval/answer.
    if(this.questions.size>512){this.poisoned=true;this.abort();return;}
    if(v.method==='serverRequest/resolved')this.questions.delete(v.params?.requestId);
    continue;
   }
   const p=this.pending.get(v.id);if(!p)continue;this.pending.delete(v.id);clearTimeout(p.timer);
   v.error?p.reject(Error('SHARED_MAC_RPC_REJECTED')):Object.hasOwn(v,'result')?p.resolve(v.result):p.reject(Error('SHARED_MAC_RESPONSE_INVALID'));
  }
 }
 request(method,params={}){
  if(!['initialize',...readMethods].includes(method))return Promise.reject(Error('SHARED_MAC_LIFECYCLE_READ_ONLY'));
  if(method==='initialize'){if(this.handshakeSent)return Promise.reject(Error('SHARED_MAC_ALREADY_INITIALIZED'));this.handshakeSent=true;}
  else if(!this.initialized)return Promise.reject(Error('SHARED_MAC_NOT_INITIALIZED'));
  if(this.closed||this.poisoned||this.eofSent)return Promise.reject(Error('SHARED_MAC_TRANSPORT_UNCONFIRMED'));
  const id=++this.next;
  return new Promise((resolve,reject)=>{
   const timer=setTimeout(()=>{this.pending.delete(id);this.poisoned=true;reject(Error('SHARED_MAC_RPC_UNCONFIRMED'));},this.timeout);
   this.pending.set(id,{resolve,reject,timer});this.child.stdin.write(JSON.stringify({id,method,params})+'\n');
  });
 }
 async initialize(){
  const value=await this.request('initialize',{clientInfo:{name:'agbrio_shared_macos',title:'Agbrio',version:'0.1.0'}});
  if(!value||typeof value.userAgent!=='string'||!value.userAgent.trim())fail('SHARED_MAC_INITIALIZE_INVALID');
  await new Promise((resolve,reject)=>this.child.stdin.write(JSON.stringify({method:'initialized',params:{}})+'\n',error=>error?reject(Error('SHARED_MAC_TRANSPORT_UNCONFIRMED')):resolve()));
  this.initialized=true;
  const loaded=await this.request('thread/loaded/list');
  if(!Array.isArray(loaded?.data))fail('SHARED_MAC_INITIALIZE_INVALID');
 }
 async idle(){
  if(this.closed||this.poisoned||this.questions.size)return false;
  const revision=this.revision;
  const list=async()=>{
   const ids=new Set();let cursor;const cursors=new Set();
   for(let pages=0;pages<32;pages++){
    const value=await this.request('thread/loaded/list',cursor?{cursor}:{});
    if(!Array.isArray(value?.data)||value.data.some(id=>typeof id!=='string'||!id||id.length>256)||value.data.length>1024)fail('SHARED_MAC_ACTIVITY_UNCONFIRMED');
    for(const id of value.data){if(ids.has(id))fail('SHARED_MAC_ACTIVITY_UNCONFIRMED');ids.add(id);}
    if(value.nextCursor==null)return [...ids].sort();
    if(typeof value.nextCursor!=='string'||cursors.has(value.nextCursor))fail('SHARED_MAC_ACTIVITY_UNCONFIRMED');
    cursor=value.nextCursor;cursors.add(cursor);
   }fail('SHARED_MAC_ACTIVITY_UNCONFIRMED');
  };
  const ids=await list();
  for(const id of ids){const value=await this.request('thread/read',{threadId:id});const thread=value?.thread;
   if(thread?.id!==id||thread.status?.type!=='idle'||(thread.status.activeFlags?.length??0)!==0)return false;
  }
  return revision===this.revision&&!this.questions.size&&JSON.stringify(ids)===JSON.stringify(await list())&&revision===this.revision;
 }
 async end(){
  if(this.eofSent)fail('SHARED_MAC_CLOSE_UNCONFIRMED');
  this.eofSent=true;this.child.stdin.end(); // One EOF; no invented shutdown RPC or signal retry.
  const deadline=Date.now()+this.timeout;
  while(!this.closed&&Date.now()<deadline)await pause(10);
  if(!this.closed||this.exit?.code!==0||this.exit?.signal)fail('SHARED_MAC_CLOSE_UNCONFIRMED');
 }
}

// This is an internal, desktop-local adapter, not an MCP executable/argv API.
// No downloader, shell, provider/key override, permission or model operation.
export function ownedAppServer(binary,{expectedSha256,home,codexHome}={}){
 return async()=>{
  if(process.platform!=='darwin'||!path.isAbsolute(binary)||!home||!path.isAbsolute(home)||!codexHome||!path.isAbsolute(codexHome)||!/^[a-f0-9]{64}$/.test(expectedSha256??''))fail('SHARED_MAC_BINARY_UNVERIFIED');
  if(await realpath(binary)!==binary)fail('SHARED_MAC_BINARY_UNVERIFIED');
  const fd=await open(binary,'r');
  try{const info=await fd.stat();if(!info.isFile()||info.mode&0o6000||!(info.mode&0o111))fail('SHARED_MAC_BINARY_UNVERIFIED');
   if(createHash('sha256').update(await fd.readFile()).digest('hex')!==expectedSha256)fail('SHARED_MAC_BINARY_UNVERIFIED');
  }finally{await fd.close();}
  const child=spawn(binary,['app-server','--listen','stdio://'],{shell:false,stdio:['pipe','pipe','pipe'],env:{...process.env,HOME:home,CODEX_HOME:codexHome}});
  return new StdioSession(child);
 };
}

// Intent is durable before any child operation. Crash/restart never adopts a
// remembered PID or retries PREPARING/ACTIVATING/CLOSING/UNKNOWN automatically.
export class Journal {
 constructor(root){if(!path.isAbsolute(root))fail('SHARED_MAC_STORAGE_INVALID');this.root=root;}
 async validate(file){
  const m=await lstat(file);
  if(m.isSymbolicLink()||(process.getuid&&(m.uid!==process.getuid()||(m.mode&0o077))))fail('SHARED_MAC_STORAGE_UNTRUSTED');
  return m;
 }
 async locked(action){
  await mkdir(this.root,{recursive:true,mode:0o700});
  if(await realpath(this.root)!==this.root||!(await this.validate(this.root)).isDirectory())fail('SHARED_MAC_STORAGE_UNTRUSTED');
  const entries=await readdir(this.root);
  if(entries.some(name=>!['state.json','operation.lock'].includes(name)))fail('SHARED_MAC_FOREIGN_PRESERVED');
  const file=path.join(this.root,'operation.lock');let lock;
  try{lock=await open(file,'wx',0o600);}catch{fail('SHARED_MAC_BUSY_OR_UNCONFIRMED');}
  try{return await action();}finally{
   const original=await lock.stat();await lock.close();
   const current=await lstat(file).catch(()=>null);
   if(current&&current.ino===original.ino&&current.dev===original.dev&&!current.isSymbolicLink())await unlink(file);
  }
 }
 async read(){
  const file=path.join(this.root,'state.json');let info;
  try{info=await this.validate(file);}catch(error){if(error.code==='ENOENT')return {phase:'STOPPED',epoch:null};throw error;}
  if(!info.isFile()||info.nlink!==1||info.size>4096)fail('SHARED_MAC_STORAGE_UNTRUSTED');
  let value;try{value=JSON.parse(await readFile(file,'utf8'));}catch{fail('SHARED_MAC_STORAGE_UNTRUSTED');}
  if(value.schema!==1||!['STOPPED','PREPARING','PREPARED','ACTIVATING','ACTIVE','CLOSING','UNKNOWN'].includes(value.phase)||typeof value.epoch!=='string'||!/^[-a-f0-9]{36}$/.test(value.epoch))fail('SHARED_MAC_STORAGE_UNTRUSTED');
  return {phase:value.phase,epoch:value.epoch};
 }
 async write(value){
  const file=path.join(this.root,'state.json');const existing=await lstat(file).catch(error=>{if(error.code!=='ENOENT')throw error;});
  if(existing){await this.validate(file);if(!existing.isFile()||existing.nlink!==1)fail('SHARED_MAC_STORAGE_UNTRUSTED');}
  const temporary=path.join(this.root,'.state-'+randomUUID());const handle=await open(temporary,'wx',0o600);
  try{await handle.writeFile(JSON.stringify({schema:1,...value}));await handle.sync();}finally{await handle.close();}
  try{await rename(temporary,file);}finally{await unlink(temporary).catch(error=>{if(error.code!=='ENOENT')throw error;});}
 }
}

export class MacSharedLifecycle {
 constructor({journal,createSession}){this.journal=journal;this.createSession=createSession;this.session=null;this.epoch=null;this.operating=false;}
 async operation(action){if(this.operating)fail('SHARED_MAC_BUSY');this.operating=true;try{return await this.journal.locked(action);}finally{this.operating=false;}}
 status(value){const live=!!this.session&&!this.session.closed&&!this.session.poisoned&&!this.session.eofSent;return {phase:value.phase,epoch:value.epoch,ready:['PREPARED','ACTIVE'].includes(value.phase)&&live,active:value.phase==='ACTIVE'&&live&&this.activeEpoch===value.epoch,desktopConnected:false,realPackageVerified};}
 async inspect(){return this.status(await this.journal.read());}
 same(value,epoch){if(!epoch||epoch!==value.epoch)fail('SHARED_MAC_EPOCH_CHANGED');if(!this.session||this.epoch!==epoch||this.session.closed||this.session.poisoned)fail('SHARED_MAC_RECONCILE_REQUIRED');}
 async unknown(value){await this.journal.write({...value,phase:'UNKNOWN'});}
 async prepare(){return this.operation(async()=>{
  const value=await this.journal.read();
  if(['PREPARED','ACTIVE'].includes(value.phase)){this.same(value,value.epoch);return this.status(value);}
  if(value.phase!=='STOPPED')fail('SHARED_MAC_RECONCILE_REQUIRED');
  const next={phase:'PREPARING',epoch:randomUUID()};await this.journal.write(next);this.epoch=next.epoch;
  try{this.session=await this.createSession();await this.session.initialize();next.phase='PREPARED';await this.journal.write(next);return this.status(next);}
  catch(error){await this.unknown(next);throw error;}
 });}
 async activate(epoch){return this.operation(async()=>{
  const value=await this.journal.read();this.same(value,epoch);
  if(value.phase==='ACTIVE')return this.status(value);
  if(value.phase!=='PREPARED')fail('SHARED_MAC_RECONCILE_REQUIRED');
  try{if(!await this.session.idle())fail('SHARED_MAC_ACTIVITY_UNCONFIRMED');}catch(error){if(this.session.poisoned||this.session.closed)await this.unknown(value);throw error;}
  await this.journal.write({...value,phase:'ACTIVATING'});
  // Activation admits this exact initialized protocol connection. It is not
  // a fabricated Codex Desktop attachment or a thread/start/model operation.
  await this.journal.write({...value,phase:'ACTIVE'});this.activeEpoch=epoch;return this.status({...value,phase:'ACTIVE'});
 });}
 connection(epoch){if(this.operating||epoch!==this.activeEpoch||epoch!==this.epoch||!this.session||this.session.closed||this.session.poisoned||this.session.eofSent)fail('SHARED_MAC_CONNECTION_UNAVAILABLE');return this.session;}
 async close(epoch){return this.operation(async()=>{
  const value=await this.journal.read();
  if(value.phase==='STOPPED'&&value.epoch===epoch)return this.status(value);
  if(!['PREPARED','ACTIVE'].includes(value.phase))fail('SHARED_MAC_RECONCILE_REQUIRED');this.same(value,epoch);
  try{if(!await this.session.idle())fail('SHARED_MAC_ACTIVITY_UNCONFIRMED');}catch(error){if(this.session.poisoned||this.session.closed)await this.unknown(value);throw error;}
  await this.journal.write({...value,phase:'CLOSING'});
  this.activeEpoch=null;
  try{await this.session.end();await this.journal.write({...value,phase:'STOPPED'});return this.status({...value,phase:'STOPPED'});}
  catch(error){await this.unknown(value);throw error;}
 });}
 async reconcile(epoch){return this.operation(async()=>{
  const value=await this.journal.read();if(epoch!==value.epoch)fail('SHARED_MAC_EPOCH_CHANGED');
  // Only the same in-memory child handle proves exit. A saved PID is not proof.
  if(this.epoch!==epoch||!this.session?.closed)fail('SHARED_MAC_RECONCILE_REQUIRED');
  await this.journal.write({...value,phase:'STOPPED'});return this.status({...value,phase:'STOPPED'});
 });}
}
