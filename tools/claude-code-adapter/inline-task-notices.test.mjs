import test from 'node:test';import assert from 'node:assert/strict';import {webcrypto} from 'node:crypto';
import {prepareInlineTaskNotices,finishInlineTaskNotices,resumeDeferredTaskNotices,tickTaskNotices,inlineFrame} from '../claude-code-plugin/hooks/codex-notices.mjs';
if(!globalThis.crypto)globalThis.crypto=webcrypto;
const sid='10000000-0000-4000-8000-000000000001',rowId='40000000-0000-4000-8000-000000000001',notice='a'.repeat(64);
function fixture(){
 let time=10000,claimed=false,settled=false;const data=new Map(),hooks={},posts=[],prompts=[];
 const state={identity:{sessionId:sid,cwd:'D:/workspace'},instanceId:sid,origin:'http://127.0.0.1:12345',token:'a'.repeat(64),resourceBusy:true};
 const text='Agbrio Codex task notice (metadata only)\nnoticeId: '+notice+'\nthreadId: exact\nkind: QUESTION\n';
 const $={clock:{now:async()=>time,sleep:()=>new Promise(()=>{})},store:{get:async key=>structuredClone(data.get(key)),set:async(key,value)=>data.set(key,structuredClone(value))},http:{fetch:async(_url,init)=>{
  const p=JSON.parse(init.body);posts.push(p);if(p.action==='CLAIM'){claimed=true;return{ok:true,text:'{"accepted":true}'};}if(p.action==='ACK'){settled=true;return{ok:true,text:'{"accepted":true}'};}
  return{ok:true,text:JSON.stringify({subscriptions:[],inlineNoticeProtocol:1,notice:claimed||settled||p.excludeNoticeIds?.includes(notice)?null:{id:notice,text},settled:[]})};
 }},prompt:{submit:async value=>{prompts.push(value);return{};}}};
 hooks['session.append']=async($,e,next)=>{const prepared=await prepareInlineTaskNotices($,state,e);const result=await next(prepared?.event??e);await finishInlineTaskNotices($,state,prepared,result);return result;};
 hooks['turn.start']=async($,e,next)=>{await resumeDeferredTaskNotices($,state);return next(e);};
 const append=async(event={})=>{const e={door:'tool',origin:{kind:'tool'},message:{type:'user',role:'user',content:[{type:'tool_result',tool_use_id:'tool-exact',content:'ORIGINAL\n工具数据',is_error:true}]},...event};return hooks['session.append']($,e,async changed=>({uuid:rowId,...changed}));};
 return{$,state,data,posts,prompts,text,append,time:value=>time=value,hooks};
}
test('busy receipt is attached once at a tool boundary without changing original output or denial',async()=>{
 const f=fixture();await tickTaskNotices(f.$,f.state);assert.equal(f.prompts.length,0);assert.equal(f.posts.filter(p=>p.action==='CLAIM').length,0);
 const result=await f.append(),block=result.message.content[0];assert.ok(block.content.startsWith('ORIGINAL\n工具数据'));assert.equal(block.is_error,true);assert.equal(block.tool_use_id,'tool-exact');assert.ok(block.content.includes(f.text));
 await new Promise(resolve=>setImmediate(resolve));assert.equal(f.posts.filter(p=>p.action==='ACK').length,1);assert.equal(f.posts.find(p=>p.action==='ACK').messageId,rowId);assert.equal(f.prompts.length,0);
 assert.equal((await f.append()).message.content[0].content,'ORIGINAL\n工具数据');
});
test('child, human, unrelated plugin and binary results are untouched',async()=>{
 const f=fixture();await tickTaskNotices(f.$,f.state);
 for(const extra of [{agentId:'child'},{origin:{kind:'person'}},{origin:{kind:'plugin',name:'other'}},{message:{type:'user',role:'user',content:[{type:'tool_result',tool_use_id:'image',content:[{type:'image',data:'bytes'}]}]}}]){
  const result=await f.append(extra);assert.ok(!JSON.stringify(result).includes('Agbrio task notice: '));
 }
 assert.equal(f.posts.filter(p=>p.action==='CLAIM').length,0);
});
test('idle fallback uses the same durable notice and never submits asUser',async()=>{
 const f=fixture();await tickTaskNotices(f.$,f.state);f.state.resourceBusy=false;f.time(11000);await tickTaskNotices(f.$,f.state);
 assert.equal(f.prompts.length,1);assert.equal(f.prompts[0].text,f.text);assert.equal(f.prompts[0].asUser,undefined);assert.equal(f.posts.filter(p=>p.action==='CLAIM').length,1);
});
test('ambiguous inline append remains fenced after reload and directory changes',async()=>{
 const f=fixture();await tickTaskNotices(f.$,f.state);const next=async()=>{throw Error('native outcome unknown');};
 await assert.rejects(f.hooks['session.append'](f.$,{door:'tool',origin:{kind:'tool'},message:{type:'user',content:[{type:'tool_result',tool_use_id:'tool-exact',content:'original'}]}},next));
 f.state.identity.cwd='D:/workspace/subdir';f.time(12000);await tickTaskNotices(f.$,f.state);const result=await f.append();
 assert.ok(!result.message.content[0].content.includes('Agbrio task notice: '));assert.equal(f.prompts.length,0);assert.equal(f.posts.filter(p=>p.action==='CLAIM').length,1);
});
test('frame is unambiguously bound to the nonce, exact ID and native tool result',()=>{
 const r={id:notice,proofNonce:sid,text:'fixed'};assert.equal(inlineFrame(r,'tool-exact'),'\n\n[Agbrio task notice: '+JSON.stringify({noticeId:notice,proofNonce:sid,toolUseId:'tool-exact'})+']\nfixed\n[/Agbrio task notice]');
});
