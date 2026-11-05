// Exact delivered app, isolated HOME, no account/provider prompt or browser.
import {spawn,execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {mkdtemp,mkdir,rm,stat,rename,writeFile,readFile} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import path from 'node:path';
import {setTimeout as delay} from 'node:timers/promises';
const execute=promisify(execFile);
if(process.platform!=='darwin'||process.arch!=='arm64')throw Error('MACOS_ARM64_REQUIRED');
const app=path.resolve(process.argv[2]??'');if(!app.endsWith('/Agbrio.app'))throw Error('EXACT_CANDIDATE_APP_REQUIRED');
const binary=path.join(app,'Contents/MacOS/ai-work-router'),node=path.join(app,'Contents/MacOS/browser-executor-node');
const cli=path.join(app,'Contents/Resources/app-control/control.mjs'),helper=path.join(app,'Contents/Resources/macos-platform/mac-resource-sampler');
const root=await mkdtemp(path.join(tmpdir(),'agbrio-macos-cli-')),home=path.join(root,'home');await mkdir(home);
const env={...process.env,HOME:home,CODEX_HOME:path.join(home,'.codex'),AI_WORK_ROUTER_CODEX_EXECUTABLE:path.join(root,'provider-disabled')};
for(const key of Object.keys(env))if(key==='USERPROFILE'||key==='LOCALAPPDATA'||key==='APPDATA'||key.startsWith('AGBRIO_')||key.startsWith('AI_WORK_ROUTER_MOBILE_'))delete env[key];
const owned=new Map();let initial,finished=false;
async function identity(pid){const{stdout}=await execute(helper,['identity',String(pid)],{env,timeout:5000});return JSON.parse(stdout);}
async function command(operation,...args){const{stdout}=await execute(node,[cli,operation,'--app',app,...args],{env,timeout:45000,maxBuffer:131072});const result=JSON.parse(stdout);if(result.errorCode)throw Error(result.errorCode);return result;}
async function remember(state){const proof=await identity(state.pid);if(proof.image!==binary||proof.ownerUid!==process.geteuid()||!proof.created)throw Error('CANDIDATE_IDENTITY_MISMATCH');owned.set(state.pid,proof);return state;}
try{
 initial=spawn(binary,[],{env,stdio:'ignore'});initial.once('error',()=>{});
 let first;for(let i=0;i<100;i++){try{first=await command('Status');break;}catch{}await delay(100);}
 if(!first||first.pid!==initial.pid)throw Error('ISOLATED_CANDIDATE_STARTUP_UNCONFIRMED');await remember(first);
 const diagnostic=await command('Diagnostics');if(diagnostic.providerProbePerformed!==false)throw Error('CLI_DIAGNOSTICS_PROVIDER_PROBE');
 await command('Stop','--expected-instance',first.instanceId);
 const data=path.join(home,'Library/Application Support/AIWorkRouter');await rename(data,path.join(root,'first-start-data'));
 const second=await remember(await command('Start'));if(second.instanceId===first.instanceId)throw Error('CLI_START_DID_NOT_CREATE_NEW_EPOCH');
 // LaunchServices must keep the selected home/disabled-provider context.
 if(!(await stat(path.join(home,'Library/Application Support/AIWorkRouter/data/router.db'))).isFile())throw Error('CLI_HOME_CONTEXT_LOST');
 await writeFile(path.join(data,'fixture-preserve.txt'),'owned lifecycle preservation fixture');
 const third=await remember(await command('Restart','--expected-instance',second.instanceId));if(third.instanceId===second.instanceId)throw Error('CLI_RESTART_DID_NOT_CREATE_NEW_EPOCH');
 if(await readFile(path.join(data,'fixture-preserve.txt'),'utf8')!=='owned lifecycle preservation fixture')throw Error('CLI_RESTART_DATA_NOT_PRESERVED');
 let stale=false;try{await command('Stop','--expected-instance',first.instanceId);}catch(error){stale=String(error.stdout??error).includes('APP_CONTROL_INSTANCE_CHANGED');}if(!stale)throw Error('CLI_STALE_EPOCH_NOT_REJECTED');
 if((await command('Status')).instanceId!==third.instanceId)throw Error('CLI_STALE_STOP_CHANGED_SUCCESSOR');
 await command('Stop','--expected-instance',third.instanceId);
 for(const pid of owned.keys()){if((await identity(pid)).state!=='EXITED')throw Error('CLI_OWNED_EXIT_UNCONFIRMED');}
 finished=true;console.log('AUTOMATED_VALIDATION_PASS: delivered Mac CLI status/diagnostics/start/stop/restart/stale-epoch; isolated home; no account/model/provider operation');
}finally{
 // Failure cleanup only for exact task-created identities; never retry a lost
 // application command or signal a reused PID/another candidate instance.
 for(const[pid,before]of owned){const current=await identity(pid).catch(()=>null);if(current?.state==='EXITED')continue;if(current?.image===before.image&&current.created===before.created&&current.ownerUid===before.ownerUid){process.kill(pid,'SIGTERM');for(let i=0;i<30;i++){if((await identity(pid)).state==='EXITED')break;await delay(100);}}}
 let allGone=true;for(const[pid]of owned)if((await identity(pid).catch(()=>null))?.state!=='EXITED')allGone=false;
 if(initial?.pid&&!owned.has(initial.pid)&&initial.exitCode===null){initial.kill('SIGTERM');await delay(500);if(initial.exitCode===null)allGone=false;}
 if(allGone&&path.dirname(root)===path.resolve(tmpdir())&&path.basename(root).startsWith('agbrio-macos-cli-'))await rm(root,{recursive:true});
 else if(!finished)throw Error('CLI_FIXTURE_CLEANUP_UNCONFIRMED');
}
