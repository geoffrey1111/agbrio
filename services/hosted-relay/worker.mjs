// Agbrio hosted HTTP relay. No provider credentials or transcript persistence.
import {DurableObject} from 'cloudflare:workers';
const MAX_BODY=16*1024*1024, MAX_FRAME=23*1024*1024;
const json=(value,status=200)=>Response.json(value,{status,headers:{'Cache-Control':'no-store'}});
const fail=(code,status=400)=>json({error:code},status);
function unavailable(req,code,status){const u=new URL(req.url);if(req.method!=='GET'||!['/','/mobile'].includes(u.pathname)||!req.headers.get('Accept')?.includes('text/html'))return fail(code,status);const zh=(req.headers.get('Accept-Language')??'').toLowerCase().startsWith('zh'),expired=code==='SUBSCRIPTION_INACTIVE';const title=zh?(expired?'连接已到期或停用':'电脑暂时离线'):(expired?'Connection expired or disabled':'Computer offline');const note=zh?(expired?'在电脑 Agbrio 的设备设置中使用新兑换码续期。':'保持电脑和 Agbrio 运行，然后重试。'):(expired?'Renew in Agbrio’s device settings on your computer.':'Keep your computer and Agbrio running, then retry.');const label=zh?'重试':'Retry';return new Response(`<!doctype html><html lang="${zh?'zh':'en'}"><meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover"><meta name="color-scheme" content="light"><title>Agbrio</title><style>body{margin:0;background:#fffcf8;color:#302c27;font:16px system-ui;min-height:100dvh;display:grid;place-items:center}main{padding:32px;max-width:440px}h1{font-size:24px}p{line-height:1.6;color:#746a5d}button{background:#aa5735;border:0;border-radius:24px;padding:14px 24px;color:white;font:inherit;cursor:pointer}</style><main><strong>Agbrio · Agent Bridge</strong><h1>${title}</h1><p>${note}</p><form><button>${label}</button></form></main></html>`,{status,headers:{'Content-Type':'text/html; charset=utf-8','Cache-Control':'no-store'}});}
const uuid=()=>crypto.randomUUID().replaceAll('-','');
const validId=x=>typeof x==='string'&&/^[a-f0-9]{32}$/.test(x);
const validKey=x=>typeof x==='string'&&/^[a-f0-9]{64}$/.test(x);
export async function hash(text){return [...new Uint8Array(await crypto.subtle.digest('SHA-256',new TextEncoder().encode(text)))].map(x=>x.toString(16).padStart(2,'0')).join('');}
async function hostToken(env,id,key){const k=await crypto.subtle.importKey('raw',new TextEncoder().encode(env.ADMIN_TOKEN),{name:'HMAC',hash:'SHA-256'},false,['sign']);return [...new Uint8Array(await crypto.subtle.sign('HMAC',k,new TextEncoder().encode(`host:${id}:${key}`)))].map(x=>x.toString(16).padStart(2,'0')).join('');}
async function boundedBody(req,max){if(Number(req.headers.get('Content-Length'))>max)throw Error('TOO_LARGE');if(!req.body)return new Uint8Array();const reader=req.body.getReader(),chunks=[];let size=0;while(true){const {done,value}=await reader.read();if(done)break;size+=value.length;if(size>max){await reader.cancel();throw Error('TOO_LARGE');}chunks.push(value);}const all=new Uint8Array(size);let at=0;for(const c of chunks){all.set(c,at);at+=c.length;}return all;}
async function readJSON(req,max=16384){return JSON.parse(new TextDecoder().decode(await boundedBody(req,max)));}
function b64(bytes){let s='';for(let i=0;i<bytes.length;i+=16384)s+=String.fromCharCode(...bytes.subarray(i,i+16384));return btoa(s);}
function unb64(s){if(typeof s!=='string'||s.length>MAX_FRAME||!/^[A-Za-z0-9+/]*={0,2}$/.test(s))throw Error('INVALID_BODY');return Uint8Array.from(atob(s),c=>c.charCodeAt(0));}
function safeHeaders(headers,response=false){const allowed=response?['content-type','cache-control','etag','last-modified','content-disposition','www-authenticate','location','set-cookie','x-aiwr-host-instance']:['content-type','accept','cookie','authorization','origin','if-none-match','if-modified-since','range'];return [...headers].filter(([k])=>allowed.includes(k.toLowerCase()));}
export default {async fetch(req,env){try{
 const u=new URL(req.url);if(u.protocol!=='https:')return fail('TLS_REQUIRED',400);
 if(u.hostname===env.CONTROL_HOST){
  if(u.pathname==='/health'&&req.method==='GET')return json({service:'Agbrio hosted relay',version:1});
  if(req.method!=='POST')return fail('NOT_FOUND',404);
  const origin=req.headers.get('Origin');if(origin&&origin!==u.origin)return fail('ORIGIN_REJECTED',403);
  if(u.pathname.startsWith('/admin/')&&req.headers.get('Authorization')!==`Bearer ${env.ADMIN_TOKEN}`)return fail('UNAUTHORIZED',401);
  if(!['/v1/redeem','/admin/issue','/admin/ready','/admin/revoke','/admin/expire','/admin/status'].includes(u.pathname))return fail('NOT_FOUND',404);
  return env.CONTROL.get(env.CONTROL.idFromName('control-v1')).fetch(req);
 }
 const suffix='.'+env.ZONE_ROOT;const label=u.hostname.endsWith(suffix)?u.hostname.slice(0,-suffix.length):'';
 if(!/^ag-[a-f0-9]{32}$/.test(label))return fail('NOT_FOUND',404);
 if(u.pathname==='/__agbrio/entrance'&&req.method==='GET')return json({service:'Agbrio hosted relay',version:1});
 return env.TENANTS.get(env.TENANTS.idFromName(label.slice(3))).fetch(req);
 }catch{return fail('RELAY_REQUEST_FAILED',400);}}};

