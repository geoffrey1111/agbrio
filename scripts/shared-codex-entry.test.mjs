import {test} from 'node:test';
import assert from 'node:assert/strict';
import {launchEntry as nativeEntry} from '../src-tauri/resources/codex-shared/launch-entry.mjs';
const launchEntry=(o,deps)=>nativeEntry(o,{desktopState:async()=>({desktop:'fixture-desktop',count:0}),...deps});
const options={directory:'fixture',source:'fixture-resources',node:'fixture-node'};
test('installed entry reuses a ready backend without preparation or resource replacement',async()=>{
 const actions=[];await launchEntry(options,{control:async o=>{actions.push(o.action);return {ready:true};},discover:()=>{throw Error('Must not rediscover a live owner');},stage:()=>{throw Error('Must not replace live assets');}});
 assert.deepEqual(actions,['status','launch']);
});
test('entry rejects an incomplete update before disabling existing state',async()=>{
 const actions=[];await assert.rejects(launchEntry(options,{control:async o=>{actions.push(o.action);return {ready:false};},discover:()=>{throw Error('SHARED_NATIVE_RUNTIME_INCOMPLETE');}}),/SHARED_NATIVE_RUNTIME_INCOMPLETE/);
 assert.deepEqual(actions,['status','prepare-check']);
});
test('cold installed entry resolves current package and prepares before explicit launch',async()=>{
 const actions=[];await launchEntry(options,{control:async o=>{actions.push(o.action);if(o.action==='setup')assert.equal(o.binary,'complete-new-cohort');return {ready:false};},discover:async()=>({binary:'complete-new-cohort',version:'new'}),stage:async()=>{actions.push('stage');}});
 assert.deepEqual(actions,['status','prepare-check','disable','stage','setup','launch']);
});
test('uncertain live state prevents entry from staging files or starting another backend',async()=>{
 const actions=[];await assert.rejects(launchEntry(options,{control:async o=>{actions.push(o.action);if(o.action==='prepare-check')throw Error('SHARED_CONNECTION_UNCERTAIN');return {ready:false};},discover:()=>assert.fail('Uncertain owner must prevent native retention'),stage:()=>assert.fail('Unexpected stage')}),/SHARED_CONNECTION_UNCERTAIN/);
 assert.deepEqual(actions,['status','prepare-check']);
});
test('idle native upgrade is discovered and verified before retiring the old backend',async()=>{
 const actions=[];await launchEntry(options,{control:async o=>{actions.push(o.action);return {ready:true,desktopConnected:false,activeTurns:0};},discover:async()=>{actions.push('verify-new');return {binary:'new',version:'new'};},saved:async()=>({binary:'old',version:'old'}),stage:async()=>{actions.push('stage');}});
 assert.deepEqual(actions,['status','prepare-check','verify-new','disable','stage','setup','launch']);
});
test('active native work defers discovery, update and stop even without an open Desktop',async()=>{
 const actions=[];await launchEntry(options,{control:async o=>{actions.push(o.action);return {ready:true,desktopConnected:false,activeTurns:2};},discover:()=>{throw Error('Active backend must not update');}});
 assert.deepEqual(actions,['status','launch']);
});

test('idle entry replaces changed resources even when the native cohort has not changed',async()=>{
 const actions=[];await launchEntry(options,{control:async o=>{actions.push(o.action);return {ready:true,desktopConnected:false,activeTurns:0};},discover:async()=>({binary:'same',version:'same'}),saved:async()=>({binary:'same',version:'same'}),resourcesMatch:async()=>false,stage:async()=>actions.push('stage')});
 assert.deepEqual(actions,['status','prepare-check','disable','stage','setup','launch']);
});

test('idle entry reuses only matching native and actual shared resources',async()=>{
 const actions=[];await launchEntry(options,{control:async o=>{actions.push(o.action);return {ready:true,desktopConnected:false,activeTurns:0};},discover:async()=>({binary:'same',version:'same'}),saved:async()=>({binary:'same',version:'same'}),resourcesMatch:async()=>true,stage:()=>assert.fail('Matching resources must remain in place')});
 assert.deepEqual(actions,['status','prepare-check','launch']);
});

test('ordinary Desktop blocks the cold entry before copying or starting shared resources',async()=>{
 const actions=[];await assert.rejects(launchEntry(options,{control:async o=>{actions.push(o.action);return {ready:false};},desktopState:async()=>({count:3}),discover:()=>assert.fail('Must not pin native files'),stage:()=>assert.fail('Must not stage resources')}),/SHARED_DESKTOP_CLOSE_REQUIRED/);
 assert.deepEqual(actions,['status']);
});

test('unresolved Desktop identity fails closed before preparing a writer',async()=>{
 const actions=[];await assert.rejects(launchEntry(options,{control:async o=>{actions.push(o.action);return {ready:false};},desktopState:async()=>{throw Error('SHARED_DESKTOP_BINARY_UNRESOLVED');},discover:()=>assert.fail('Must not discover/pin')}),/SHARED_DESKTOP_BINARY_UNRESOLVED/);
 assert.deepEqual(actions,['status']);
});

test('failed cold activation retires only the entry-owned preparation',async()=>{
 const actions=[];await assert.rejects(launchEntry(options,{control:async o=>{actions.push(o.action);if(o.action==='launch')throw Error('SHARED_DESKTOP_CONNECTION_FAILED');return {ready:false};},discover:async()=>({binary:'new',version:'new'}),stage:async()=>actions.push('stage')}),/SHARED_DESKTOP_CONNECTION_FAILED/);
 assert.deepEqual(actions,['status','prepare-check','disable','stage','setup','launch','disable']);
});

test('reused resident launch failure never disables its existing writer',async()=>{
 const actions=[];await assert.rejects(launchEntry(options,{control:async o=>{actions.push(o.action);if(o.action==='launch')throw Error('SHARED_DESKTOP_CONNECTION_FAILED');return {ready:true,desktopConnected:true};}}),/SHARED_DESKTOP_CONNECTION_FAILED/);
 assert.deepEqual(actions,['status','launch']);
});

test('failed activation cleanup reports incomplete recovery instead of hiding it',async()=>{
 let prepared=false;await assert.rejects(launchEntry(options,{control:async o=>{if(o.action==='setup')prepared=true;if(o.action==='launch')throw Error('SHARED_DESKTOP_CONNECTION_FAILED');if(o.action==='disable'&&prepared)throw Error('SHARED_DESKTOP_BUSY');return {ready:false};},discover:async()=>({binary:'new',version:'new'}),stage:async()=>{}}),/SHARED_ROLLBACK_INCOMPLETE/);
});
