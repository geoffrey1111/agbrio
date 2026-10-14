// Real disposable Windows process tree. Never target production process names.
// One temporary Node copy; no Codex, model, shared setup or production settings.
import assert from 'node:assert/strict';
import {spawn,execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {mkdir,mkdtemp,copyFile,writeFile,readFile,readdir,lstat,unlink,rmdir} from 'node:fs/promises';
import {join,resolve,relative} from 'node:path';
import {randomUUID} from 'node:crypto';
import {lifecycle} from '../src-tauri/resources/codex-shared/control.mjs';
const exec=promisify(execFile),scratch=resolve('runtime/shared-native-compatibility');
assert.equal(process.platform,'win32');await mkdir(scratch,{recursive:true});
const directory=await mkdtemp(join(scratch,'process-cleanup-')),runtime=join(directory,'runtime');
const source=resolve('src-tauri/resources/codex-shared');
let root,rootClosed,stranger,strangerClosed,run,started=false,verified=false;
const recoverUnavailable=process.argv.includes('--recover-unavailable');
const instance=randomUUID();
async function clean(path){assert.ok(!relative(directory,path).startsWith('..'));for(const name of await readdir(path)){const p=join(path,name),s=await lstat(p);assert.equal(s.isSymbolicLink(),false);if(s.isDirectory())await clean(p);else await unlink(p);}await rmdir(path);}
try{
 await mkdir(runtime);const node=join(runtime,'node.exe');await copyFile(process.execPath,node);
 await copyFile(join(source,'peer-service.ps1'),join(runtime,'peer-service.ps1'));
 const leaf=join(directory,'leaf.mjs'),rootScript=join(directory,'root.mjs');
 await writeFile(leaf,`import {spawn} from 'node:child_process';
 if(process.argv[2]==='branch'){const c=spawn(process.execPath,[process.argv[1],'leaf'],{detached:true,windowsHide:true,stdio:'ignore'});c.unref();process.stdout.write(JSON.stringify({grandchild:c.pid})+'\\n');}
 setInterval(()=>{},1000);`);
 await writeFile(rootScript,`import {spawn} from 'node:child_process';import {createServer} from 'node:net';
 let healthy=false;const server=createServer(socket=>{socket.on('error',()=>{});if(healthy){const body=JSON.stringify({instance:'${instance}',activeTurns:1});socket.end('HTTP/1.1 200 OK\\r\\nContent-Type: application/json\\r\\nContent-Length: '+Buffer.byteLength(body)+'\\r\\nConnection: close\\r\\n\\r\\n'+body);}});await new Promise(yes=>server.listen(0,'127.0.0.1',yes));
 const c=spawn(process.execPath,[process.argv[2],'branch'],{detached:true,windowsHide:true,stdio:['ignore','pipe','ignore']});c.unref();
 c.stdout.once('data',data=>process.stdout.write(JSON.stringify({branch:c.pid,port:server.address().port,...JSON.parse(data)})+'\\n'));
 process.on('message',message=>{if(message==='EXIT_FIXTURE_ROOT')process.exit(0);else if(message==='HEALTHY_FIXTURE')healthy=true;else if(message==='UNREACHABLE_FIXTURE')healthy=false;});setInterval(()=>{},1000);`);
 stranger=spawn(node,[leaf,'leaf'],{windowsHide:true,stdio:'ignore'});strangerClosed=new Promise(yes=>stranger.once('close',yes));
 root=spawn(node,[rootScript,leaf],{detached:true,windowsHide:true,stdio:['ignore','pipe','ignore','ipc']});rootClosed=new Promise(yes=>root.once('close',yes));started=true;
 const tree=await new Promise((yes,no)=>{const timer=setTimeout(()=>no(Error('FIXTURE_TREE_START_TIMEOUT')),8000);root.once('error',no);root.stdout.once('data',data=>{clearTimeout(timer);yes(JSON.parse(data));});});
 assert.ok(Number.isInteger(tree.branch)&&Number.isInteger(tree.grandchild));
 const helper=join(directory,'verify-stop.ps1'),capture=join(directory,'snapshot.json');
 const quoted=source.replaceAll("'","''");
 const recovery=resolve('src-tauri/resources/codex-shared/shared-offline-recovery.ps1').replaceAll("'","''");
 await writeFile(helper,`param([string]$Directory,[uint32]$RootPid,[uint32]$StrangerPid,[string]$Action)
$ErrorActionPreference='Stop'
. '${quoted}/owned-processes.ps1'
$owner=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$recoverUnavailable=$${recoverUnavailable?'true':'false'}
if($recoverUnavailable){. '${recovery}'}
$path=Join-Path $Directory 'snapshot.json'
if($Action -eq 'Capture'){
 $saved=Get-SharedProcessIdentity $RootPid $Directory
 $other=Get-SharedProcessIdentity $StrangerPid $Directory
 $snapshot=Get-SharedStopSnapshot $saved $Directory $owner
 if($recoverUnavailable){
  [IO.File]::WriteAllText((Join-Path $Directory 'runtime-config.json'),(@{format=2;instance='${instance}';ownerSid=$owner;port=${tree.port}}|ConvertTo-Json))
  [IO.File]::WriteAllText((Join-Path $Directory 'ready.json'),(@{instance='${instance}';supervisorIdentity=$saved}|ConvertTo-Json -Depth 5))
 }
 [IO.File]::WriteAllText($path,(@{snapshot=$snapshot;other=$other}|ConvertTo-Json -Depth 9))
 @{children=@($snapshot.children|ForEach-Object {$_.pid});strangerIncluded=(@($snapshot.children|Where-Object {$_.pid -eq $StrangerPid}).Count -gt 0)}|ConvertTo-Json -Compress
}elseif($Action -eq 'HealthyGuard'){
 $rejected=$false
 try{Get-AgbrioUnavailableSnapshot $Directory '${quoted}'|Out-Null}catch{if($_.Exception.Message -ceq 'SHARED_RECOVERY_USE_NORMAL_STOP'){$rejected=$true}else{throw}}
 if(!$rejected -or (Test-Path -LiteralPath (Join-Path $Directory 'stop-processes.json'))){throw 'FIXTURE_HEALTHY_GUARD_FAILED'}
 @{responsiveBusyRefused=$true}|ConvertTo-Json -Compress
}else{
 $record=Get-Content -LiteralPath $path -Raw|ConvertFrom-Json
 try{
  $orphanCount=@($record.snapshot.children|Where-Object {Test-SharedProcessIdentity $_ (Get-SharedProcessIdentity ([uint32]$_.pid) $Directory)}).Count
  $rootAbsent=!(Test-SharedProcessIdentity $record.snapshot.root (Get-SharedProcessIdentity $RootPid $Directory))
  if($recoverUnavailable){
   $preview=Get-AgbrioUnavailableSnapshot $Directory '${quoted}'
   $noConsentRejected=$false
   try{Stop-AgbrioUnavailableOwner $Directory '${quoted}' $preview|Out-Null}catch{if($_.Exception.Message -ceq 'SHARED_RECOVERY_OWNER_CONFIRMATION_REQUIRED'){$noConsentRejected=$true}else{throw}}
   if(!$noConsentRejected -or !(Test-SharedProcessIdentity $record.snapshot.root (Get-SharedProcessIdentity $RootPid $Directory))){throw 'FIXTURE_NO_CONSENT_FAILED'}
   $changed=@{instance='different';snapshot=$preview.snapshot};$changedRejected=$false
   try{Stop-AgbrioUnavailableOwner $Directory '${quoted}' $changed -OwnerConfirmed|Out-Null}catch{if($_.Exception.Message -ceq 'SHARED_RECOVERY_IDENTITY_CHANGED'){$changedRejected=$true}else{throw}}
   if(!$changedRejected -or !(Test-SharedProcessIdentity $record.snapshot.root (Get-SharedProcessIdentity $RootPid $Directory))){throw 'FIXTURE_CHANGED_IDENTITY_FAILED'}
   $result=Stop-AgbrioUnavailableOwner $Directory '${quoted}' $preview -OwnerConfirmed
  }else{$result=Invoke-SharedStopSnapshot $record.snapshot $record.snapshot.root $Directory $owner}
  $otherAlive=Test-SharedProcessIdentity $record.other (Get-SharedProcessIdentity $StrangerPid $Directory)
  @{rootAbsent=$rootAbsent;orphansBefore=$orphanCount;stopped=$result.stopped;unrelatedPreserved=$otherAlive}|ConvertTo-Json -Compress
 }finally{
  Invoke-SharedStopSnapshot $record.snapshot $record.snapshot.root $Directory $owner|Out-Null
  Invoke-SharedStopSnapshot @{root=$record.other;children=@()} $record.other $Directory $owner|Out-Null
 }
}
`);
 const ps=join(process.env.WINDIR,'System32/WindowsPowerShell/v1.0/powershell.exe');
 run=async action=>JSON.parse((await exec(ps,['-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',helper,'-Directory',directory,'-RootPid',String(root.pid),'-StrangerPid',String(stranger.pid),'-Action',action],{windowsHide:true,timeout:45000,maxBuffer:16384})).stdout);
 const captured=await run('Capture');assert.deepEqual(captured.children.sort((a,b)=>a-b),[tree.branch,tree.grandchild].sort((a,b)=>a-b));assert.equal(captured.strangerIncluded,false);
 if(process.argv.includes('--fail-after-capture'))throw Error('FIXTURE_INJECTED_FAILURE');
 if(recoverUnavailable){
  await assert.rejects(lifecycle({directory,scripts:source,action:'disable'}),/SHARED_CONNECTION_UNCERTAIN/);
  root.send('HEALTHY_FIXTURE');await new Promise(yes=>setTimeout(yes,50));assert.deepEqual(await run('HealthyGuard'),{responsiveBusyRefused:true});root.send('UNREACHABLE_FIXTURE');await new Promise(yes=>setTimeout(yes,50));
 }
 if(!recoverUnavailable){root.send('EXIT_FIXTURE_ROOT');assert.equal(await rootClosed,0);}
 const stopped=await run('Stop');assert.deepEqual(stopped,{rootAbsent:!recoverUnavailable,orphansBefore:2,stopped:true,unrelatedPreserved:true});
 if(recoverUnavailable){
  await rootClosed;
  await lifecycle({directory,scripts:source,action:'disable'});
  assert.equal(JSON.parse(await readFile(join(directory,'runtime-config.json'),'utf8')).phase,'DISABLED');
  assert.equal(await lstat(join(directory,'stop-processes.json')).catch(error=>{if(error.code==='ENOENT')return null;throw error;}),null);
 }
 await strangerClosed;verified=true;
 console.log(JSON.stringify({realWindowsOrphanCleanup:recoverUnavailable?'NOT_RUN':'PASS',...recoverUnavailable?{unresponsiveOwnerRecovery:'PASS',normalDisableBeforeRecovery:'REFUSED',normalDisableAfterRecovery:'PASS',explicitOwnerConfirmationRequired:true,changedIdentityRejected:true,responsiveBusyRefused:true}:{},capturedChildren:2,orphanChildrenStopped:2,unrelatedSameBinaryPreserved:true,desktopStarts:0,codexProcessesStarted:0,modelRequests:0,temporaryNodeBytes:(await lstat(node)).size}));
}finally{
 // Keep evidence on an uncertain failed run; never fall back to process-name kill.
 if(!started||verified)await clean(directory);
 else{
  let recovered=false;
  try{if(run){await readFile(join(directory,'snapshot.json'));await run('Stop');await Promise.all([rootClosed,strangerClosed]);await clean(directory);recovered=true;}}catch{}
  if(recovered)process.stderr.write('FIXTURE_FAILURE_CLEANUP_VERIFIED\n');
  else process.stderr.write('FIXTURE_CLEANUP_REQUIRES_REVIEW\n');
 }
}
