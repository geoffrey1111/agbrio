// Manual, disposable GitHub Mac fixture. No account, provider page or model call.
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp,mkdir,rm,writeFile,readFile} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import path from 'node:path';
import net from 'node:net';
import http from 'node:http';
import {pathToFileURL} from 'node:url';
const pause=ms=>new Promise(done=>setTimeout(done,ms));
async function until(action,check){for(let n=0;n<100;n++){const value=await action();if(check(value))return value;await pause(100);}throw Error('OWNED_BROWSER_SETTLEMENT_UNCONFIRMED');}
test('real Mac Chromium preserves native identity, human-only stop, owned close and unrelated process',{skip:process.platform!=='darwin'||process.env.AGBRIO_MAC_BROWSER_FIXTURE!=='1',timeout:180000},async t=>{
 const base=await mkdtemp(path.join(tmpdir(),'agbrio-browser-native-'));
 const resources=path.resolve('src-tauri/resources/browser-executor-lite');
 const{ChromiumRuntime}=await import(pathToFileURL(path.join(resources,'native-runtime.mjs')).href);
 const{macosSecurityPlatform:platform}=await import(pathToFileURL(path.join(resources,'security-browser-macos.mjs')).href);
 const runtime=new ChromiumRuntime({resourceDirectory:resources,dataDirectory:path.join(base,'primary')});
 const secondary=path.join(base,'protected'),protectedProfile=path.join(secondary,'browser-profile/chatgpt');let protectedOwner,protectedStarted=false;
 const server=http.createServer((_request,response)=>{response.setHeader('content-type','text/html');response.end('<!doctype html><title>Agbrio offline fixture</title><p>Local transport fixture only</p>');});
 await new Promise(done=>server.listen(0,'127.0.0.1',done));
 let settled=false;
 t.after(async()=>{
  try{
   await runtime.shutdown();
   const primary=await platform.inventory(runtime.profile).catch(()=>[]);
   for(const owner of primary){if(owner.automated||owner.ownershipUnproven)throw Error('PRIMARY_FIXTURE_OWNERSHIP_UNCONFIRMED');await platform.close(owner);}
   if(primary.length)await until(()=>platform.inventory(runtime.profile),items=>items.length===0);
   if(protectedOwner)await platform.close(protectedOwner);
   if(protectedStarted)await until(()=>platform.inventory(protectedProfile),items=>items.length===0);
   // Failed fixture evidence stays only in this ephemeral CI job until its VM
   // is discarded. Never upload a browser profile as an artifact.
   if(settled)await rm(base,{recursive:true,force:true,maxRetries:10,retryDelay:100});
  }finally{server.closeAllConnections();await new Promise(done=>server.close(done));}
  if(settled)console.log(JSON.stringify({status:'MACOS_NATIVE_BROWSER_E2E_PASS',providerInputSubmitted:false,modelsInvoked:0,humanOnlyCdp:false,exactNativeIdentity:true,sameProfileRestart:true,unrelatedProcessPreserved:true}));
 });
 await assert.rejects(runtime.connection(),/CHROMIUM_COMPONENT_NOT_INSTALLED/);
 // This explicit manual fixture is the only download in this test job.
 const binary=await runtime.verifiedBinary(true);
 const browser=await runtime.connection();const page=(await browser.pages())[0];
 const local=`http://127.0.0.1:${server.address().port}/`;
 await page.goto(local,{waitUntil:'domcontentloaded'});assert.equal(await page.title(),'Agbrio offline fixture');assert.equal(page.url(),local);
 const initial=(await platform.inventory(runtime.profile))[0];assert.equal(initial.pid,browser.process().pid);assert.equal(initial.automated,true);assert.equal(initial.ownershipUnproven,false);
 // The ordinary explicit-open path focuses the already-owned local page; its
 // URL is verified first, so its about:blank provider navigation is not reached.
 const focused=await runtime.openForAuthentication();assert.ok(['FOREGROUND','ATTENTION'].includes(focused.focus));assert.equal(page.url(),local);
 await mkdir(protectedProfile,{recursive:true,mode:0o700});await platform.ensureOwnedProfile(secondary,protectedProfile);
 const protectedLease={schema:'AIWR_BROWSER_SESSION_V1',mode:'HUMAN_ONLY',executorPid:process.pid,profile:protectedProfile};
 await writeFile(path.join(protectedProfile,'aiwr-browser-session.json'),JSON.stringify(protectedLease),{mode:0o600,flag:'wx'});
 const pid=await platform.launch(binary,[`--user-data-dir=${protectedProfile}`,'--disable-extensions','--no-first-run','--no-default-browser-check','--restore-last-session']);protectedStarted=true;
 const protectedItems=await until(()=>platform.inventory(protectedProfile),items=>items.length===1&&items[0].pid===pid&&items[0].visible);
 protectedOwner={...protectedItems[0],binary};await writeFile(path.join(protectedProfile,'aiwr-browser-session.json'),JSON.stringify({...protectedLease,...protectedOwner}),{mode:0o600});
 // AUTH_REQUIRED here is an offline test marker, never a real account challenge.
 await runtime.latchAuthRequired();await assert.rejects(runtime.connection(),/AUTH_REQUIRED/);
 const human=await runtime.openHumanSecurity();assert.equal(human.mode,'HUMAN_ONLY');assert.equal(await runtime.authRequired(),true);
 const humanOwner=(await platform.inventory(runtime.profile))[0];assert.equal(humanOwner.automated,false);assert.equal(humanOwner.ownershipUnproven,false);assert.notEqual(humanOwner.pid,initial.pid);
 const reused=await runtime.openHumanSecurity();assert.equal(reused.reused,true);assert.equal((await platform.inventory(runtime.profile))[0].pid,humanOwner.pid);
 const cdpClosed=await new Promise(done=>{const socket=net.createConnection({host:'127.0.0.1',port:9229});socket.once('connect',()=>{socket.destroy();done(false);});socket.once('error',()=>done(true));socket.setTimeout(1000,()=>{socket.destroy();done(false);});});assert.equal(cdpClosed,true,'HUMAN_ONLY must have no CDP listener');
 await runtime.ownerConfirmedAuthentication();assert.equal(await runtime.authRequired(),false);assert.deepEqual(await platform.inventory(runtime.profile),[]);
 const protectedAfter=(await platform.inventory(protectedProfile))[0];assert.equal(protectedAfter.pid,protectedOwner.pid);assert.equal(protectedAfter.started,protectedOwner.started);
 const restarted=new ChromiumRuntime({resourceDirectory:resources,dataDirectory:path.join(base,'primary')});
 await restarted.connection();assert.equal(restarted.profile,runtime.profile);await restarted.shutdown();
 assert.deepEqual(await platform.inventory(runtime.profile),[]);
 await platform.close(protectedOwner);await until(()=>platform.inventory(protectedProfile),items=>items.length===0);protectedOwner=null;protectedStarted=false;
 const marker=JSON.parse(await readFile(path.join(runtime.profile,'aiwr-profile-owner.json'),'utf8'));assert.ok(marker.ownerId);settled=true;

});