export class Control extends DurableObject {
 constructor(ctx,env){super(ctx,env);this.ctx=ctx;this.env=env;}
 async fetch(req){return this.ctx.blockConcurrencyWhile(async()=>{try{
  const p=new URL(req.url).pathname, input=await readJSON(req,p==='/admin/issue'?32768:16384), s=this.ctx.storage;
  if(p==='/admin/issue'){
   if(!Array.isArray(input.codes)||input.codes.length<1||input.codes.length>60)return fail('INVALID_BATCH');
   const ids=new Set(),hashes=new Set();for(const c of input.codes){if(!validKey(c.hash)||!validId(c.tenantId)||![7,30,365].includes(c.days)||ids.has(c.tenantId)||hashes.has(c.hash))return fail('INVALID_CODE');ids.add(c.tenantId);hashes.add(c.hash);const old=await s.get('c:'+c.hash),allocated=await s.get('a:'+c.tenantId);if(old&&(old.days!==c.days||old.tenantId!==c.tenantId)||allocated&&allocated!==c.hash)return fail('ISSUANCE_CONFLICT',409);}
   for(const c of input.codes)if(!await s.get('c:'+c.hash))await s.put({['c:'+c.hash]:{...c,ready:false,revoked:false},['a:'+c.tenantId]:c.hash});
   return json({codes:input.codes.map(c=>({hash:c.hash,tenantId:c.tenantId,hostname:`ag-${c.tenantId}.${this.env.ZONE_ROOT}`}))});
  }
  if(p==='/admin/ready'){
   if(!Array.isArray(input.hashes)||input.hashes.length>60)return fail('INVALID_BATCH');
   for(const h of input.hashes){const c=await s.get('c:'+h);if(!c)return fail('CODE_NOT_FOUND',404);}
   for(const h of input.hashes){const c=await s.get('c:'+h);await s.put('c:'+h,{...c,ready:true});}return json({ready:true});
  }
  if(p==='/admin/status'){const codes=await s.list({prefix:'c:'});return json({codes:[...codes.values()].map(c=>({hash:c.hash,tenantId:c.usedTenant??c.tenantId,days:c.days,ready:c.ready,redeemedAt:c.redeemedAt??null,expiresAt:c.expiresAt??null,revoked:c.revoked}))});}
  if(p==='/admin/revoke'){
   if(!validKey(input.hash))return fail('INVALID_CODE');const c=await s.get('c:'+input.hash);if(!c)return fail('CODE_NOT_FOUND',404);
   await s.put('c:'+input.hash,{...c,revoked:true});if(c.usedTenant){const t=await s.get('t:'+c.usedTenant);await s.put('t:'+c.usedTenant,{...t,revoked:true});await this.env.TENANTS.get(this.env.TENANTS.idFromName(c.usedTenant)).configure({...t,revoked:true});}return json({revoked:true});
  }
  if(p==='/admin/expire'){
   if(!validKey(input.hash))return fail('INVALID_CODE');const c=await s.get('c:'+input.hash);if(!c?.usedTenant)return fail('SUBSCRIPTION_NOT_FOUND',404);const t=await s.get('t:'+c.usedTenant),expired={...t,expiresAt:Date.now()-1};await s.put('t:'+c.usedTenant,expired);await this.env.TENANTS.get(this.env.TENANTS.idFromName(c.usedTenant)).configure(expired);return json({expired:true});
  }
  if(p!=='/v1/redeem')return fail('NOT_FOUND',404);
  if(typeof input.code!=='string'||!/^AGB(?:7|30|365)-[a-f0-9]{64}$/.test(input.code.trim())||!validKey(input.deviceKey))return fail('INVALID_CODE');
  const codeHash=await hash(input.code.trim()), deviceHash=await hash(input.deviceKey), c=await s.get('c:'+codeHash);
  if(!c||c.revoked)return fail('CODE_UNAVAILABLE',403);if(!c.ready)return fail('SERVICE_NOT_READY',503);
  if(c.deviceHash&&c.deviceHash!==deviceHash)return fail('CODE_ALREADY_USED',409);
  const existing=await s.get('d:'+deviceHash), id=c.usedTenant??existing??c.tenantId, prior=await s.get('t:'+id);
  if(prior?.revoked)return fail('SUBSCRIPTION_REVOKED',403);
  const token=await hostToken(this.env,id,input.deviceKey), origin=`https://ag-${id}.${this.env.ZONE_ROOT}`;
  const expiresAt=c.deviceHash?(prior?.expiresAt??c.expiresAt):(Math.max(Date.now(),prior?.expiresAt??0)+c.days*86400000);
  const t={id,origin,expiresAt,tokenHash:await hash(token),revoked:false};
  // Persist the one-use decision before RPC; retry finishes the same lease.
  await s.put({['c:'+codeHash]:{...c,deviceHash,usedTenant:id,expiresAt,redeemedAt:c.redeemedAt??Date.now()},['d:'+deviceHash]:id,['t:'+id]:t});
  await this.env.TENANTS.get(this.env.TENANTS.idFromName(id)).configure(t);
  return json({tenantId:id,origin,expiresAt,hostToken:token});
 }catch{return fail('CONTROL_REQUEST_FAILED');}});}
}

