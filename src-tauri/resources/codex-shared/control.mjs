// Owner-local lifecycle. No token, URL credential or public message is logged.
import {spawn,execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {readFile,writeFile,rename,mkdir,unlink} from 'node:fs/promises';
import {dirname,join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {randomBytes,randomUUID} from 'node:crypto';
import {createRequire} from 'node:module';
const exec=promisify(execFile),sleep=ms=>new Promise(r=>setTimeout(r,ms));
export async function atomic(path,value){await writeFile(path+'.pending',JSON.stringify(value));await rename(path+'.pending',path);}
export function desktopLaunchOptions(url,environment=process.env){
 // libuv's Windows job kills non-detached children when this controller exits.
 // Desktop is explicitly requested as a visible, independent user application.
 return {detached:true,windowsHide:false,stdio:'ignore',env:{...environment,CODEX_APP_SERVER_WS_URL:url}};
}
export function restoreChanges(snapshot,installed){return Object.entries(installed).map(([name,value])=>({name,installed:value,previous:snapshot[name]??null}));}
export function sharedUrl(value){try{const u=new URL(value);return ['ws:','wss:'].includes(u.protocol)&&u.hostname==='127.0.0.1'&&u.port==='47116'&&u.pathname==='/rpc'&&!u.username&&!u.password&&!u.search&&!u.hash;}catch{return false;}}
export async function lifecycle(options,dependencies={}){
 const persist=dependencies.atomic??atomic,execute=dependencies.exec??exec,createProcess=dependencies.spawn??spawn,request=dependencies.fetch??fetch,pause=dependencies.sleep??sleep,now=dependencies.now??Date.now;
 const {directory,binary,version,desktop,action}=options,settingsPath=join(directory,'settings.json'),configPath=join(directory,'runtime-config.json'),journalPath=join(directory,'environment-journal.json');
 const ps=join(process.env.WINDIR,'System32/WindowsPowerShell/v1.0/powershell.exe'),script=join(options.scripts??join(directory,'runtime'),'environment.ps1');
 const run=dependencies.run??(async(args)=>JSON.parse((await execute(ps,['-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',script,...args],{windowsHide:true,timeout:20000,maxBuffer:262144})).stdout));
 const load=async(path)=>{try{return JSON.parse(await readFile(path,'utf8'));}catch(e){if(e.code==='ENOENT')return null;throw Error('SHARED_SETTINGS_INVALID');}};
 let saved=await load(settingsPath),cfg=await load(configPath);
 const health=async()=>{try{const v=await(await request(`http://127.0.0.1:${cfg.port}/health`,{signal:AbortSignal.timeout(1500)})).json();return v.instance===cfg.instance?v:null;}catch{return null;}};
 const status=async()=>{const h=cfg?.format===2?await health():null;return{enabled:saved?.enabled===true&&saved?.format===2,ready:!!h?.ready,desktopConnected:!!h?.desktopConnected,state:saved?.format!==2?(saved?.enabled?'LEGACY_REPAIR_REQUIRED':'INDEPENDENT'):saved.phase==='RECOVERY_REQUIRED'?'RECOVERY_REQUIRED':saved.phase==='ACTIVE'?(h?.desktopConnected?'SHARED_CONNECTED':'SHARED_RECONNECTING'):saved.phase==='PREPARED'&&h?.ready?'SHARED_PREPARED':'INDEPENDENT',updatePending:false,launchProgress:await load(join(directory,'launch-progress.json')).catch(()=>null)};};
 const stop=async()=>{
  const h=cfg?.format===2?await health():null;
  if(h?.activeTurns)throw Error('SHARED_DESKTOP_BUSY');
  if(h){const r=await request(`http://127.0.0.1:${cfg.port}/stop`,{method:'POST',headers:{Authorization:'Bearer '+cfg.controlToken},signal:AbortSignal.timeout(6000)});if(!r.ok)throw Error(r.status===409?'SHARED_DESKTOP_BUSY':'SHARED_STOP_FAILED');
   for(let i=0;i<40;i++){if(!await health())break;await sleep(100);}
  }
  // Also cleans verified startup failures and legacy supervisors without /stop.
  await run(['-Action','Stop','-Directory',directory]);
  if(cfg?.format===2&&await health())throw Error('SHARED_STOP_FAILED');
 };
 const restore=async()=>{
  let journal=await load(journalPath);
  if(!journal&&saved&&saved.format!==2){
   const before=await run(['-Action','Snapshot']);const changes=[];
   if(sharedUrl(before.CODEX_APP_SERVER_WS_URL))changes.push({name:'CODEX_APP_SERVER_WS_URL',installed:before.CODEX_APP_SERVER_WS_URL,previous:sharedUrl(saved.previousDesktopUrl)?null:saved.previousDesktopUrl??null});
   if(before.NODE_EXTRA_CA_CERTS===join(directory,'trusted-ca.pem'))changes.push({name:'NODE_EXTRA_CA_CERTS',installed:before.NODE_EXTRA_CA_CERTS,previous:saved.previousCa??null});
   if(before.NODE_USE_SYSTEM_CA==='1')changes.push({name:'NODE_USE_SYSTEM_CA',installed:'1',previous:saved.previousSystemCa??null});
   journal={changes};await persist(journalPath,journal);
  }
  if(journal)await run(['-Action','Restore','-SnapshotPath',journalPath]);
 };
 const rollback=async()=>{
  if((cfg?.format===2?await health():null)?.activeTurns)throw Error('SHARED_DESKTOP_BUSY');
  const failures=[];try{await restore();}catch(e){failures.push(e);}
  try{await stop();}catch(e){if(e.message==='SHARED_DESKTOP_BUSY')throw e;failures.push(e);}
  try{await run(['-Action','CleanCertificate','-Directory',directory]);}catch(e){failures.push(e);}
  if(saved){saved={...saved,enabled:false,phase:failures.length?'RECOVERY_REQUIRED':'DISABLED'};try{await persist(settingsPath,saved);}catch(e){failures.push(e);}}
  if(cfg){cfg={...cfg,enabled:false,phase:failures.length?'RECOVERY_REQUIRED':'DISABLED'};try{await persist(configPath,cfg);}catch(e){failures.push(e);}}
  if(failures.length)throw Error('SHARED_ROLLBACK_INCOMPLETE');
  await unlink(journalPath).catch(e=>{if(e.code!=='ENOENT')throw e;});
 };
 if(action==='status')return status();
 if(action==='disable'){await rollback();return status();}
 if(action==='launch'){
  const progress=async(step,state='RUNNING',errorCode=null)=>persist(join(directory,'launch-progress.json'),{step,state,errorCode,updatedAt:now()}).catch(()=>{});
  await progress('CHECKING');
  try{
  if(saved?.format!==2||!['PREPARED','ACTIVE'].includes(saved.phase)||!(await health())?.ready)throw Error('SHARED_NOT_PREPARED');
  // Never close or launch into the owner's already-running Desktop instance.
  const scan=await execute(ps,['-NoProfile','-NonInteractive','-Command',"$ErrorActionPreference='Stop';$root=(Get-AppxPackage -Name 'OpenAI.Codex').InstallLocation;$desktop=Join-Path $root 'app/ChatGPT.exe';if(!(Test-Path -LiteralPath $desktop)){throw 'SHARED_DESKTOP_BINARY_UNRESOLVED'};$count=@(Get-CimInstance Win32_Process -Filter \"Name='ChatGPT.exe'\" | Where-Object {$_.ExecutablePath -match '^C:\\\\Program Files\\\\WindowsApps\\\\OpenAI\\.Codex_'}).Count;@{desktop=$desktop;count=$count}|ConvertTo-Json -Compress"],{windowsHide:true,timeout:10000});
  let installed;try{installed=JSON.parse(scan.stdout);}catch{throw Error('SHARED_DESKTOP_BINARY_UNRESOLVED');}
  if(!Number.isInteger(installed.count)||typeof installed.desktop!=='string'||!installed.desktop)throw Error('SHARED_DESKTOP_BINARY_UNRESOLVED');
  if(installed.count>0){
   if((await health())?.desktopConnected){
    await progress('SHOWING');
    const shown=await run(['-Action','ShowDesktop']);
    if(!shown.visible)throw Error('SHARED_DESKTOP_WINDOW_UNAVAILABLE');
    await progress('CONNECTED','DONE');return status();
   }
   throw Error('SHARED_DESKTOP_CLOSE_REQUIRED');
  }
  const originalSaved={...saved},originalConfig={...cfg},reopening=saved.phase==='ACTIVE';
  let child;
  try{
   await progress('LAUNCHING');
   saved.desktop=installed.desktop;saved.phase='CONNECTING';await persist(settingsPath,saved);
   child=createProcess(saved.desktop,[],desktopLaunchOptions(`ws://127.0.0.1:${cfg.port}/rpc`));
   child.on('error',()=>{});
   await progress('CONNECTING');
   const deadline=now()+45000;let connected=false,waitingWindow=false;
   while(now()<deadline){
    if(child.exitCode!==null)break;
    if((await health())?.desktopConnected){
     if(!waitingWindow){waitingWindow=true;await progress('SHOWING');}
     const window=await run(['-Action','DesktopWindow','-DesktopPid',String(child.pid)]);
     if(window.visible){await pause(600);connected=child.exitCode===null&&!!(await health())?.desktopConnected;if(connected)break;}
    }
    await pause(200);
   }
   if(!connected)throw Error('SHARED_DESKTOP_CONNECTION_FAILED');
   // Commit product state only. Desktop's persistent default endpoint stays
   // untouched even after success, so an absent/crashed gateway cannot trap a
   // normal Desktop launch. Only this explicitly launched process uses ws://.
   cfg.enabled=true;cfg.phase='ACTIVE';await persist(configPath,cfg);saved.enabled=true;saved.phase='ACTIVE';await persist(settingsPath,saved);
   let iconApplied=false;
   try{const result=await execute(ps,['-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',join(options.scripts??join(directory,'runtime'),'desktop-identity.ps1'),'-DesktopPid',String(child.pid),'-Directory',directory],{windowsHide:true,timeout:10000});iconApplied=JSON.parse(result.stdout).applied===true;}catch{/* A cosmetic failure cannot roll back a connected Desktop. */}
   await persist(join(directory,'desktop-launch.json'),{attemptedAt:Date.now(),pid:child.pid,state:'CONNECTED',windowVisible:true,iconApplied}).catch(()=>{});
   child.unref();await progress('CONNECTED','DONE');return status();
  }catch(e){
   await persist(join(directory,'desktop-launch.json'),{attemptedAt:Date.now(),pid:child?.pid??null,state:'FAILED',errorCode:/^SHARED_[A-Z_]+$/.test(e.message)?e.message:'SHARED_LIFECYCLE_FAILED',exitCode:child?.exitCode??null}).catch(()=>{});
   try{
    // Reopening an existing shared runtime is not a new enable transaction.
    // A failed window must never stop its backend or other active conversations.
    if(reopening){saved=originalSaved;cfg=originalConfig;await persist(configPath,cfg);await persist(settingsPath,saved);}
    else await rollback();
   }finally{
    // Only our newly launched process can be closed on failed activation.
    if(child&&child.exitCode===null)child.kill();
   }
   throw e;
  }
  }catch(e){await progress('FAILED','FAILED',/^SHARED_[A-Z_]+$/.test(e.message)?e.message:'SHARED_LIFECYCLE_FAILED');throw e;}
 }
 if(action!=='setup')throw Error('SHARED_ACTION_INVALID');
 if(saved?.format===2&&['ACTIVE','PREPARED'].includes(saved.phase)&&(await health())?.ready)return status();
 // Recovery is idempotent; legacy self-pointers are never saved as originals.
 if(saved||cfg)await rollback();
 await run(['-Action','Protect','-Directory',directory]);
 const snapshot=await run(['-Action','Snapshot']);if(snapshot.CODEX_APP_SERVER_WS_URL&&!sharedUrl(snapshot.CODEX_APP_SERVER_WS_URL))throw Error('SHARED_OTHER_DESKTOP_ENDPOINT_CONFIGURED');
 if(snapshot.CODEX_APP_SERVER_WS_URL)throw Error('SHARED_STALE_ENDPOINT_REQUIRES_REPAIR');
 const nativeVersion=(await execute(binary,['--version'],{windowsHide:true,timeout:10000})).stdout.trim();if(nativeVersion!==`codex-cli ${version}`)throw Error('SHARED_NATIVE_BINARY_UNVERIFIED');
 const sid=(await execute(ps,['-NoProfile','-NonInteractive','-Command','[Security.Principal.WindowsIdentity]::GetCurrent().User.Value'],{windowsHide:true,timeout:10000})).stdout.trim();
 cfg={format:2,enabled:false,phase:'PREPARING',binary,version,desktop,ownerSid:sid,port:47116,instance:randomUUID(),controlToken:randomBytes(32).toString('hex')};
 cfg={...cfg,...dependencies.config};saved={...cfg,originalEnvironment:snapshot};delete saved.controlToken;
 try{
  await persist(settingsPath,saved);await persist(configPath,cfg);
  const child=createProcess(join(directory,'runtime/node.exe'),[join(directory,'runtime/runtime.mjs'),configPath],{detached:true,windowsHide:true,stdio:'ignore'});child.on('error',()=>{});child.unref();
  let ready=false;for(let i=0;i<100;i++){if((await health())?.ready){ready=true;break;}if(child.exitCode!==null)break;await sleep(200);}
  if(!ready)throw Error('SHARED_START_FAILED');
  // Prepare only native RPC; never launch a second Desktop. The owner's single
  // Desktop must connect successfully during explicit launch before activation.
  await (dependencies.probe??probeGateway)({directory,url:`ws://127.0.0.1:${cfg.port}/rpc`});
  cfg.phase='PREPARED';saved.phase='PREPARED';await persist(configPath,cfg);await persist(settingsPath,saved);
  return status();
 }catch(e){await rollback();throw e;}
}
export async function probeGateway({directory,url}){
 const require=createRequire(import.meta.url),{WebSocket}=require(join(directory,'runtime/ws/index.js'));
 const ws=new WebSocket(url,{handshakeTimeout:6000});
 try{
  await new Promise((resolve,reject)=>{ws.once('open',resolve);ws.once('error',()=>reject(Error('SHARED_GATEWAY_PREFLIGHT_FAILED')));});
  await new Promise((resolve,reject)=>{
   const timer=setTimeout(()=>reject(Error('SHARED_GATEWAY_PREFLIGHT_FAILED')),6000);
   ws.on('message',data=>{try{const value=JSON.parse(data.toString());if(value.id===1){clearTimeout(timer);value.error?reject(Error('SHARED_GATEWAY_PREFLIGHT_FAILED')):resolve();}}catch{}});
   ws.send(JSON.stringify({id:1,method:'initialize',params:{clientInfo:{name:'aiwr_gateway_probe',version:'0.1.0'}}}));
  });
 }finally{ws.close();}
}
const normalizedEntry=path=>path.replace(/^\\\\\?\\/,'').replaceAll('\\','/').toLowerCase();
if(process.argv[1]&&normalizedEntry(fileURLToPath(import.meta.url))===normalizedEntry(resolve(process.argv[1]))){
 try{const result=await lifecycle(JSON.parse(await readFile(process.argv[2],'utf8')));process.stdout.write(JSON.stringify(result));}
 catch(e){process.stderr.write(/^SHARED_[A-Z_]+$/.test(e.message)?e.message:'SHARED_LIFECYCLE_FAILED');process.exitCode=1;}
}
