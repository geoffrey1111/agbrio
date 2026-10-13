import {lifecycle,readDesktopState} from './control.mjs';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {mkdir,readFile,writeFile,readdir,lstat,copyFile,rename,unlink,rmdir} from 'node:fs/promises';
import {join,dirname,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {randomUUID,createHash} from 'node:crypto';
const execute=promisify(execFile);
async function resourceMap(root){
 const result=new Map();
 async function walk(path,prefix=''){if((await lstat(path)).isSymbolicLink())throw Error('SHARED_RUNTIME_LINK_REJECTED');for(const name of await readdir(path)){const file=join(path,name),stat=await lstat(file),key=prefix+name;if(stat.isSymbolicLink())throw Error('SHARED_RUNTIME_LINK_REJECTED');if(stat.isDirectory())await walk(file,key+'/');else result.set(key,createHash('sha256').update(await readFile(file)).digest('hex'));}}
 await walk(root);return result;
}
export async function sharedResourcesMatch({directory,source,node}){
 // Compare actual files, not a marker or the native version. Missing, renamed,
 // changed or linked resources cannot authorize reuse of an idle old writer.
 try{
  const expected=await resourceMap(source),ws=await resourceMap(join(dirname(source),'browser-executor/node_modules/ws'));
  for(const [name,hash] of ws)expected.set('ws/'+name,hash);
  expected.set('node.exe',createHash('sha256').update(await readFile(node)).digest('hex'));
  const actual=await resourceMap(join(directory,'runtime'));actual.delete('version');
  return expected.size===actual.size&&[...expected].every(([name,hash])=>actual.get(name)===hash);
 }catch(error){if(error.code==='ENOENT')return false;throw error;}
}
async function copyTree(from,to){if((await lstat(from)).isSymbolicLink())throw Error('SHARED_RUNTIME_LINK_REJECTED');await mkdir(to,{recursive:true});for(const name of await readdir(from)){const source=join(from,name),stat=await lstat(source);if(stat.isSymbolicLink())throw Error('SHARED_RUNTIME_LINK_REJECTED');if(stat.isDirectory())await copyTree(source,join(to,name));else await copyFile(source,join(to,name));}}
async function discardStage(path){for(const name of await readdir(path)){const entry=join(path,name),stat=await lstat(entry);if(stat.isSymbolicLink())throw Error('SHARED_RUNTIME_LINK_REJECTED');if(stat.isDirectory())await discardStage(entry);else await unlink(entry);}await rmdir(path);}
export async function stageSharedResources({directory,source,node,stamp},{move=rename}={}){
  if(stamp!==undefined&&!/^[a-f0-9]{64}$/.test(stamp))throw Error('SHARED_RUNTIME_STAMP_INVALID');
  const next=join(directory,'runtime-next-'+randomUUID());
  try{await copyTree(source,next);
  await copyTree(join(dirname(source),'browser-executor/node_modules/ws'),join(next,'ws'));await copyFile(node,join(next,'node.exe'));
  if(stamp!==undefined)await writeFile(join(next,'version'),stamp);
  const current=join(directory,'runtime');let exists=false;try{const stat=await lstat(current);if(stat.isSymbolicLink())throw Error('SHARED_RUNTIME_LINK_REJECTED');exists=stat.isDirectory();}catch(e){if(e.code!=='ENOENT')throw e;}
  const old=join(directory,'runtime-previous-'+randomUUID());if(exists)await move(current,old);
  try{await move(next,current);}catch(e){if(exists){try{await move(old,current);}catch{throw Error("SHARED_RUNTIME_ROLLBACK_FAILED");}}throw Error("SHARED_RUNTIME_PROMOTION_FAILED");}
  }finally{try{const stat=await lstat(next);if(stat.isSymbolicLink())throw Error('SHARED_RUNTIME_LINK_REJECTED');if(dirname(resolve(next))!==resolve(directory)||!next.includes('runtime-next-'))throw Error('SHARED_RUNTIME_CLEANUP_REJECTED');await discardStage(next);}catch(e){if(e.code!=='ENOENT')throw e;}}
}
export async function launchEntry(options,dependencies={}){
 let stage='STATUS';
 try{
 const control=dependencies.control??lifecycle,exec=dependencies.exec??execute;
 const {directory,source,node}=options,base={directory,scripts:source};
 const previous=await control({...base,action:'status'});
 // Reuse the live owner. No update, background stop, or second native writer.
 if(previous.ready&&(previous.desktopConnected||previous.activeTurns!==0)){stage='DESKTOP_LAUNCH';return await control({...base,action:'launch'});}
 // Fail before native retention, resource staging or starting any shared writer.
 // A normal Desktop owns its backend until the owner explicitly exits it.
 stage='DESKTOP_STATE';const currentDesktop=await (dependencies.desktopState??(()=>readDesktopState(source,exec)))();
 if(!Number.isInteger(currentDesktop.count)||currentDesktop.count<0)throw Error('SHARED_DESKTOP_BINARY_UNRESOLVED');
 if(currentDesktop.count>0)throw Error('SHARED_DESKTOP_CLOSE_REQUIRED');
 // No native retention while an unreachable supervisor may still own work.
 stage='PREPARE_CHECK';await control({...base,action:'prepare-check'});
 const native=dependencies.discover??(async()=>{
  const p=join(process.env.WINDIR,'System32/WindowsPowerShell/v1.0/powershell.exe');
  const out=await exec(p,['-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',join(source,'desktop-runtime.ps1'),'-Directory',directory,'-ProbeNode',node],{windowsHide:true,timeout:120000,maxBuffer:262144});return JSON.parse(out.stdout);
 });
 stage='NATIVE_DISCOVERY';const candidate=await native(); // Entire candidate is checked before old state changes.
 stage='RESOURCE_VALIDATION';if(previous.ready){const saved=await (dependencies.saved??(async()=>JSON.parse(await readFile(join(directory,'settings.json'),'utf8'))))();if(saved.binary===candidate.binary&&saved.version===candidate.version&&await (dependencies.resourcesMatch??sharedResourcesMatch)(options)){stage='DESKTOP_LAUNCH';return await control({...base,action:'launch'});}}
 stage='DISABLE';await control({...base,action:'disable'});
 stage='RESOURCE_STAGING';await (dependencies.stage??(()=>stageSharedResources(options)))();
 stage='NATIVE_SETUP';await control({...base,...candidate,action:'setup'});
 stage='DESKTOP_LAUNCH';try{return await control({...base,action:'launch'});}catch(error){
  // This entry owns the new preparation, unlike reuse of a resident above.
  // Failed activation must retire it, with existing busy/identity guards intact.
  try{await control({...base,action:'disable'});}catch{stage='RECOVERY';throw Error('SHARED_ROLLBACK_INCOMPLETE');}
  throw error;
 }
 }catch(error){const code=String(error.stderr??error.message).match(/SHARED_[A-Z_]+/)?.[0]??'SHARED_LAUNCH_NOT_COMPLETED';throw Object.assign(Error(code,{cause:error}),{launchStage:stage});}
}
if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url)){
 try{
 if(process.argv[2]==='--stage-only'){
  if(process.argv.length!==7)throw Error('SHARED_RUNTIME_STAGE_ARGUMENTS_INVALID');
  await stageSharedResources({source:resolve(process.argv[3]),directory:resolve(process.argv[4]),node:resolve(process.argv[5]),stamp:process.argv[6]});
  process.stdout.write(JSON.stringify({staged:true}));
 }else{
  const source=resolve(process.argv[2]);
  await launchEntry({source,directory:join(process.env.LOCALAPPDATA,'AIWorkRouter/codex-shared'),node:join(dirname(source),'browser-executor-node.exe')});
 }
 }
 catch(e){
  const code=String(e.stderr??e.message).match(/SHARED_[A-Z_]+/)?.[0]??(process.argv[2]==='--stage-only'?'SHARED_RUNTIME_COPY_FAILED':'SHARED_LAUNCH_NOT_COMPLETED');
  const diagnostic={code,stage:e.launchStage??'ENTRY',updatedAt:Date.now()};
  if(process.argv[2]!=='--stage-only')try{const directory=join(process.env.LOCALAPPDATA,'AIWorkRouter/codex-shared'),path=join(directory,'launch-diagnostic.json');if((await lstat(directory)).isSymbolicLink())throw Error('link');const existing=await lstat(path).catch(err=>{if(err.code==='ENOENT')return null;throw err;});if(existing?.isSymbolicLink()||existing?.isDirectory())throw Error('link');await writeFile(path,JSON.stringify(diagnostic));}catch{/* Diagnostics cannot create a missing owner root or hide recovery failure. */}
  process.stderr.write(process.argv[2]==='--stage-only'?code:JSON.stringify(diagnostic));process.exitCode=1;
 }
}
