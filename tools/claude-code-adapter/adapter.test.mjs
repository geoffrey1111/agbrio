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
 register((name,fn)=>{callbacks.set(name,fn);return {catch:()=>{}};},{hostUrl:'http://127.0.0.1:12345',token:'a'.repeat(64)});
 const $={session:{id:async()=>sid,cwd:async()=>cwd,version:async()=>({version:'2.1.286'})},clock:{every:(_,fn)=>{heartbeat=fn;}},http:{fetch:async(url,init)=>{posted.push({url,event:JSON.parse(init.body)});return{ok:true,status:200};}}};
 let passed=0;const next=async e=>{passed++;return e;};
 await callbacks.get('session.start')($,{},next);await heartbeat();
 assert.equal(posted[0].event.kind,'SESSION_START');assert.equal(posted[1].event.kind,'HEARTBEAT');assert.equal(posted[0].event.instanceId,posted[1].event.instanceId);assert.equal(passed,1);
 assert(!callbacks.has('prompt.submit'));assert(!callbacks.has('command.run'));assert(!('model' in $));
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
 const callbacks=new Map(),posted=[];register((name,fn)=>{callbacks.set(name,fn);return {catch:()=>{}};},{hostUrl:'http://127.0.0.1:12345',token:'a'.repeat(64)});
 const $={session:{id:async()=>sid,cwd:async()=>cwd,version:async()=>({version:'2.1.286'})},clock:{every:()=>{},now:async()=>1791763200000},http:{fetch:async(_,init)=>{posted.push(JSON.parse(init.body));return{ok:true};}}};
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
