import test from 'node:test';
import assert from 'node:assert/strict';
import {handleCodexWait,tickCodexWaits,registerCodexWait,waitNotice,appendCodexWait} from '../claude-code-plugin/hooks/codex-wait.mjs';
const sid='10000000-0000-4000-8000-000000000001',tid='20000000-0000-4000-8000-000000000001',turn='30000000-0000-4000-8000-000000000001';
const input={action:'wait',threadId:tid,generation:1,turnId:turn};
function fixture(){
 let time=100000,status='RUNNING',failure=false,submits=0,calls=0;const data=new Map(),hooks={};
 const state={identity:{sessionId:sid,cwd:'D:\\fixture'},instanceId:sid,origin:'http://127.0.0.1:12345',token:'a'.repeat(64),stopped:false};
 const $={clock:{now:async()=>time},store:{get:async k=>structuredClone(data.get(k)),set:async(k,v)=>data.set(k,structuredClone(v))},http:{fetch:async()=>{calls++;if(failure)throw Error('network');return{ok:true,text:JSON.stringify({...input,state:status})};}},prompt:{submit:async()=>{submits++;return{};}}};
 registerCodexWait((n,a,b)=>{hooks[n]=b??a;return{catch:()=>{}};},state);
 hooks['session.append']=async($,e,next)=>appendCodexWait($,state,e,await next(e));
 return{$,state,data,hooks,time:v=>time=v,status:v=>status=v,fail:v=>failure=v,submits:()=>submits,calls:()=>calls,wait:()=>handleCodexWait($,state,input),tick:()=>tickCodexWaits($,state)};
}
test('one exact terminal result wakes once; native acceptance alone stays UNKNOWN',async()=>{
 const f=fixture();await f.wait();await f.tick();assert.equal(f.submits(),0);
 f.status('RESULT_READY');f.time(106000);await f.tick();assert.equal(f.submits(),1);
 f.time(112000);await f.tick();await f.wait();assert.equal(f.submits(),1);
 assert.equal((await handleCodexWait(f.$,f.state,{action:'status'})).waits[0].status,'UNKNOWN');
});
test('durable reservation prevents resend after reload or ambiguous native failure',async()=>{
 const f=fixture();await f.wait();f.status('RESULT_READY');f.$.prompt.submit=async()=>{throw Error('uncertain');};await f.tick();
 f.state.codexWaitQueue=null;f.time(120000);await f.tick();assert.equal(f.submits(),0);
 assert.equal((await f.wait()).wait.status,'UNKNOWN');
});
test('store failure before reservation produces no prompt',async()=>{
 const f=fixture();await f.wait();f.status('RESULT_READY');f.$.store.set=async()=>{throw Error('disk');};await assert.rejects(f.tick);assert.equal(f.submits(),0);
});
test('cancel and expiry stop notices; uncertain attempts cannot be unlocked by cancel',async()=>{
 for(const mode of ['cancel','expire']){const f=fixture();await f.wait();if(mode==='cancel')await handleCodexWait(f.$,f.state,{...input,action:'cancel'});else f.time(3800000);f.status('RESULT_READY');await f.tick();assert.equal(f.submits(),0);}
 const f=fixture();await f.wait();f.status('RESULT_READY');await f.tick();assert.equal((await handleCodexWait(f.$,f.state,{...input,action:'cancel'})).wait.status,'UNKNOWN');
});
test('mismatched source and offline responses never wake; recovery resumes waiting only',async()=>{
 const f=fixture();await f.wait();f.fail(true);await f.tick();assert.equal(f.submits(),0);f.fail(false);f.status('RESULT_READY');f.time(106000);await f.tick();assert.equal(f.submits(),1);
 const g=fixture();g.$.http.fetch=async()=>({ok:true,text:JSON.stringify({...input,turnId:sid,state:'RESULT_READY'})});await assert.rejects(g.wait);assert.equal(g.data.size,0);
});
test('foreign session, stopped session, invalid target and excessive TTL fail closed',async()=>{
 const f=fixture();for(const change of [{threadId:'title'},{turnId:'x'},{generation:0},{ttlSeconds:86401}])await assert.rejects(()=>handleCodexWait(f.$,f.state,{...input,...change}));
 await f.wait();f.state.identity={sessionId:turn,cwd:'D:\\fixture'};assert.deepEqual((await handleCodexWait(f.$,f.state,{action:'status'})).waits,[]);
 f.state.stopped=true;await assert.rejects(f.tick);
});
test('only independently stored exact plugin notification is delivery proof',async()=>{
 const f=fixture();await f.wait();f.status('RESULT_READY');await f.tick();const text=waitNotice(input,'RESULT_READY');
 const e={door:'prompt',origin:{kind:'plugin',name:'agbrio-bridge'}};
 const result={uuid:sid,message:{type:'user',content:text}};
 for(const extra of [{agentId:'child'},{origin:{kind:'user'}}])await f.hooks['session.append'](f.$,{...e,...extra},async()=>result);
 assert.equal((await f.wait()).wait.status,'UNKNOWN');
 await f.hooks['session.append'](f.$,e,async()=>({...result,message:{type:'user',content:'changed'}}));assert.equal((await f.wait()).wait.status,'UNKNOWN');
 await f.hooks['session.append'](f.$,e,async()=>result);assert.equal((await f.wait()).wait.status,'DELIVERED');
});
test('parallel wait and tick deduplicate; no body or user identity is submitted',async()=>{
 const f=fixture();await Promise.all([f.wait(),f.wait()]);f.status('ACTION_REQUIRED');let submitted;f.$.prompt.submit=async v=>{submitted=v;};await Promise.all([f.tick(),f.tick()]);
 assert.equal((await handleCodexWait(f.$,f.state,{action:'status'})).waits.length,1);assert.equal(submitted.asUser,undefined);assert.match(submitted.text,/metadata only/);
});
test('heartbeat without any registered wait performs no source request',async()=>{const f=fixture();await f.tick();assert.equal(f.calls(),0);});
test('shared plugin storage keeps simultaneous Claude sessions independent across reload',async()=>{
 const f=fixture();await f.wait();f.$.http.fetch=async(_,init)=>({ok:true,text:JSON.stringify({...JSON.parse(init.body),state:'RUNNING'})});const other={...f.state,identity:{sessionId:turn,cwd:'D:\\fixture'},codexWaitQueue:null};
 await handleCodexWait(f.$,other,{...input,threadId:sid});
 assert.equal((await handleCodexWait(f.$,f.state,{action:'status'})).waits[0].threadId,tid);
 assert.equal((await handleCodexWait(f.$,other,{action:'status'})).waits[0].threadId,sid);
 const reloaded={...f.state,instanceId:turn,codexWaitQueue:null};assert.equal((await handleCodexWait(f.$,reloaded,{action:'status'})).waits[0].status,'WAITING');
});
