import test from 'node:test';
import assert from 'node:assert/strict';
import {replaceStateFile} from '../src-tauri/resources/codex-shared/state-file.mjs';
test('atomic state survives transient Windows reader locks without truncating its target',async()=>{
 const events=[];let tries=0;
 await replaceStateFile('fixture/ready.json',{ready:true},{write:async(p,text)=>events.push(['write',p,JSON.parse(text)]),move:async(a,b)=>{events.push(['move',a,b]);if(tries++<3)throw Object.assign(Error('reader'),{code:'EPERM'});},pause:async()=>events.push(['wait'])});
 assert.equal(events.filter(e=>e[0]==='write').length,1);assert.equal(events.filter(e=>e[0]==='move').length,4);assert.equal(events.filter(e=>e[0]==='wait').length,3);assert.deepEqual(events[0],['write','fixture/ready.json.pending',{ready:true}]);
});
test('persistent lock is bounded and retains the staged snapshot for diagnosis',async()=>{
 let tries=0;await assert.rejects(replaceStateFile('fixture',{ready:false},{write:async()=>{},move:async()=>{tries++;throw Object.assign(Error('locked'),{code:'EBUSY'});},pause:async()=>{},attempts:3}),e=>e.code==='EBUSY');assert.equal(tries,3);
});
test('disk/full/path failures never retry or claim a committed state',async()=>{
 let attempts=0;await assert.rejects(replaceStateFile('fixture',{}, {write:async()=>{},move:async()=>{attempts++;throw Object.assign(Error('disk'),{code:'ENOSPC'});},pause:()=>assert.fail('No blind retry')}),e=>e.code==='ENOSPC');assert.equal(attempts,1);
});