export class Tenant extends DurableObject {
 constructor(ctx,env){super(ctx,env);this.ctx=ctx;this.env=env;this.pending=new Map();this.minute=0;this.requests=0;}
 async configure(t){return this.ctx.blockConcurrencyWhile(async()=>{await this.ctx.storage.put('lease',t);if(t.revoked||t.expiresAt<=Date.now()){for(const ws of this.ctx.getWebSockets()){ws.send(JSON.stringify({type:'inactive',state:t.revoked?'REVOKED':'EXPIRED'}));ws.close(1008,'Subscription inactive');}this.finishPending();}await this.schedule();});}
 async lease(){return this.ctx.storage.get('lease');}
 async schedule(){const t=await this.lease();const deadlines=this.ctx.getWebSockets().map(w=>w.deserializeAttachment()?.deadline).filter(x=>x>Date.now());if(t&&!t.revoked&&t.expiresAt>Date.now())deadlines.push(t.expiresAt);if(deadlines.length)await this.ctx.storage.setAlarm(Math.min(...deadlines));}
 async alarm(){const t=await this.lease();for(const ws of this.ctx.getWebSockets()){const a=ws.deserializeAttachment();if(!t||t.revoked||t.expiresAt<=Date.now()||!a?.authorized&&a?.deadline<=Date.now()){ws.send(JSON.stringify({type:'inactive',state:t?.revoked?'REVOKED':'EXPIRED'}));ws.close(1008,'Connection expired');}}if(!t||t.revoked||t.expiresAt<=Date.now())this.finishPending();await this.schedule();}
 finishPending(){for(const p of this.pending.values()){clearTimeout(p.timer);p.resolve(fail('HOST_DISCONNECTED_UNCERTAIN',503));}this.pending.clear();}
 async fetch(req){
  const t=await this.lease(),u=new URL(req.url);if(!t||u.origin!==t.origin)return fail('NOT_FOUND',404);if(t.revoked||t.expiresAt<=Date.now())return unavailable(req,'SUBSCRIPTION_INACTIVE',402);
  if(u.pathname==='/__agbrio/connector'){
   if(req.headers.get('Upgrade')?.toLowerCase()!=='websocket'||req.headers.has('Origin'))return fail('CONNECTOR_ONLY',403);
   const protocols=(req.headers.get('Sec-WebSocket-Protocol')??'').split(',').map(x=>x.trim());const token=protocols.find(x=>x.startsWith('cap-'))?.slice(4);
   if(!protocols.includes('agbrio')||!validKey(token)||await hash(token)!==t.tokenHash)return fail('UNAUTHORIZED',401);
   if(this.ctx.getWebSockets().some(w=>w.deserializeAttachment()?.authorized))return fail('CONNECTOR_BUSY',409);
   const pair=new WebSocketPair();this.ctx.acceptWebSocket(pair[1]);pair[1].serializeAttachment({authorized:true,deadline:t.expiresAt});pair[1].send(JSON.stringify({type:'ready',expiresAt:t.expiresAt}));await this.schedule();return new Response(null,{status:101,webSocket:pair[0],headers:{'Sec-WebSocket-Protocol':'agbrio'}});
  }
  if(u.pathname.startsWith('/__agbrio/')||u.pathname.startsWith('/admin/'))return fail('NOT_FOUND',404);
  const minute=Math.floor(Date.now()/60000);if(minute!==this.minute){this.minute=minute;this.requests=0;}if(++this.requests>240)return fail('RATE_LIMIT',429);
  const socket=this.ctx.getWebSockets().find(w=>w.deserializeAttachment()?.authorized);if(!socket)return unavailable(req,'COMPUTER_OFFLINE',503);
  if(this.pending.size>=12)return fail('HOST_BUSY',429);
  const origin=req.headers.get('Origin');if(origin&&origin!==t.origin)return fail('ORIGIN_REJECTED',403);
  if(!['GET','HEAD','POST','DELETE'].includes(req.method))return fail('METHOD_REJECTED',405);
  if(Number(req.headers.get('Content-Length'))>MAX_BODY)return fail('TOO_LARGE',413);
  let body;try{body=await boundedBody(req,MAX_BODY);}catch{return fail('TOO_LARGE',413);}
  const id=uuid(),frame={type:'request',id,method:req.method,path:u.pathname+u.search,headers:safeHeaders(req.headers),body:b64(body)};
  return new Promise(resolve=>{const timer=setTimeout(()=>{this.pending.delete(id);resolve(fail('HOST_TIMEOUT_UNCERTAIN',504));},60000);this.pending.set(id,{resolve,timer});try{socket.send(JSON.stringify(frame));}catch{clearTimeout(timer);this.pending.delete(id);resolve(fail('HOST_DISCONNECTED_UNCERTAIN',503));}});
 }
 async webSocketMessage(ws,message){try{
  const t=await this.lease(),a=ws.deserializeAttachment();if(!t||t.revoked||t.expiresAt<=Date.now()){ws.close(1008,'Subscription inactive');return;}
  if(typeof message!=='string'||message.length>MAX_FRAME){ws.close(1009,'Too large');return;}const f=JSON.parse(message);
  if(!a?.authorized){if(f.type!=='auth'||!validKey(f.token)||await hash(f.token)!==t.tokenHash||this.ctx.getWebSockets().some(w=>w!==ws&&w.deserializeAttachment()?.authorized)){ws.close(1008,'Authentication rejected');return;}ws.serializeAttachment({authorized:true,deadline:t.expiresAt});ws.send(JSON.stringify({type:'ready',expiresAt:t.expiresAt}));return;}
  if(f.type!=='response'||!validId(f.id))return;const p=this.pending.get(f.id);if(!p)return;
  if(!Number.isInteger(f.status)||f.status<200||f.status>599||!Array.isArray(f.headers))throw Error('INVALID_RESPONSE');
  const body=unb64(f.body);if(body.length>MAX_BODY)throw Error('TOO_LARGE');const headers=new Headers(f.headers);const result=new Response([204,205,304].includes(f.status)?null:body,{status:f.status,headers:safeHeaders(headers,true)});
  clearTimeout(p.timer);this.pending.delete(f.id);p.resolve(result);
 }catch{ws.close(1008,'Invalid frame');this.finishPending();}}
 webSocketClose(ws){if(ws.deserializeAttachment()?.authorized)this.finishPending();}
 webSocketError(ws){if(ws.deserializeAttachment()?.authorized)this.finishPending();}
}
