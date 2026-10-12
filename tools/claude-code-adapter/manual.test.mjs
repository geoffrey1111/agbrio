import test from 'node:test';
import assert from 'node:assert/strict';
import {webcrypto} from 'node:crypto';
import {acceptManual,registerManual,textHash,flushReceipt} from '../claude-code-plugin/hooks/register.mjs';
if(!globalThis.crypto)globalThis.crypto=webcrypto;
const sid='10000000-0000-4000-8000-000000000001',instance='20000000-0000-4000-8000-000000000001',rid='30000000-0000-4000-8000-000000000001',row='40000000-0000-4000-8000-000000000001';
async function fixture(){
 const store=new Map(),posts=[];let submits=0;let appended;
 const state={identity:{sessionId:sid,cwd:'D:\\fixture'},instanceId:instance,origin:'http://127.0.0.1:12345',token:'a'.repeat(64),stopped:false};
 const command={requestId:rid,sessionId:sid,instanceId:instance,cwd:state.identity.cwd,text:'Exact approved\n中文\n```code```'};command.textSha256=await textHash(command.text);
 const $={store:{get:async key=>store.get(key),set:async(key,v)=>store.set(key,structuredClone(v))},http:{fetch:async(_,init)=>{posts.push(JSON.parse(init.body));return{ok:true};}},prompt:{submit:async({text})=>{submits++;return{text};}}};
 registerManual((name,fn)=>{assert.equal(name,'session.append');appended=fn;return{catch:()=>{}};},state);
 return{$,state,command,store,posts,submits:()=>submits,append:async(extra={})=>{const e={door:'prompt',origin:{kind:'plugin',name:'agbrio-bridge'},uuid:row,message:{type:'user',role:'user',content:[{type:'text',text:command.text}]},...extra};return appended($,e,async()=>({uuid:row,message:e.message}));}};
}
test('submission is not SENT until the exact final native row is stored; replay never resubmits',async()=>{
 const f=await fixture();await acceptManual(f.$,f.state,f.command);assert.equal(f.submits(),1);assert.equal(f.posts.length,0);assert(f.state.manualActive);
 await f.append();assert.equal(f.posts.at(-1).status,'SENT');assert.equal(f.posts.at(-1).messageId,row);assert.equal(f.state.manualActive,null);
 await acceptManual(f.$,f.state,f.command);assert.equal(f.submits(),1);assert.equal(f.posts.at(-1).status,'SENT');
});
test('subagent, foreign origin and changed content cannot become delivery proof',async()=>{
 const f=await fixture();await acceptManual(f.$,f.state,f.command);
 for(const extra of [{agentId:'child'},{origin:{kind:'user'}},{door:'note'},{message:{type:'user',content:[{type:'text',text:'rewritten'}]}}])await f.append(extra);
 assert.equal(f.posts.length,0);assert(f.state.manualActive);await f.append();assert.equal(f.posts.at(-1).status,'SENT');
});
test('changed native instance or text hash is refused before all mutation',async()=>{
 const f=await fixture();for(const changed of [{instanceId:rid},{cwd:'D:\\elsewhere'},{sessionId:rid},{text:'changed'}])await assert.rejects(()=>acceptManual(f.$,f.state,{...f.command,...changed}));assert.equal(f.submits(),0);assert.equal(f.store.size,0);
});
test('durable store failure makes zero submissions; unknown old attempts never retry',async()=>{
 const f=await fixture();f.$.store.set=async()=>{throw Error('disk unavailable');};await assert.rejects(()=>acceptManual(f.$,f.state,f.command));assert.equal(f.submits(),0);
 const g=await fixture();g.$.prompt.submit=async()=>{throw Error('uncertain physical submission');};await assert.rejects(()=>acceptManual(g.$,g.state,g.command));g.state.manualActive=null;let retried=0;g.$.prompt.submit=async()=>{retried++;};await acceptManual(g.$,g.state,g.command);assert.equal(retried,0);assert.equal(g.posts.at(-1).status,'UNKNOWN');
});
test('lost receipt transport retries only the same receipt, never the prompt',async()=>{
 const f=await fixture();await acceptManual(f.$,f.state,f.command);const fetch=f.$.http.fetch;f.$.http.fetch=async()=>{throw Error('offline');};await f.append();assert.equal(f.state.manualReceipt.status,'SENT');assert.equal(f.submits(),1);f.$.http.fetch=fetch;await flushReceipt(f.$,f.state);assert.equal(f.state.manualReceipt,null);assert.equal(f.posts.at(-1).messageId,row);assert.equal(f.submits(),1);
});
test('a native queued delivery may be meta while still storing the exact approved user-role text',async()=>{
 const f=await fixture();await acceptManual(f.$,f.state,f.command);await f.append({door:'delivery',message:{type:'user',role:'user',isMeta:true,content:[{type:'text',text:f.command.text}]}});assert.equal(f.posts.at(-1).status,'SENT');assert.equal(f.posts.at(-1).messageId,row);assert.equal(f.submits(),1);
});
