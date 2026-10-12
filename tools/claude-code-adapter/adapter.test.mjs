import test from 'node:test';
import assert from 'node:assert/strict';
import {createHash,webcrypto} from 'node:crypto';
import {hostOrigin,register} from '../claude-code-plugin/hooks/register.mjs';
import {executeReviewedCommand} from './control.mjs';
if(!globalThis.crypto)globalThis.crypto=webcrypto;
const sid='10000000-0000-4000-8000-000000000001',uid='20000000-0000-4000-8000-000000000001',mid='30000000-0000-4000-8000-000000000001';
const cwd='D:\\fixture';
test('private adapter endpoint accepts only exact loopback origins',()=>{
 assert.equal(hostOrigin('http://127.0.0.1:12345'),'http://127.0.0.1:12345');
 for(const url of ['https://example.invalid','http://127.0.0.1:12345/x','http://127.0.0.1:12345?token=x','http://user:pw@127.0.0.1:12345','http://localhost:12345','file:///x'])assert.throws(()=>hostOrigin(url));
});
function api(){const saved=new Map();let submits=0,aborts=0;return {store:{get:async k=>saved.get(k),set:async(k,v)=>{saved.set(k,v);}},prompt:{submit:async({text})=>{submits++;return{text};}},turn:{abort:async()=>{aborts++;}},counts:()=>({submits,aborts}),saved};}
const context={sessionId:sid,instanceId:uid,cwd,bindingRevision:2,state:'IDLE',turnId:mid};
const command={sessionId:sid,instanceId:uid,cwd,bindingRevision:2,requestId:mid,ownerApproved:true,kind:'SEND',text:'隔离原文',textSha256:createHash('sha256').update('隔离原文').digest('hex')};
test('default quota boundary calls no prompt, abort or storage API',async()=>{
 const $=api();assert.equal((await executeReviewedCommand($,command,context)).status,'NOT_SENT');assert.deepEqual($.counts(),{submits:0,aborts:0});assert.equal($.saved.size,0);
});
test('offline reviewed dispatch fences and durable replay suppress duplicate submits',async()=>{
 const $=api();for(const patch of [{sessionId:uid},{bindingRevision:3},{ownerApproved:false},{textSha256:'bad'}])await assert.rejects(()=>executeReviewedCommand($,{...command,...patch},context,{writesEnabled:true}));
 assert.deepEqual($.counts(),{submits:0,aborts:0});
 const first=await executeReviewedCommand($,command,context,{writesEnabled:true});const again=await executeReviewedCommand($,command,context,{writesEnabled:true});
 assert.equal(first.status,'ACCEPTED');assert.deepEqual(again,first);assert.equal($.counts().submits,1);
 // Provider callback uncertainty is sticky, never classified as no delivery.
 const unknown=api();unknown.prompt.submit=async()=>{throw new Error('lost callback');};
 assert.equal((await executeReviewedCommand(unknown,command,context,{writesEnabled:true})).status,'UNKNOWN');
 assert.equal((await executeReviewedCommand(unknown,command,context,{writesEnabled:true})).status,'UNKNOWN');
});
test('observation registration sends metadata only; no control handler or model calls',async()=>{
 const callbacks=new Map(),posted=[];let heartbeat;
 register((name,...args)=>{callbacks.set(name,args.at(-1));return {catch:()=>{}};},{hostUrl:'http://127.0.0.1:12345',token:'a'.repeat(64)});
 const $={command:{register:async()=>{}},ui:{log:async()=>{}},session:{id:async()=>sid,cwd:async()=>cwd,version:async()=>({version:'2.1.286'})},clock:{every:(_,fn)=>{heartbeat=fn;}},http:{fetch:async(url,init)=>{posted.push({url,event:JSON.parse(init.body)});return{ok:true,status:200};}}};
 let passed=0;const next=async e=>{passed++;return e;};
 await callbacks.get('session.start')($,{},next);await heartbeat();
 assert.equal(posted[0].event.kind,'SESSION_START');assert.equal(posted[1].event.kind,'HEARTBEAT');assert.equal(posted[0].event.instanceId,posted[1].event.instanceId);assert.equal(passed,1);
 assert(!callbacks.has('prompt.submit'));assert(callbacks.has('command.run'));assert(!('model' in $));
});
test('concurrent duplicates submit once; reused request identity cannot change payload',async()=>{
 const $=api();const replies=await Promise.all([executeReviewedCommand($,command,context,{writesEnabled:true}),executeReviewedCommand($,command,context,{writesEnabled:true})]);
 assert.equal(replies[0].status,'ACCEPTED');assert.equal($.counts().submits,1);
 const text='different';await assert.rejects(()=>executeReviewedCommand($,{...command,text,textSha256:createHash('sha256').update(text).digest('hex')},context,{writesEnabled:true}),/REPLAY_CONFLICT/);
 assert.equal($.counts().submits,1);
});
test('native interruption pins expected turn and pending approval never becomes permission',async()=>{
 const $=api();const stop={...command,kind:'STOP',expectedTurnId:mid};
 for(const change of [{pendingApproval:true},{unknownSend:true}])await assert.rejects(()=>executeReviewedCommand($,stop,{...context,...change},{writesEnabled:true}));
 await assert.rejects(()=>executeReviewedCommand($,{...stop,expectedTurnId:uid},context,{writesEnabled:true}));
 assert.equal((await executeReviewedCommand($,stop,context,{writesEnabled:true})).status,'ACCEPTED');assert.equal($.counts().aborts,1);
});
test('SDK turn.complete uses exact native turn and full final answer; interruption is not a reply',async()=>{
 const callbacks=new Map(),posted=[];register((name,...args)=>{callbacks.set(name,args.at(-1));return {catch:()=>{}};},{hostUrl:'http://127.0.0.1:12345',token:'a'.repeat(64)});
 const $={command:{register:async()=>{}},ui:{log:async()=>{}},session:{id:async()=>sid,cwd:async()=>cwd,version:async()=>({version:'2.1.286'})},clock:{every:()=>{},now:async()=>1791763200000},http:{fetch:async(_,init)=>{posted.push(JSON.parse(init.body));return{ok:true};}}};
 const next=async e=>e;
 await callbacks.get('session.start')($,{},next);
 await callbacks.get('turn.start')($,{turnId:mid},next);
 const answer='完整答复\n\n```text\n原代码块\n```';
 await callbacks.get('turn.complete')($,{turnId:mid,reason:'answer',isAborted:false,answer},next);
 await new Promise(resolve=>setImmediate(resolve));
 assert.equal(typeof posted.at(-1).completedAt,'number');assert.equal(posted.at(-1).kind,'COMPLETE');assert.equal(posted.at(-1).text,answer);assert.equal(posted.at(-1).turnId,mid);
 await callbacks.get('turn.complete')($,{turnId:mid,reason:'aborted',isAborted:true,answer:'partial'},next);
 await new Promise(resolve=>setImmediate(resolve));
 assert.equal(posted.at(-1).kind,'INTERRUPTED');assert.equal(posted.at(-1).text,null);
 assert(!callbacks.has('classic.Stop')); // No transcript-file reader needed.
});

