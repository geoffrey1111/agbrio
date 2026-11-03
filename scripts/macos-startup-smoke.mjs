// Isolated startup on an ephemeral runner; no provider/login/profile operation.
import {spawn} from 'node:child_process';
import {mkdtemp, mkdir, stat, rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import path from 'node:path';
import {setTimeout as delay} from 'node:timers/promises';

if(process.platform!=='darwin'||process.arch!=='arm64')throw Error('MACOS_ARM64_RUNNER_REQUIRED');
const app=path.resolve(process.argv[2]??'');
if(!app.endsWith('/Agbrio.app'))throw Error('EXACT_CANDIDATE_APP_REQUIRED');
const binary=path.join(app,'Contents/MacOS/ai-work-router');
if(!(await stat(binary)).isFile())throw Error('CANDIDATE_EXECUTABLE_MISSING');
const root=await mkdtemp(path.join(tmpdir(),'agbrio-macos-startup-'));
const home=path.join(root,'home');await mkdir(home);
const env={...process.env,HOME:home,CODEX_HOME:path.join(home,'.codex'),AI_WORK_ROUTER_CODEX_EXECUTABLE:path.join(root,'provider-disabled')};
for(const key of Object.keys(env))if(key==='USERPROFILE'||key==='LOCALAPPDATA'||key==='APPDATA'||key.startsWith('AGBRIO_')||key.startsWith('AI_WORK_ROUTER_MOBILE_'))delete env[key];
const child=spawn(binary,[],{env,stdio:['ignore','ignore','pipe']});
let started=false,exited=false,stderr='',failure;
child.once('spawn',()=>{started=true;});child.once('exit',()=>{exited=true;});
child.once('error',error=>{failure=error;});
child.stderr.on('data',bytes=>{if(stderr.length<65536)stderr+=String(bytes).slice(0,65536-stderr.length);});
try{
  await delay(8000);
  if(failure||!started||exited)throw Error('CANDIDATE_STARTUP_FAILED: '+(failure?.message??stderr));
  if(!(await stat(path.join(home,'Library/Application Support/AIWorkRouter/data/router.db'))).isFile())throw Error('ISOLATED_DATABASE_NOT_CREATED');
  console.log('AUTOMATED_VALIDATION_PASS: isolated startup/database; providers disabled; not feature parity or USER_UAT_PASS');
}finally{
  if(started&&!exited){child.kill('SIGTERM');for(let i=0;i<30&&!exited;i++)await delay(100);}
  if(started&&!exited){child.kill('SIGKILL');for(let i=0;i<30&&!exited;i++)await delay(100);}
  if(started&&!exited)throw Error('OWNED_CANDIDATE_EXIT_UNCONFIRMED');
  if(path.dirname(root)!==path.resolve(tmpdir())||!path.basename(root).startsWith('agbrio-macos-startup-'))throw Error('SMOKE_CLEANUP_BOUNDARY_INVALID');
  await rm(root,{recursive:true});
}
