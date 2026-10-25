// Owner-local launch-token boundary. No remote API, shell text, credentials or native permission edits.
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {readFile,mkdir,writeFile,open,unlink,rmdir,lstat} from 'node:fs/promises';
import {join,dirname,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {replaceStateFile} from './state-file.mjs';
import {randomUUID} from 'node:crypto';
const execute=promisify(execFile),source=dirname(fileURLToPath(import.meta.url));
// Set only after a validated child request, while its Windows wrapper owns the mutation mutex.
let heldScope=null;
const windowsPowerShell=()=>join(process.env.WINDIR,'System32/WindowsPowerShell/v1.0/powershell.exe');
export async function launchPreference(directory){
 let value;try{value=JSON.parse(await readFile(join(directory,'launch-preferences.json'),'utf8'));}catch(e){if(e.code==='ENOENT')return true;throw Error('SHARED_LAUNCH_PREFERENCES_INVALID');}
 if(!value||Object.keys(value).length!==1||typeof value.launchAsAdministrator!=='boolean')throw Error('SHARED_LAUNCH_PREFERENCES_INVALID');
 return value.launchAsAdministrator;
}
export async function inspectPrivileges(executeFile=execute){
 try{const {stdout}=await executeFile(windowsPowerShell(),['-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',join(source,'privilege.ps1'),'-Action','Inspect'],{windowsHide:true,timeout:15000});const v=JSON.parse(stdout);if(!v.ownerSid?.startsWith('S-1-')||typeof v.elevated!=='boolean')throw Error();return v;}catch{throw Error('SHARED_PRIVILEGE_UNCONFIRMED');}
}
export async function verifyProcessPrivilege(pid,ownerSid,imagePath,elevated){
 const {stdout}=await execute(windowsPowerShell(),['-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',join(source,'peer-service.ps1'),'-QueryPid',String(pid)],{windowsHide:true,timeout:15000});
 const v=JSON.parse(stdout),canonical=p=>resolve(p.replace(/^\\\\\?\\/,'' )).toLowerCase();
 if(v?.pid!==pid||v.ownerSid!==ownerSid||canonical(v.imagePath)!==canonical(imagePath)||v.elevated!==elevated)throw Error('SHARED_PRIVILEGE_TOKEN_MISMATCH');
 return v;
}
async function runtimePrivilege(directory,identity){
 let cfg;try{cfg=JSON.parse(await readFile(join(directory,'runtime-config.json'),'utf8'));}catch(e){if(e.code==='ENOENT')return null;throw Error('SHARED_SETTINGS_INVALID');}
 if(cfg.format!==2)return null;
 let health;try{health=await(await fetch(`http://127.0.0.1:${cfg.port}/health`,{signal:AbortSignal.timeout(1500)})).json();}catch{return null;}
 if(health.instance!==cfg.instance||health.ready!==true)return null;
 // Resolve the live token even for an older cohort without the new health field.
 const {stdout}=await execute(windowsPowerShell(),['-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',join(source,'peer-service.ps1'),'-QueryPid',String(health.supervisorPid)],{windowsHide:true,timeout:15000});
 const owner=JSON.parse(stdout);
 if(owner?.pid!==health.supervisorPid||owner.ownerSid!==identity.ownerSid||resolve(owner.imagePath).toLowerCase()!==resolve(directory,'runtime/node.exe').toLowerCase()||typeof owner.elevated!=='boolean')throw Error('SHARED_PRIVILEGE_UNCONFIRMED');
 return {...health,elevated:owner.elevated};
}
export function requiredPrivilege(kind,desired,current,runtime){
 if(kind==='disable')return current||(runtime?.elevated??desired);
 if(kind==='launch'&&runtime){
  if(runtime.desktopConnected)return runtime.elevated; // Only reveal the already-running window.
  if(runtime.elevated!==desired)throw Error('SHARED_PRIVILEGE_RESTART_REQUIRED');
 }
 if(kind==='setup'&&runtime)return runtime.elevated; // Reuse never promotes/demotes running work.
 return desired;
}
export async function dispatchLifecycle(options,dependencies={}){
 const run=dependencies.run??(()=>import('./control.mjs').then(m=>m.lifecycle(options)));
 if(['status','prepare-check'].includes(options.action))return run();
 if(!['setup','launch','disable'].includes(options.action))throw Error('SHARED_ACTION_INVALID');
 const identity=await(dependencies.inspect??inspectPrivileges)(),runtime=await(dependencies.runtime??runtimePrivilege)(options.directory,identity);
 const inScope=heldScope?.directory===resolve(options.directory).toLowerCase();
 const desired=inScope?heldScope.elevated:await(dependencies.preference??launchPreference)(options.directory);
 const elevated=requiredPrivilege(options.action,desired,identity.elevated,runtime);
 if(inScope&&identity.elevated===elevated)return run();
 return(dependencies.submit??submitPrivilegeJob)({kind:'control',options,elevated,identity});
}
export async function dispatchEntry(options,run,dependencies={}){
 const identity=await(dependencies.inspect??inspectPrivileges)(),desired=await(dependencies.preference??launchPreference)(options.directory);
 if(identity.elevated===desired)return run();
 const runtime=await(dependencies.runtime??runtimePrivilege)(options.directory,identity);
 if(runtime?.desktopConnected)return run(); // Showing a live window does not change its token.
 if(runtime&&runtime.elevated!==desired)throw Error('SHARED_PRIVILEGE_RESTART_REQUIRED');
 const desktop=await(dependencies.desktop??(async()=>{const m=await import('./control.mjs');return m.readDesktopState(options.source);}))();
 if(!Number.isInteger(desktop.count)||desktop.count<0)throw Error('SHARED_DESKTOP_BINARY_UNRESOLVED');
 if(desktop.count>0)throw Error('SHARED_DESKTOP_CLOSE_REQUIRED');
 if(!runtime?.activeTurns)await(dependencies.precheck??(async()=>{const m=await import('./control.mjs');return m.lifecycle({directory:options.directory,scripts:options.source,action:'prepare-check'});}))();
 // Starting through the independent shortcut gets one token handoff for the complete entry.
 return(dependencies.submit??submitPrivilegeJob)({kind:'entry',options,elevated:desired,identity});
}
export async function probePrivilege(directory,elevated,verifyDowngrade=false){const identity=await inspectPrivileges();return submitPrivilegeJob({kind:'probe',options:{directory,verifyDowngrade},elevated,identity});}
async function submitPrivilegeJob({kind,options,elevated,identity}){
 const directory=resolve(options.directory),expected=resolve(process.env.LOCALAPPDATA,'AIWorkRouter/codex-shared');
 if(directory.toLowerCase()!==expected.toLowerCase())throw Error('SHARED_PRIVILEGE_REQUEST_INVALID');
 const id=randomUUID(),folder=join(directory,'launch-jobs',id),requestPath=join(folder,'request.json');
 await mkdir(folder,{recursive:true});
 const request={format:1,id,kind,mode:elevated?'administrator':'standard',ownerSid:identity.ownerSid,createdAt:Date.now(),callerPid:process.pid,callerCreatedAt:identity.callerCreatedAt,environment:{CODEX_HOME:process.env.CODEX_HOME?resolve(process.env.CODEX_HOME):null},node:process.execPath,options};
 await writeFile(requestPath,JSON.stringify(request),{flag:'wx'});
 try{
  await execute(windowsPowerShell(),['-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',join(source,'privilege.ps1'),'-Action','Start','-Directory',directory,'-JobId',id,'-Mode',request.mode],{windowsHide:true,timeout:840000});
 }catch(e){const code=String(e.stderr??'').match(/SHARED_[A-Z_]+/)?.[0]??'SHARED_PRIVILEGE_UNCONFIRMED';throw Error(code);}
 const deadline=Date.now()+600000;
 while(Date.now()<deadline){
  let result;try{result=JSON.parse((await readFile(join(folder,'result.json'),'utf8')).replace(/^\uFEFF/,''));}catch(e){if(e.code!=='ENOENT')throw Error('SHARED_PRIVILEGE_UNCONFIRMED');}
  const released=await lstat(join(folder,'released')).then(s=>s.isFile()&&!s.isSymbolicLink()).catch(e=>{if(e.code==='ENOENT')return false;throw e;});
  if(result&&released){
   if(result.id!==id||result.ownerSid!==identity.ownerSid||result.mode!==request.mode)throw Error('SHARED_PRIVILEGE_UNCONFIRMED');
   if(!result.ok)throw Error(result.errorCode);
   for(const name of ['result.json','claimed','request.json','released'])await unlink(join(folder,name));await rmdir(folder);
   return result.value;
  }
  await new Promise(r=>setTimeout(r,150));
 }
 throw Error('SHARED_PRIVILEGE_UNCONFIRMED'); // Never retry/kill a potentially running operation.
}
async function child(requestPath){
 const request=JSON.parse(await readFile(requestPath,'utf8')),folder=dirname(requestPath),expected=resolve(process.env.LOCALAPPDATA,'AIWorkRouter/codex-shared');
 if(request.format!==1||!['control','entry','probe'].includes(request.kind)||!['administrator','standard'].includes(request.mode)||!/^[-a-f0-9]{36}$/.test(request.id)||resolve(folder).toLowerCase()!==resolve(expected,'launch-jobs',request.id).toLowerCase()||resolve(request.options.directory).toLowerCase()!==expected.toLowerCase()||!Number.isFinite(request.createdAt)||!Number.isInteger(request.callerPid)||request.callerPid<=0||typeof request.callerCreatedAt!=='string'||Date.now()-request.createdAt<0||Date.now()-request.createdAt>600000)throw Error('SHARED_PRIVILEGE_REQUEST_INVALID');
 for(const path of [expected,join(expected,'launch-jobs'),folder,requestPath])if((await lstat(path)).isSymbolicLink())throw Error('SHARED_PRIVILEGE_REQUEST_INVALID');
 const identity=await inspectPrivileges();
 if(identity.ownerSid!==request.ownerSid||identity.elevated!==(request.mode==='administrator'))throw Error('SHARED_PRIVILEGE_TOKEN_MISMATCH');
 const claimed=await open(join(folder,'claimed'),'wx');await claimed.close();
 if(request.kind!=='probe')heldScope={directory:expected.toLowerCase(),elevated:identity.elevated};
 let result;
 try{
  if(request.environment?.CODEX_HOME!=null){if(typeof request.environment.CODEX_HOME!=='string'||!request.environment.CODEX_HOME||request.environment.CODEX_HOME.includes('\0'))throw Error('SHARED_PRIVILEGE_REQUEST_INVALID');process.env.CODEX_HOME=request.environment.CODEX_HOME;}
  let value;
  if(request.kind==='probe'){value=identity;if(request.options.verifyDowngrade){if(!identity.elevated)throw Error('SHARED_PRIVILEGE_TOKEN_MISMATCH');value.standardChild=await probePrivilege(expected,false);}}
  else if(request.kind==='control'){
   if(!['setup','launch','disable'].includes(request.options.action)||resolve(request.options.scripts)!==source)throw Error('SHARED_PRIVILEGE_REQUEST_INVALID');
   value=await(await import('./control.mjs')).lifecycle(request.options);
  }else{
   if(resolve(request.options.source)!==source||request.options.node!==process.execPath)throw Error('SHARED_PRIVILEGE_REQUEST_INVALID');
   value=await(await import('./launch-entry.mjs')).launchEntry(request.options);
  }
  result={ok:true,value};
 }catch(e){result={ok:false,errorCode:/^SHARED_[A-Z_]+$/.test(e.message)?e.message:'SHARED_PRIVILEGE_OPERATION_FAILED'};}
 await replaceStateFile(join(folder,'result.json'),{...result,id:request.id,ownerSid:identity.ownerSid,mode:request.mode});
}
if(process.argv[2]==='--child'&&resolve(process.argv[1])===fileURLToPath(import.meta.url))child(resolve(process.argv[3])).catch(()=>{process.exitCode=1;});
