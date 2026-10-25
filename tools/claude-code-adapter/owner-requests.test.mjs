import test from 'node:test';
import assert from 'node:assert/strict';
import {registerOwnerRequests} from '../claude-code-plugin/hooks/owner-requests.mjs';
test('classic owner question preserves exact native arguments and answer keys without submitting prompts',async()=>{
 const hooks=new Map(),posts=[],stored=new Map();const on=(name,...rest)=>{hooks.set(name,rest.at(-1));return {catch(){}};};
 const state={identity:{sessionId:'10000000-0000-4000-8000-000000000001',cwd:'D:/fixture'},instanceId:'20000000-0000-4000-8000-000000000001',origin:'http://127.0.0.1:12345',token:'fixture'};
 registerOwnerRequests(on,state);
 const $={store:{set:async(k,v)=>stored.set(k,v)},http:{fetch:async(url,init)=>{posts.push(JSON.parse(init.body));return {ok:true,text:JSON.stringify(url.endsWith('/poll')?{status:'ANSWER',answer:{action:'ANSWER',answers:{'Choose transport':'A'}}}:{captured:true})};}},prompt:{submit(){assert.fail('No prompt submission');}}};
 const event={tool:'AskUserQuestion',tool_use_id:'exact-tool',questions:[{question:'Choose transport',options:[{label:'A'},{label:'B'}]}]};
 const answer=await hooks.get('classic.PreToolUse')($,event,async()=>({}));
 assert.equal(answer.allow,true);assert.deepEqual(answer.updatedInput,{questions:event.questions,answers:{'Choose transport':'A'}});assert.equal(posts[0].toolUseId,'exact-tool');assert.equal(posts[0].sessionId,state.identity.sessionId);assert.equal(stored.get('agbrio-owner-receipts-v1').length,1);
 const before=posts.length;assert.deepEqual(await hooks.get('classic.PreToolUse')($,event,async()=>({deny:'Managed denial'})),{deny:'Managed denial'});assert.equal(posts.length,before);
});
