// Isolated real gateway -> native command/exec -> official code-mode host.
// No production settings, Desktop or model calls; optional empty-home ephemeral thread.
import assert from 'node:assert/strict';
import {spawn,execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {createRequire} from 'node:module';
import {createServer} from 'node:net';
import {createServer as createHttpServer} from 'node:http';
import {gunzipSync} from 'node:zlib';
import {mkdir,mkdtemp,copyFile,symlink,readFile,writeFile,readdir,lstat,unlink,rmdir,realpath} from 'node:fs/promises';
import {dirname,join,resolve,relative} from 'node:path';
import {randomUUID,randomBytes,createHash} from 'node:crypto';
import {lifecycle} from '../src-tauri/resources/codex-shared/control.mjs';
const exec=promisify(execFile),source=resolve('src-tauri/resources/codex-shared');
const ps=join(process.env.WINDIR,'System32/WindowsPowerShell/v1.0/powershell.exe');
assert.equal(process.platform,'win32');assert.ok(process.argv[2],'Pass an exact native binary');const binary=resolve(process.argv[2]);
const stopWithCommand=process.argv.includes('--stop-with-native-command');
const observeAutoHost=process.argv.includes('--observe-auto-host');
const executeAutoHost=process.argv.includes('--execute-auto-host');
const isolatedDesktop=process.argv.includes('--isolated-desktop');
assert.ok(!isolatedDesktop||stopWithCommand,'Isolated Desktop requires the exact shutdown gate');
assert.ok(!observeAutoHost||stopWithCommand,'Auto-host observation requires the exact descendant shutdown gate');
assert.ok(!executeAutoHost||stopWithCommand,'Automatic execution requires the exact descendant shutdown gate');
assert.ok(!stopWithCommand||!process.argv.includes('--restart-backend'),'Run crash recovery and in-flight shutdown as separate bounded gates');
const protectedPaths=['settings.json','runtime-config.json'].map(name=>join(process.env.LOCALAPPDATA,'AIWorkRouter/codex-shared',name));
const snapshot=()=>Promise.all(protectedPaths.map(async path=>{try{return createHash('sha256').update(await readFile(path)).digest('hex');}catch(e){if(e.code==='ENOENT')return null;throw e;}}));
const protectedBefore=await snapshot();
const inspected=JSON.parse((await exec(ps,['-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',join(source,'native-runtime.ps1'),'-Action','Inspect','-Binary',binary],{windowsHide:true,timeout:60000,maxBuffer:262144})).stdout);
assert.equal(inspected.cohortVerified,true);
const ownerSid=(await exec(ps,['-NoProfile','-NonInteractive','-Command','[Security.Principal.WindowsIdentity]::GetCurrent().User.Value'],{windowsHide:true})).stdout.trim();
const require=createRequire(import.meta.url),wsRoot=dirname(require.resolve('ws/package.json',{paths:[resolve('tools/isolated-browser-executor')]})),{WebSocket}=require(wsRoot);
const scratch=resolve('runtime/shared-native-compatibility');await mkdir(scratch,{recursive:true});
const root=await mkdtemp(join(scratch,'gateway-code-mode-')),runtime=join(root,'runtime'),home=join(root,'home');
const files=['runtime.mjs','native-cohort.mjs','backend-recovery.mjs','peer-service.ps1'];
let child,closed,ws,health,config,origin,cleanable=true,heldCommand=null,capturedChildren=[],shutdownVerified=false,automaticHostCount=null;const pending=new Map();let id=0,cells=0,restarts=0;
let mockServer,mockRequests=0,automaticCells=0,automaticHostIdentity=false,automaticHostPid=null,fixtureTurn=null;
let desktopChild=null,desktopAttached=false,desktopRetired=false,desktopStopScript=null;
const pause=ms=>new Promise(r=>setTimeout(r,ms));
async function clean(path){
 const inside=relative(root,path);assert.ok(!inside.startsWith('..')&&!inside.includes(':'));
 for(const item of await readdir(path,{withFileTypes:true})){const p=join(path,item.name),stat=await lstat(p);assert.equal(stat.isSymbolicLink(),false);if(stat.isDirectory())await clean(p);else await unlink(p);}await rmdir(path);
}
async function stopFixture(){
 if(!stopWithCommand)return fetch(origin+'/stop',{method:'POST',headers:{Authorization:'Bearer '+config.controlToken},signal:AbortSignal.timeout(6000)});
 await lifecycle({directory:root,scripts:runtime,action:'disable'},{run:async args=>{
  const value=JSON.parse((await exec(ps,['-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',join(runtime,'environment.ps1'),...args],{windowsHide:true,timeout:30000,maxBuffer:262144})).stdout);
  if(args[args.indexOf('-Action')+1]==='CaptureStop'&&heldCommand&&!value.rootAbsent){
   assert.equal(value.root.pid,child.pid);
   for(const pid of [health.backendPid,heldCommand.helperPid,heldCommand.hostPid])assert.ok(value.children.some(v=>v.pid===pid),'Capture the actual native command descendants before stopping');
   capturedChildren=value.children;
  }
  return value;
 }});
 shutdownVerified=true;return {status:200};
}
try{
 await mkdir(runtime);await mkdir(home);
 if(isolatedDesktop&&!executeAutoHost)await writeFile(join(home,'config.toml'),'cli_auth_credentials_store = "file"\n');
 if(executeAutoHost){
  const nonce=randomUUID();
  const done=n=>({type:'response.completed',response:{id:'fixture-'+n,usage:{input_tokens:0,output_tokens:0,total_tokens:0}}});
  const call=(n,code)=>({type:'response.output_item.done',item:{type:'custom_tool_call',call_id:'fixture-call-'+n,name:'exec',input:code}});
  mockServer=createHttpServer(async(req,res)=>{
   try{
    assert.equal(req.socket.remoteAddress,'127.0.0.1');assert.equal(req.url,'/v1/responses');assert.equal(req.method,'POST');
    assert.equal(req.headers.authorization,undefined,'The isolated provider must not receive credentials');
    const parts=[];let size=0;for await(const part of req){size+=part.length;assert.ok(size<1024*1024);parts.push(part);}
    const raw=Buffer.concat(parts),body=JSON.parse((req.headers['content-encoding']==='gzip'?gunzipSync(raw):raw).toString());
    const n=++mockRequests;assert.ok(n<=3,'Bounded local mock responses only');
    if(n===1)assert.ok(JSON.stringify(body.tools).includes('"exec"'),'Native catalog exposes Code Mode');
    if(n>1){const output=body.input.find(item=>item.type==='custom_tool_call_output'&&item.call_id==='fixture-call-'+(n-1));assert.ok(JSON.stringify(output).includes(nonce),'The native host actually returned the fixture nonce');automaticCells++;}
    const events=n===1?[call(1,`store('fixture',${JSON.stringify(nonce)});text(${JSON.stringify(nonce)});`),done(n)]:n===2?[call(2,`text(load('fixture'));`),done(n)]:[{type:'response.output_item.done',item:{type:'message',role:'assistant',id:'fixture-final',content:[{type:'output_text',text:'ISOLATED_NATIVE_AUTO_HOST_DONE'}]}},done(n)];
    res.writeHead(200,{'content-type':'text/event-stream'});res.end(events.map(event=>'data: '+JSON.stringify(event)+'\n\n').join(''));
   }catch(error){res.writeHead(500);res.end('ISOLATED_MOCK_REJECTED');mockServer.fixtureError=error;}
  });
  await new Promise((yes,no)=>{mockServer.once('error',no);mockServer.listen(0,'127.0.0.1',yes);});
  await writeFile(join(home,'config.toml'),`model = "mock-model"\nmodel_provider = "agbrio_fixture"\ncli_auth_credentials_store = "file"\napproval_policy = "never"\nsandbox_mode = "read-only"\n[features]\ncode_mode_only = true\n[features.code_mode]\nenabled = true\n[model_providers.agbrio_fixture]\nname = "Loopback fixture only"\nbase_url = "http://127.0.0.1:${mockServer.address().port}/v1"\nwire_api = "responses"\nrequires_openai_auth = false\nrequest_max_retries = 0\nstream_max_retries = 0\n`);
 }
 for(const name of files)await copyFile(join(source,name),join(runtime,name));
 if(stopWithCommand){for(const name of ['environment.ps1','owned-processes.ps1'])await copyFile(join(source,name),join(runtime,name));await copyFile(process.execPath,join(runtime,'node.exe'));}
 await symlink(wsRoot,join(runtime,'ws'),'junction');
 const reservation=createServer();await new Promise((yes,no)=>{reservation.once('error',no);reservation.listen(0,'127.0.0.1',yes);});
 const port=reservation.address().port;await new Promise(yes=>reservation.close(yes));
 config={format:2,binary,version:inspected.version,nativeFiles:inspected.files,ownerSid,port,instance:randomUUID(),controlToken:randomBytes(32).toString('hex'),codexHome:home};
 const cfg=join(root,'runtime-config.json');await writeFile(cfg,JSON.stringify(config));
 if(stopWithCommand)await writeFile(join(root,'settings.json'),JSON.stringify({format:2,enabled:true,phase:'PREPARED',binary,version:inspected.version}));
 const env={...process.env,CODEX_HOME:home};for(const key of Object.keys(env))if(/API_KEY|TOKEN|SECRET|PROXY|OTEL|CODEX_APP_SERVER_WS_URL/i.test(key))delete env[key];
 child=spawn(stopWithCommand?join(runtime,'node.exe'):process.execPath,[join(runtime,'runtime.mjs'),cfg],{env,windowsHide:true,stdio:['ignore','ignore','pipe']});
 child.stderr.resume();closed=new Promise((yes,no)=>{child.once('error',no);child.once('close',yes);});
 origin=`http://127.0.0.1:${port}`;
 const waitReady=async excluded=>{let matched=null;for(let n=0;n<100;n++){if(child.exitCode!==null)throw Error('ISOLATED_GATEWAY_EXITED');try{const value=await(await fetch(origin+'/health',{signal:AbortSignal.timeout(500)})).json();if(value.ready&&value.instance===config.instance&&value.backendPid!==excluded){matched=value;break;}}catch{}await pause(200);}assert.ok(matched,'Isolated native readiness');health=matched;assert.equal(health.desktopConnected,false);assert.equal(health.activeTurns,0);};
 await waitReady();
 const runCells=async()=>{
 ws=new WebSocket(`ws://127.0.0.1:${port}/rpc`,{handshakeTimeout:10000,maxPayload:1024*1024});
 ws.on('error',()=>{});ws.on('message',data=>{const v=JSON.parse(data);const p=pending.get(v.id);if(p){pending.delete(v.id);clearTimeout(p.timer);v.error?p.reject(Error('ISOLATED_RPC_'+v.error.code+': '+v.error.message)):p.resolve(v.result);}else if(v.id!==undefined)ws.send(JSON.stringify({id:v.id,error:{code:-32601,message:'Validation permits no approvals'}}));});
 await new Promise((yes,no)=>{ws.once('open',yes);ws.once('error',no);});
 const request=(method,params)=>{assert.ok(['initialize','command/exec',...executeAutoHost?['thread/start','turn/start','turn/interrupt']:[]].includes(method));const current=++id;return new Promise((yes,no)=>{const timer=setTimeout(()=>{pending.delete(current);no(Error('ISOLATED_RPC_TIMEOUT'));},20000);pending.set(current,{resolve:yes,reject:no,timer});ws.send(JSON.stringify({id:current,method,params}));});};
 await request('initialize',{clientInfo:{name:'agbrio_isolated_code_mode',version:'0.1'},capabilities:{experimentalApi:true}});
 ws.send(JSON.stringify({method:'initialized',params:{}}));
 if(executeAutoHost){
  const notifications=[];const listener=data=>{const value=JSON.parse(data);if(value.method)notifications.push(value);};ws.on('message',listener);
  try{
   const started=await request('thread/start',{cwd:home,ephemeral:true,config:{'features.code_mode_host':{enabled:true,disable_in_process_fallback:true}},model:'mock-model',modelProvider:'agbrio_fixture',approvalPolicy:'never',sandbox:'read-only',environments:[],baseInstructions:'Isolated protocol fixture. No external tools, files, accounts or delegation.'});
   fixtureTurn={threadId:started.thread.id};
   const turn=await request('turn/start',{threadId:fixtureTurn.threadId,input:[{type:'text',text:'Local mocked Code Mode fixture only.',textElements:[]}]});fixtureTurn.turnId=turn.turn.id;
   for(let n=0;n<150;n++){if(notifications.some(v=>v.method==='turn/completed'&&v.params.turn.id===fixtureTurn.turnId))break;if(mockServer.fixtureError)throw mockServer.fixtureError;await pause(200);}
   const completed=notifications.find(v=>v.method==='turn/completed'&&v.params.turn.id===fixtureTurn.turnId);assert.equal(completed?.params.turn.status,'completed',JSON.stringify(completed?.params.turn.error??'ISOLATED_TURN_TIMEOUT'));
   assert.equal(mockRequests,3);assert.equal(automaticCells,2);
   const expected=join(dirname(binary),'codex-code-mode-host.exe').replaceAll("'","''");
   const observation=`$ErrorActionPreference='Stop';$owner=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value;$rows=@(Get-CimInstance Win32_Process -Filter 'ParentProcessId=${health.backendPid}'|Where-Object {$_.Name -eq 'codex-code-mode-host.exe'});foreach($row in $rows){if((Invoke-CimMethod -InputObject $row -MethodName GetOwnerSid).Sid -cne $owner -or ![string]::Equals($row.ExecutablePath,'${expected}',[StringComparison]::OrdinalIgnoreCase)){throw 'ISOLATED_AUTO_HOST_IDENTITY_REJECTED'}};@{count=$rows.Count;pid=if($rows.Count -eq 1){$rows[0].ProcessId}else{$null}}|ConvertTo-Json -Compress`;
   const identity=JSON.parse((await exec(ps,['-NoProfile','-NonInteractive','-Command',observation],{windowsHide:true,timeout:15000,maxBuffer:16384})).stdout);automaticHostCount=identity.count;automaticHostPid=identity.pid;assert.equal(automaticHostCount,1);automaticHostIdentity=true;fixtureTurn=null;
  }catch(error){if(fixtureTurn?.turnId)await request('turn/interrupt',fixtureTurn).catch(()=>{});throw error;}finally{ws.off('message',listener);}
 }
 if(observeAutoHost){
  // Observe before any explicit tool-host command. Do not infer execution from
  // process existence; this is the native sibling-discovery layer only.
  const expected=join(dirname(binary),'codex-code-mode-host.exe').replaceAll("'","''");
  const observation=`$ErrorActionPreference='Stop';$owner=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value;$rows=@(Get-CimInstance Win32_Process -Filter 'ParentProcessId=${health.backendPid}'|Where-Object {$_.Name -eq 'codex-code-mode-host.exe'});foreach($row in $rows){if((Invoke-CimMethod -InputObject $row -MethodName GetOwnerSid).Sid -cne $owner -or ![string]::Equals($row.ExecutablePath,'${expected}',[StringComparison]::OrdinalIgnoreCase)){throw 'ISOLATED_AUTO_HOST_IDENTITY_REJECTED'}};@{count=$rows.Count}|ConvertTo-Json -Compress`;
  automaticHostCount=JSON.parse((await exec(ps,['-NoProfile','-NonInteractive','-Command',observation],{windowsHide:true,timeout:15000,maxBuffer:16384})).stdout).count;
 }
 const result=await request('command/exec',{command:[process.execPath,join(source,'code-mode-probe.mjs'),join(dirname(binary),'codex-code-mode-host.exe')],cwd:home,sandboxPolicy:{type:'externalSandbox',networkAccess:'restricted'},timeoutMs:15000});
 assert.equal(result.exitCode,0);const proof=JSON.parse(result.stdout);
 assert.equal(proof.javascriptCellsExecuted,2);assert.equal(proof.persistentStoreVerified,true);assert.equal(proof.delegateCalls,0);
 cells+=2;
 health=await(await fetch(origin+'/health',{signal:AbortSignal.timeout(1000)})).json();assert.equal(health.activeTurns,0);assert.equal(health.desktopConnected,false);
 if(stopWithCommand){
  const helper=join(root,'hold-native-tool.mjs'),record=join(root,'held-tool.json'),nonce=randomUUID();
  await writeFile(helper,`import {spawn} from 'node:child_process';import {writeFile} from 'node:fs/promises';
const host=spawn(process.argv[2],['--listen','stdio'],{windowsHide:true,stdio:['pipe','pipe','ignore']});
const body=Buffer.from(JSON.stringify({type:'connection/hello',supportedVersions:[1],requiredCapabilities:[],optionalCapabilities:[]})),header=Buffer.alloc(4);header.writeUInt32LE(body.length);host.stdin.write(Buffer.concat([header,body]));
let bytes=Buffer.alloc(0),saved=false;host.stdout.on('data',async data=>{bytes=Buffer.concat([bytes,data]);if(saved||bytes.length<4||bytes.length<4+bytes.readUInt32LE(0))return;const value=JSON.parse(bytes.subarray(4,4+bytes.readUInt32LE(0)).toString());if(value.type!=='connection/ready'||value.selectedVersion!==1)process.exit(2);saved=true;await writeFile(process.argv[3],JSON.stringify({nonce:process.argv[4],helperPid:process.pid,hostPid:host.pid}));});
setInterval(()=>{},1000);`);
  // The pending command is consumed on shutdown; it cannot submit a native turn.
  void request('command/exec',{command:[process.execPath,helper,join(dirname(binary),'codex-code-mode-host.exe'),record,nonce],cwd:home,sandboxPolicy:{type:'externalSandbox',networkAccess:'restricted'},timeoutMs:isolatedDesktop?180000:60000}).catch(()=>{});
  for(let n=0;n<50;n++){heldCommand=await readFile(record,'utf8').then(JSON.parse).catch(e=>{if(e.code==='ENOENT')return null;throw e;});if(heldCommand)break;await pause(200);}
  assert.equal(heldCommand?.nonce,nonce,'Official tool host must actually be running before shutdown');
 }else ws.close();
 };
 await runCells();
 if(isolatedDesktop){
  // The verified installed bootstrap honors this explicit data path before its
  // singleton check and uses CODEX_HOME for startup requirements in this mode.
  // No daily profile or credentials are copied, and the default instance stays.
  const desktop=await realpath((await exec(ps,['-NoProfile','-NonInteractive','-Command',"Join-Path (Get-AppxPackage -Name OpenAI.Codex).InstallLocation 'app/ChatGPT.exe'"],{windowsHide:true})).stdout.trim());
  const profile=join(root,'desktop-profile');await mkdir(profile);
  const env={...process.env};
  for(const key of Object.keys(env))if(/API_KEY|TOKEN|SECRET|PROXY|OTEL|^CODEX_|^OPENAI_|^AGBRIO_/i.test(key))delete env[key];
  Object.assign(env,{CODEX_HOME:home,CODEX_ELECTRON_USER_DATA_PATH:profile,CODEX_APP_SERVER_WS_URL:origin.replace('http:','ws:')+'/rpc'});
  desktopStopScript=join(root,'retire-fixture-desktop.ps1');
  await writeFile(desktopStopScript,`param([uint32]$OwnedPid,[string]$Profile,[string]$Image,[string]$Scripts,[string]$Snapshot,[ValidateSet('Capture','Stop')][string]$Action)
$ErrorActionPreference='Stop'
$owner=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
function Identity([uint32]$Value){$raw=& (Join-Path $Scripts 'peer-service.ps1') -QueryPid $Value;return $raw|ConvertFrom-Json}
$root=Identity $OwnedPid
if($Action -eq 'Capture'){
 $row=Get-CimInstance Win32_Process -Filter ('ProcessId='+$OwnedPid)
 if(!$root -or !$row -or $root.ownerSid -cne $owner -or ![string]::Equals($root.imagePath,$Image,[StringComparison]::OrdinalIgnoreCase) -or !$row.CommandLine.Contains($Profile)){throw 'FIXTURE_DESKTOP_IDENTITY_REJECTED'}
 $known=@{};$known[$OwnedPid]=$root;$table=@(Get-CimInstance Win32_Process)
 do{$added=$false;foreach($entry in $table){$id=[uint32]$entry.ProcessId;$parent=$known[[uint32]$entry.ParentProcessId];if(!$parent -or $known.ContainsKey($id)){continue};$identity=Identity $id;if(!$identity -or $identity.ownerSid -cne $owner -or [long]$identity.createdAt -lt [long]$parent.createdAt){continue};if($entry.CreationDate -and [Math]::Abs(([datetime]$entry.CreationDate).ToFileTimeUtc()-[long]$identity.createdAt) -ge 10){continue};$known[$id]=$identity;$added=$true}}while($added)
 [IO.File]::WriteAllText($Snapshot,(@{root=$root;children=@($known.Values)}|ConvertTo-Json -Depth 8));'{}'
}else{
 $saved=Get-Content -LiteralPath $Snapshot -Raw|ConvertFrom-Json
 if($saved.root.pid -ne $OwnedPid -or $saved.root.ownerSid -cne $owner -or ![string]::Equals($saved.root.imagePath,$Image,[StringComparison]::OrdinalIgnoreCase)){throw 'FIXTURE_DESKTOP_IDENTITY_REJECTED'}
 foreach($entry in @($saved.root)+@($saved.children)){$live=Identity ([uint32]$entry.pid);if($live -and $live.pid -eq $entry.pid -and $live.createdAt -ceq $entry.createdAt -and $live.ownerSid -ceq $owner -and [string]::Equals($live.imagePath,$entry.imagePath,[StringComparison]::OrdinalIgnoreCase)){Stop-Process -Id ([int]$entry.pid) -ErrorAction Stop}}
 ' {}'
}
`);
  desktopChild=spawn(desktop,['--user-data-dir='+profile],{env,detached:true,windowsHide:false,stdio:'ignore'});desktopChild.on('error',()=>{});
  assert.ok(desktopChild.pid);desktopChild.fixtureProfile=profile;desktopChild.fixtureImage=desktop;
  for(let n=0;n<180;n++){const value=await(await fetch(origin+'/health',{signal:AbortSignal.timeout(1000)})).json();if(value.desktopConnected){desktopAttached=true;break;}if(desktopChild.exitCode!==null)break;await pause(200);}
  const params=['-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',desktopStopScript,'-OwnedPid',String(desktopChild.pid),'-Profile',profile,'-Image',desktop,'-Scripts',source,'-Snapshot',join(root,'desktop-identities.json')];
  await exec(ps,[...params,'-Action','Capture'],{windowsHide:true,timeout:20000,maxBuffer:262144});
  await exec(ps,[...params,'-Action','Stop'],{windowsHide:true,timeout:20000,maxBuffer:262144});desktopRetired=true;
  assert.equal(desktopAttached,true,'The real isolated Desktop must initialize on the gateway');
 }
 if(process.argv.includes('--restart-backend')){
  // Capture and recheck exact native identity and parent. Never name-match or
  // borrow the ordinary Desktop's backend, even though it uses the same binary.
  const helper=join(root,'restart-owned-native.ps1'),quoted=source.replaceAll("'","''");
  await writeFile(helper,`param([string]$Directory,[uint32]$RootPid,[uint32]$BackendPid,[string]$Binary,[ValidateSet('Capture','Stop')][string]$Action)
$ErrorActionPreference='Stop'
. '${quoted}/owned-processes.ps1'
$owner=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$parent=Get-SharedProcessIdentity $RootPid $Directory
$native=Get-SharedProcessIdentity $BackendPid $Directory
$row=Get-CimInstance Win32_Process -Filter ('ProcessId='+$BackendPid)
$path=Join-Path $Directory 'restart-identities.json'
if(!$parent -or !$native -or !$row -or $row.ParentProcessId -ne $RootPid -or $parent.ownerSid -cne $owner -or $native.ownerSid -cne $owner -or ![string]::Equals($native.imagePath,[IO.Path]::GetFullPath($Binary),[StringComparison]::OrdinalIgnoreCase)){throw 'ISOLATED_NATIVE_IDENTITY_REJECTED'}
if($Action -eq 'Capture'){
 $saved=(Get-Content -LiteralPath (Join-Path $Directory 'ready.json') -Raw|ConvertFrom-Json).supervisorIdentity
 if(!(Test-SharedProcessIdentity $saved $parent)){throw 'ISOLATED_SUPERVISOR_IDENTITY_REJECTED'}
 [IO.File]::WriteAllText($path,(@{root=$parent;native=$native}|ConvertTo-Json -Depth 5));'{}'
}else{
 $saved=Get-Content -LiteralPath $path -Raw|ConvertFrom-Json
 if(!(Test-SharedProcessIdentity $saved.root $parent) -or !(Test-SharedProcessIdentity $saved.native $native)){throw 'ISOLATED_NATIVE_IDENTITY_REJECTED'}
 Stop-SharedProcess $BackendPid;@{stopped=$true}|ConvertTo-Json -Compress
}
`);
  const old=health.backendPid;assert.ok(Number.isInteger(old)&&old>0);
  const act=async action=>JSON.parse((await exec(ps,['-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',helper,'-Directory',root,'-RootPid',String(child.pid),'-BackendPid',String(old),'-Binary',binary,'-Action',action],{windowsHide:true,timeout:15000,maxBuffer:16384})).stdout);
  await act('Capture');assert.equal((await act('Stop')).stopped,true);
  await waitReady(old);restarts=1;await runCells();
 }
 const stopped=await stopFixture();assert.equal(stopped.status,200);
 let timer;const exit=await Promise.race([closed,new Promise(yes=>{timer=setTimeout(()=>yes('timeout'),6000);})]);clearTimeout(timer);assert.equal(exit,0);
 assert.ok(Number.isInteger(health.backendPid)&&health.backendPid>0);
 const remaining=(await exec(ps,['-NoProfile','-NonInteractive','-Command',`if(Get-Process -Id ${health.backendPid} -ErrorAction SilentlyContinue){'ALIVE'}else{'ABSENT'}`],{windowsHide:true})).stdout.trim();
 assert.equal(remaining,'ABSENT');assert.deepEqual(await snapshot(),protectedBefore);
 if(stopWithCommand){
  assert.equal(JSON.parse(await readFile(join(root,'settings.json'),'utf8')).phase,'DISABLED');
  assert.equal(await lstat(join(root,'stop-processes.json')).catch(e=>{if(e.code==='ENOENT')return null;throw e;}),null);
  if(automaticHostPid)assert.ok(capturedChildren.some(v=>v.pid===automaticHostPid),'Capture the automatically discovered helper before shutdown');
  for(const pid of [heldCommand.helperPid,heldCommand.hostPid,...automaticHostPid?[automaticHostPid]:[]])assert.equal((await exec(ps,['-NoProfile','-NonInteractive','-Command',`if(Get-Process -Id ${pid} -ErrorAction SilentlyContinue){'ALIVE'}else{'ABSENT'}`],{windowsHide:true})).stdout.trim(),'ABSENT');
 }
 console.log(JSON.stringify({isolatedGatewayCodeMode:'PASS',version:inspected.version,javascriptCellsExecuted:cells,nativeRestartsObserved:restarts,persistentStoreVerified:true,threadsCreated:executeAutoHost?1:0,ephemeralFixtureThreads:executeAutoHost?1:0,modelRequests:0,loopbackMockRequests:mockRequests,delegateCalls:0,nativeCopies:0,desktopStarts:isolatedDesktop?1:0,productionSettingsUnchanged:true,ownedRuntimeStopped:true,ownedBackendAbsent:true,...isolatedDesktop?{isolatedDesktopAttach:desktopAttached?'PASS':'FAIL',fixtureDesktopRetired:desktopRetired,dailyProfileUsed:false}:{},...stopWithCommand?{inFlightNativeToolShutdown:'PASS',capturedNativeDescendants:capturedChildren.length,toolHelperAbsent:true,officialToolHostAbsent:true}:{},...observeAutoHost||executeAutoHost?{nativeAutoHostCount:automaticHostCount,nativeSiblingDiscovery:automaticHostCount>0?'PASS':'NOT_PROVEN',nativeAutomaticHostExecution:automaticHostIdentity?'PASS':'NOT_PROVEN',automaticJavaScriptCells:automaticCells}:{} }));
}finally{
 for(const p of pending.values())clearTimeout(p.timer);pending.clear();ws?.terminate();
 if(mockServer)await new Promise(yes=>mockServer.close(yes));
 if(desktopChild&&!desktopRetired){cleanable=false;process.stderr.write('ISOLATED_DESKTOP_REQUIRES_IDENTITY_RECOVERY\n');}
 if(child&&config&&origin&&(child.exitCode===null||(stopWithCommand&&!shutdownVerified))){
  try{const stopped=await stopFixture();if(stopped.status===200){let timer;await Promise.race([closed,new Promise(yes=>{timer=setTimeout(yes,6000);})]);clearTimeout(timer);}}catch{cleanable=false;}
 }
 // A failed stop must retain its evidence, not kill an unproven process tree.
 if(child&&(child.exitCode===null||(stopWithCommand&&!shutdownVerified))){cleanable=false;process.stderr.write('ISOLATED_RUNTIME_REQUIRES_RECOVERY\n');}
 if(cleanable){const link=join(runtime,'ws');try{assert.equal((await lstat(link)).isSymbolicLink(),true);await rmdir(link);}catch(e){if(e.code!=='ENOENT')throw e;}await clean(root);}
}
