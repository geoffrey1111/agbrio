import test from 'node:test';
import assert from 'node:assert/strict';
import {webcrypto} from 'node:crypto';
import {emit,refreshCwd} from '../claude-code-plugin/hooks/register.mjs';
if(!globalThis.crypto)globalThis.crypto=webcrypto;
const sid='10000000-0000-4000-8000-000000000001',instance='20000000-0000-4000-8000-000000000001';
const SPARE=String.raw`C:\Users\x\.claude\spares\spare-a`,PROJECT=String.raw`D:\project`;
function fixture(ok){
 const posts=[];
 const state={identity:{sessionId:sid,cwd:SPARE,version:'2.1.299'},instanceId:instance,origin:'http://127.0.0.1:1',token:'a'.repeat(64),sequence:0,pending:[],queue:Promise.resolve(),stopped:false,delivered:0};
 const $={http:{fetch:async(_,init)=>{posts.push(JSON.parse(init.body));if(!ok())throw new Error('offline');return{ok:true,status:200,text:'{}'};}},session:{cwd:async()=>PROJECT},ui:{log:async()=>{}}};
 return{state,$,posts};
}
test('while Agbrio is unreachable heartbeats never fill the queue or switch the session off',async()=>{
 let online=false;const f=fixture(()=>online);
 await emit(f.$,f.state,'SESSION_START').catch(()=>{});
 for(let n=0;n<200;n++)await emit(f.$,f.state,'HEARTBEAT').catch(()=>{});
 assert.equal(f.state.stopped,false);
 assert.deepEqual(f.state.pending.map(e=>e.kind),['SESSION_START','HEARTBEAT']);
 online=true;await emit(f.$,f.state,'HEARTBEAT');
 assert.equal(f.state.pending.length,0);
 const delivered=f.posts.filter((_,i)=>i>=f.posts.length-2).map(e=>e.kind);assert.deepEqual(delivered,['SESSION_START','HEARTBEAT']);
});
test('a normal session keeps its root when its working directory changes',async()=>{
 const f=fixture(()=>true);f.state.identity={...f.state.identity,cwd:PROJECT};f.$.session.cwd=async()=>String.raw`D:\project\src-tauri`;
 await emit(f.$,f.state,'SESSION_START');await refreshCwd(f.$,f.state);await emit(f.$,f.state,'HEARTBEAT');
 assert.deepEqual(f.posts.map(e=>e.cwd),[PROJECT,PROJECT]);
});
test('a spare session follows its current folder for later events',async()=>{
 const f=fixture(()=>true);await emit(f.$,f.state,'SESSION_START');
 await refreshCwd(f.$,f.state);await emit(f.$,f.state,'HEARTBEAT');
 assert.deepEqual(f.posts.map(e=>e.cwd),[SPARE,PROJECT]);
});
