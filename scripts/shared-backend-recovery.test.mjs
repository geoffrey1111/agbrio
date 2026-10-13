import {test} from 'node:test';
import assert from 'node:assert/strict';
import {backendRecovery,verifiedBackendStart} from '../src-tauri/resources/codex-shared/backend-recovery.mjs';
const settle=()=>new Promise(r=>setImmediate(r));
test('simultaneous native exit/error/close produces only one queued restart',async()=>{
 let fire,scheduled=0,started=0;const recovery=backendRecovery(async()=>{started++;},()=>assert.fail(),{setTimer:fn=>{scheduled++;fire=fn;return 1;}});
 recovery.schedule(1000);recovery.schedule(1000);assert.equal(scheduled,1);fire();await settle();assert.equal(started,1);recovery.cancel();
});
test('changed or missing native cohort shuts down once without uncaught rejection/retry',async()=>{
 let fire,failed=0;const recovery=backendRecovery(async()=>{throw Error('SHARED_NATIVE_COHORT_MISMATCH');},async error=>{assert.equal(error.message,'SHARED_NATIVE_COHORT_MISMATCH');failed++;},{setTimer:fn=>{fire=fn;return 1;}});
 recovery.schedule(1000);fire();await settle();assert.equal(failed,1);recovery.schedule(1000);await settle();assert.equal(failed,1);
});
test('shutdown cancels the retry and a queued callback cannot revive the native writer',async()=>{
 let fire,cleared=0,started=0;const recovery=backendRecovery(async()=>{started++;},()=>assert.fail(),{setTimer:fn=>{fire=fn;return 1;},clearTimer:id=>{assert.equal(id,1);cleared++;}});
 recovery.schedule(1000);recovery.cancel();fire();await settle();assert.equal(cleared,1);assert.equal(started,0);
});

test('shutdown during pending port preparation prevents verification and spawning',async()=>{
 let closing=false,release;
 const pending=verifiedBackendStart(()=>closing,()=>new Promise(r=>release=r),()=>assert.fail('Must not verify after shutdown'),()=>assert.fail('Must not spawn after shutdown'));
 closing=true;release({});assert.equal(await pending,null);
});

test('shutdown during native integrity verification prevents the in-flight retry from spawning',async()=>{
 let closing=false,release,entered;
 const verifying=new Promise(r=>entered=r);
 const pending=verifiedBackendStart(()=>closing,async()=>({}),()=>{entered();return new Promise(r=>release=r);},()=>assert.fail('Must not spawn after shutdown'));
 await verifying;closing=true;release();assert.equal(await pending,null);
});

test('valid native preparation starts once; verification failure never launches a child',async()=>{
 let launches=0;const candidate={port:12345},child={pid:42};
 assert.equal(await verifiedBackendStart(()=>false,async()=>candidate,async value=>assert.equal(value,candidate),value=>{assert.equal(value,candidate);launches++;return child;}),child);
 await assert.rejects(verifiedBackendStart(()=>false,async()=>candidate,async()=>{throw Error('SHARED_NATIVE_COHORT_MISMATCH');},()=>assert.fail()),/SHARED_NATIVE_COHORT_MISMATCH/);
 assert.equal(launches,1);
});