test('permission observation error never retries an already-called native request',async()=>{
 let permissionError;register((name,fn)=>({catch:handler=>{if(name==='classic.PermissionRequest')permissionError=handler;}}),{hostUrl:'http://127.0.0.1:12345',token:'a'.repeat(64)});
 let calls=0;const next=async()=>{calls++;return{native:true};};next.called=true;
 await assert.rejects(()=>permissionError({}, {}, next),/AGBRIO_NATIVE_PERMISSION_UNCONFIRMED/);assert.equal(calls,0);
 next.called=false;assert.deepEqual(await permissionError({}, {}, next),{native:true});assert.equal(calls,1);
});

test('connection failures expose stages without secret or raw exceptions; retries retain event identity',async()=>{
 const callbacks=new Map(),logs=[],posted=[];let heartbeat;let ok=false;
 register((name,...args)=>{callbacks.set(name,args.at(-1));return{catch:()=>{}};},{hostUrl:'http://127.0.0.1:12345',token:'a'.repeat(64)});
 const $={command:{register:async()=>{}},ui:{log:async t=>logs.push(t)},session:{id:async()=>sid,cwd:async()=>cwd,version:async()=>({version:'2.1.295'})},clock:{every:(_,f)=>heartbeat=f},http:{fetch:async(_,init)=>{posted.push(JSON.parse(init.body));return{ok,status:ok?200:403};}}};
 let passed=0;await callbacks.get('session.start')($,{},async()=>passed++);await heartbeat();
 const failed=JSON.parse((await callbacks.get('command.run')()).text);
 assert.equal(failed.phase,'HTTP');assert.equal(failed.httpStatus,403);assert.equal(failed.error,'AGBRIO_ADAPTER_DELIVERY_UNCONFIRMED');assert.equal(logs.length,1);assert.equal(passed,1);
 assert(!JSON.stringify(logs).includes('a'.repeat(64)));assert(!JSON.stringify(logs).includes(sid));assert(!JSON.stringify(logs).includes(cwd));
 assert.equal(posted[0].eventId,posted[1].eventId);
 ok=true;await heartbeat();const healthy=JSON.parse((await callbacks.get('command.run')()).text);assert.equal(healthy.phase,'CONNECTED');assert.equal(healthy.error,null);assert.equal(healthy.pendingEvents,0);
});
test('identity API failures pass through and report sanitized stage without publishing',async()=>{
 const callbacks=new Map(),logs=[];let posts=0,passed=0;
 register((name,...args)=>{callbacks.set(name,args.at(-1));return{catch:()=>{}};},{hostUrl:'http://127.0.0.1:12345',token:'a'.repeat(64)});
 const $={command:{register:async()=>{}},ui:{log:async t=>logs.push(t)},session:{id:async()=>sid,cwd:async()=>cwd,version:async()=>{throw new Error('PRIVATE_CALLBACK_TOKEN_'+('a'.repeat(64)));}},http:{fetch:async()=>posts++}};
 await callbacks.get('session.start')($,{},async()=>passed++);await new Promise(resolve=>setImmediate(resolve));
 const status=JSON.parse((await callbacks.get('command.run')()).text);assert.equal(status.phase,'VERSION');assert.equal(status.error,'AGBRIO_VERSION_FAILED');assert.equal(status.identityReady,false);assert.equal(passed,1);assert.equal(posts,0);
 assert.equal(logs.length,1);assert(!logs[0].includes('PRIVATE_CALLBACK'));assert(!logs[0].includes('a'.repeat(64)));
});
