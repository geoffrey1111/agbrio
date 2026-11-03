// Exact owned-process metadata, normal close/focus and a kernel profile guard.
// No browser automation imports, argv/environment reads or account operations.
import {spawn} from 'node:child_process';
import {readFile,writeFile,open,lstat,readdir,realpath,chmod} from 'node:fs/promises';
import {createReadStream} from 'node:fs';
import {createHash,randomUUID} from 'node:crypto';
import {homedir,tmpdir} from 'node:os';
import path from 'node:path';
const helper=path.join(import.meta.dirname,'macos-native-helper');
const pendingOwners=new Map();
const hash='b0170371b54da26fc49bac55533df471ec55a937bc0c6f778722016ba31cc480';
function environment(){return{PATH:'/usr/bin:/bin',HOME:homedir(),TMPDIR:process.env.TMPDIR||tmpdir(),LANG:process.env.LANG||'en_US.UTF-8'};}
function code(value){return /^(SECURITY_|PROFILE_)[A-Z_]+$/.test(value??'')?value:'SECURITY_NATIVE_UNAVAILABLE';}
function child(request){
 if(process.platform!=='darwin')throw Error('SECURITY_RECOVERY_MACOS_ONLY');
 const processHandle=spawn(helper,[],{stdio:['pipe','pipe','ignore'],env:environment()});
 processHandle.stdin.on('error',()=>{});processHandle.stdin.write(JSON.stringify(request)+'\n');return processHandle;
}
async function request(payload){
 const processHandle=child(payload);processHandle.stdin.end();
 return new Promise((done,fail)=>{
  let result='',settled=false;const finish=(error,value)=>{if(settled)return;settled=true;clearTimeout(timer);error?fail(Error(code(error))):done(value);};
  const timer=setTimeout(()=>{processHandle.kill();finish('SECURITY_NATIVE_UNCONFIRMED');},10000);
  processHandle.once('error',()=>finish('SECURITY_NATIVE_UNAVAILABLE'));
  processHandle.stdout.on('data',chunk=>{result+=chunk;if(result.length>131072){processHandle.kill();finish('SECURITY_NATIVE_UNAVAILABLE');}});
  processHandle.once('close',exit=>{let value;try{value=JSON.parse(result);}catch{return finish('SECURITY_NATIVE_UNAVAILABLE');}if(exit!==0||value.errorCode)return finish(value.errorCode);finish(null,value);});
 });
}
async function marker(file){
 const info=await lstat(file);if(!info.isFile()||info.isSymbolicLink()||info.uid!==process.geteuid()||(info.mode&0o077)||info.size>8192||info.nlink!==1)throw Error('PROFILE_OWNERSHIP_UNPROVEN');
 return JSON.parse(await readFile(file,'utf8'));
}
async function ensureOwnedProfile(data,profile){
 if(await realpath(profile)!==profile)throw Error('PROFILE_OWNERSHIP_UNPROVEN');
 const owner=path.join(profile,'aiwr-profile-owner.json'),location=path.join(data,'owned-profile-location.json');let record;
 try{record=await marker(owner);}catch(error){
  if(error.code!=='ENOENT')throw error;
  if((await readdir(profile)).length)throw Error('PROFILE_OWNERSHIP_UNPROVEN');
  record={schema:'AIWR_OWNED_CHROMIUM_PROFILE_V1',ownerId:randomUUID(),profile};
  await chmod(profile,0o700);await writeFile(owner,JSON.stringify(record),{flag:'wx',mode:0o600});
 }
 if(record.schema!=='AIWR_OWNED_CHROMIUM_PROFILE_V1'||!/^[a-f0-9-]{36}$/.test(record.ownerId??''))throw Error('PROFILE_OWNERSHIP_UNPROVEN');
 try{const saved=await marker(location);if(saved.ownerId!==record.ownerId||saved.profile!==profile||saved.schema!==record.schema)throw Error('PROFILE_OWNERSHIP_UNPROVEN');}
 catch(error){if(error.code!=='ENOENT')throw error;await writeFile(location,JSON.stringify({...record,profile}),{flag:'wx',mode:0o600});}
}
async function lease(profile){try{return await marker(path.join(profile,'aiwr-browser-session.json'));}catch(error){if(error.code==='ENOENT')return null;throw error;}}
async function inventory(profile){
 const actual=await request({operation:'inventory',profile});const saved=await lease(profile),pending=pendingOwners.get(profile);
 return actual.map(item=>{
  const known=pending?.pid===item.pid&&pending.binary===item.binary&&(!pending.started||pending.started===item.started)?pending:
   saved?.schema==='AIWR_BROWSER_SESSION_V1'&&saved.profile===profile&&saved.pid===item.pid&&saved.binary===item.binary&&saved.started===item.started?{automated:saved.mode==='AUTOMATED'}:null;
  if(known===pending)pending.started=item.started;
  // Unknown native occupants always block acquisition and can never be adopted
  // as a human session. Known launch argv is owned by this module/runtime; no
  // kernel API that also returns environment/credentials is used.
  return {...item,profile,automated:known?known.automated:true,ownershipUnproven:!known};
 });
}
async function withExclusiveProfile(profile,action){
 const guard=child({operation:'guard',profile});let held=false,lost=false;guard.once('close',()=>{lost=true;});
 try{
  await new Promise((done,fail)=>{
   let output='';const timer=setTimeout(()=>fail(Error('SECURITY_RECOVERY_GUARD_UNAVAILABLE')),10000);
   const finish=error=>{clearTimeout(timer);error?fail(error):done();};
   guard.once('error',()=>finish(Error('SECURITY_RECOVERY_GUARD_UNAVAILABLE')));guard.once('close',()=>{if(!held)finish(Error('SECURITY_RECOVERY_GUARD_UNAVAILABLE'));});
   guard.stdout.on('data',chunk=>{output+=chunk;if(output.includes('ACQUIRED')){held=true;finish();}else if(output.includes('BUSY'))finish(Error('SECURITY_RECOVERY_BUSY'));else if(output.length>32768||output.includes('errorCode'))finish(Error('SECURITY_RECOVERY_GUARD_UNAVAILABLE'));});
  });
  const assertHeld=()=>{if(lost||!held)throw Error('SECURITY_RECOVERY_GUARD_UNAVAILABLE');};assertHeld();return await action(assertHeld);
 }finally{
  const ended=new Promise(done=>{if(guard.exitCode!==null||guard.signalCode!==null||!guard.pid)return done();const timer=setTimeout(()=>guard.kill(),5000);guard.once('close',()=>{clearTimeout(timer);done();});});
  guard.stdin.end('\n');if(!held)guard.kill();await ended;
 }
}
export const macosSecurityPlatform={
 ensureOwnedProfile,inventory,withExclusiveProfile,
 async rememberOwnedLaunch(profile,pid,binary,automated){
  pendingOwners.set(profile,{pid,binary:await realpath(binary),automated});
  const owners=await inventory(profile),owner=owners.length===1?owners[0]:null;
  if(!owner||owner.pid!==pid||owner.ownershipUnproven)throw Error('SECURITY_BROWSER_IDENTITY_CHANGED');return owner;
 },
 async launch(binary,args){
  if(!binary.endsWith('/Chromium.app/Contents/MacOS/Chromium'))throw Error('SECURITY_BROWSER_IDENTITY_CHANGED');
  const digest=createHash('sha256');for await(const part of createReadStream(binary))digest.update(part);if(digest.digest('hex')!==hash)throw Error('SECURITY_BROWSER_IDENTITY_CHANGED');
  const profile=args[0]?.startsWith('--user-data-dir=')?args[0].slice('--user-data-dir='.length):null;
  if(!profile||JSON.stringify(args.slice(1))!==JSON.stringify(['--disable-extensions','--no-first-run','--no-default-browser-check','--restore-last-session']))throw Error('SECURITY_HUMAN_ARGUMENTS_INVALID');
  const processHandle=spawn(binary,args,{detached:true,stdio:'ignore',env:environment()});
  await new Promise((done,fail)=>{processHandle.once('spawn',done);processHandle.once('error',fail);});processHandle.unref();pendingOwners.set(profile,{pid:processHandle.pid,binary:await realpath(binary),automated:false});return processHandle.pid;
 },
 async close(record){await request({operation:'close',profile:record.profile,pid:record.pid,binary:record.binary,started:record.started});},
 async focus(record){const result=await request({operation:'focus',profile:record.profile,pid:record.pid,binary:record.binary,started:record.started});if(!['FOREGROUND','ATTENTION'].includes(result.status))throw Error('SECURITY_BROWSER_FOCUS_FAILED');return result.status;}
};
