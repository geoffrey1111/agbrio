// Full native preparation plus a repeated tool verifier while the gateway lives.
// Disposable empty home/ephemeral port; no Desktop, real chat, account or model.
import assert from 'node:assert/strict';
import {execFile,spawn} from 'node:child_process';
import {promisify} from 'node:util';
import {createServer} from 'node:net';
import {mkdtemp,mkdir,writeFile,readFile,readdir,lstat,unlink,rmdir} from 'node:fs/promises';
import {resolve,join,relative} from 'node:path';
import {createHash} from 'node:crypto';
import {lifecycle} from '../src-tauri/resources/codex-shared/control.mjs';
import {stageSharedResources} from '../src-tauri/resources/codex-shared/launch-entry.mjs';
const exec=promisify(execFile),resourceIndex=process.argv.indexOf('--resource-source'),source=resolve(resourceIndex<0?'src-tauri/resources/codex-shared':process.argv[resourceIndex+1]);
assert.ok(process.argv[2],'Pass exact native binary');const binary=resolve(process.argv[2]),node=resolve(process.argv[3]??process.execPath);assert.equal(process.platform,'win32');
const ps=join(process.env.WINDIR,'System32/WindowsPowerShell/v1.0/powershell.exe');
const protectedPaths=['settings.json','runtime-config.json'].map(n=>join(process.env.LOCALAPPDATA,'AIWorkRouter/codex-shared',n));
const snapshot=()=>Promise.all(protectedPaths.map(async p=>createHash('sha256').update(await readFile(p)).digest('hex')));
const before=await snapshot(),scratch=resolve('runtime/shared-native-compatibility');await mkdir(scratch,{recursive:true});
const existingConfig=process.argv.includes('--existing-config');
const holdReady=process.argv.includes('--hold-ready');
const leavePrepared=process.argv.includes('--leave-prepared');
const expectLockedFailure=process.argv.includes('--expect-locked-failure');assert.ok(!expectLockedFailure||holdReady);
const directory=await mkdtemp(join(scratch,'cold-prepare-')),home=join(directory,'home');
let child,closed,finished=false,handedOff=false;const codes=[];
let locker,lockerClosed,releaseTimer,lockPoll;
const server=createServer();await new Promise(yes=>server.listen(0,'127.0.0.1',yes));const port=server.address().port;await new Promise(yes=>server.close(yes));
const inspected=JSON.parse((await exec(ps,['-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',join(source,'native-runtime.ps1'),'-Action','Inspect','-Binary',binary],{windowsHide:true,timeout:60000})).stdout);
const options={directory,scripts:source,binary,version:inspected.version,desktop:'ISOLATED_NO_DESKTOP'};
let prepared=false;
const deps={config:{port,...existingConfig?{}:{codexHome:home}},
 exec:async(file,args,opts)=>args.some(a=>a.endsWith('desktop-state.ps1'))?{stdout:JSON.stringify({desktop:'ISOLATED_NO_DESKTOP',count:prepared?1:0})}:exec(file,args,opts),
 spawn:(file,args,opts)=>{
  assert.ok(args.length,'Never start a Desktop');child=spawn(file,args,{...opts,stdio:leavePrepared?'ignore':['ignore','ignore','pipe']});child.stderr?.on('data',bytes=>codes.push(...(bytes.toString().match(/SHARED_[A-Z_]+/g)??[])));closed=new Promise(yes=>child.once('close',yes));
  if(holdReady)lockPoll=setInterval(async()=>{const value=await readFile(join(directory,'ready.json.pending'),'utf8').then(JSON.parse).catch(()=>null);if(value?.supervisorIdentity?.pid===child.pid&&!releaseTimer){clearInterval(lockPoll);releaseTimer=setTimeout(()=>locker.stdin.end('\n'),700);}},25);
  return child;
 }
};
async function clean(path){assert.ok(!relative(directory,path).startsWith('..'));for(const name of await readdir(path)){const p=join(path,name);assert.equal((await lstat(p)).isSymbolicLink(),false);if((await lstat(p)).isDirectory())await clean(p);else await unlink(p);}await rmdir(path);}
try{
 await mkdir(home);
 await stageSharedResources({directory,source,node});
 if(holdReady){
  const path=join(directory,'ready.json');await writeFile(path,'{"ready":false}');
  const code=`$stream=[IO.File]::Open('${path.replaceAll("'","''")}',[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read);try{[Console]::WriteLine('LOCKED');$null=[Console]::ReadLine()}finally{$stream.Dispose()}`;
  locker=spawn(ps,['-NoProfile','-NonInteractive','-EncodedCommand',Buffer.from(code,'utf16le').toString('base64')],{windowsHide:true,stdio:['pipe','pipe','ignore']});lockerClosed=new Promise(yes=>locker.once('close',yes));
  await new Promise((yes,no)=>{const timer=setTimeout(()=>no(Error('LOCK_FIXTURE_NOT_READY')),8000);locker.stdout.once('data',data=>{clearTimeout(timer);assert.equal(data.toString().trim(),'LOCKED');yes();});});
 }
 if(expectLockedFailure){
  await assert.rejects(lifecycle({...options,action:'setup'},deps));
  let timer;await Promise.race([closed,new Promise((_,no)=>{timer=setTimeout(()=>no(Error('OLD_LOCK_FAILURE_DID_NOT_EXIT')),6000)})]);clearTimeout(timer);
  assert.ok(codes.includes('SHARED_RUNTIME_FAILED'),'The old producer must expose its failed state replacement as a runtime failure');
  assert.deepEqual(await snapshot(),before);finished=true;
  console.log(JSON.stringify({legacyLockedStateProducerExit:'REPRODUCED',codes,desktopStarts:0,threadsCreated:0,modelRequests:0,productionSettingsUnchanged:true}));
 }else{
 const ready=await lifecycle({...options,action:'setup'},deps);assert.equal(ready.ready,true);prepared=true;
 if(leavePrepared){handedOff=true;console.log(JSON.stringify({preparedDirectory:directory,port}));}
 else{
 await assert.rejects(lifecycle({...options,action:'launch'},deps),/SHARED_DESKTOP_CLOSE_REQUIRED/);
 const after=await lifecycle({...options,action:'status'},deps);assert.equal(after.ready,true,'Gateway must survive the repeated tool verifier');
 await lifecycle({...options,action:'disable'},deps);let timer;await Promise.race([closed,new Promise((_,no)=>{timer=setTimeout(()=>no(Error('OWNED_RUNTIME_DID_NOT_EXIT')),6000)})]);clearTimeout(timer);
 assert.deepEqual(await snapshot(),before);finished=true;
 console.log(JSON.stringify({coldPreparation:'PASS',repeatedVerifierWhileGatewayAlive:'PASS',existingReadyReaderLock:holdReady?'RECOVERED':'NOT_TESTED',existingConfigurationRead:existingConfig,desktopStarts:0,threadsCreated:0,modelRequests:0,productionSettingsUnchanged:true,ownedRuntimeStopped:true,codes}));
 }
 }
}finally{
 clearInterval(lockPoll);clearTimeout(releaseTimer);if(locker){if(!locker.stdin.destroyed)locker.stdin.end('\n');await lockerClosed;}
 if(!handedOff){if(child?.exitCode===null)try{await lifecycle({...options,action:'disable'},deps);await closed;}catch{finished=false;}
 if(finished||!child)await clean(directory);else process.stderr.write('OWNED_COLD_PREPARATION_REQUIRES_REVIEW\n');}
}
