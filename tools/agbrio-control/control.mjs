#!/usr/bin/env node
// Owner-local macOS client. No network, shell or provider operations.
import net from 'node:net';
import {randomUUID} from 'node:crypto';
import {lstatSync,realpathSync,readFileSync} from 'node:fs';
import {dirname,resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
import {execFileSync,spawnSync} from 'node:child_process';

export function exchange(socket,command,{expectedInstance=null,requestId=randomUUID(),...fields}={}){
 return new Promise((done,fail)=>{
  const connection=net.createConnection(socket);let bytes=Buffer.alloc(0),reply,settled=false;
  const finish=(error)=>{if(settled)return;settled=true;connection.destroy();error?fail(Error(error)):done(reply.data);};
  const payload={apiVersion:1,requestId,command,expectedInstance,...fields};
  const request=Buffer.from(JSON.stringify(payload)+'\n');if(request.length>8193){connection.destroy();fail(Error('APP_CONTROL_REQUEST_TOO_LARGE'));return;}
  connection.setTimeout(5000,()=>finish('APP_CONTROL_RESPONSE_UNCONFIRMED'));
  connection.once('error',()=>finish('APP_CONTROL_UNREACHABLE'));
  connection.once('connect',()=>connection.write(request));
  connection.on('data',chunk=>{
   if(reply){finish('APP_CONTROL_RESPONSE_INVALID');return;}
   bytes=Buffer.concat([bytes,chunk]);if(bytes.length>131072){finish('APP_CONTROL_RESPONSE_TOO_LARGE');return;}
   const end=bytes.indexOf(10);if(end<0)return;
   if(end!==bytes.length-1){finish('APP_CONTROL_RESPONSE_INVALID');return;}
   try{reply=JSON.parse(bytes.subarray(0,end).toString('utf8'));}catch{finish('APP_CONTROL_RESPONSE_INVALID');return;}
   if(reply.errorCode){finish(/^APP_|^PUBLISHED_BOARD_|^REVIEW_BOARD_|^CLAUDE_/.test(reply.errorCode)?reply.errorCode:'APP_CONTROL_REQUEST_FAILED');return;}
   if(reply.apiVersion!==1||reply.requestId!==requestId||!reply.data){finish('APP_CONTROL_RESPONSE_INVALID');return;}
   connection.write('ACK\n');
  });
  connection.once('end',()=>finish(reply?null:'APP_CONTROL_RESPONSE_UNCONFIRMED'));
 });
}
function endpoint(){
 const user=process.geteuid();const directory=`/private/tmp/agbrio-control-v1-${user}`,socket=`${directory}/control.sock`;
 const folder=lstatSync(directory),file=lstatSync(socket);
 if(!folder.isDirectory()||folder.isSymbolicLink()||folder.uid!==user||(folder.mode&0o777)!==0o700||!file.isSocket()||file.uid!==user||(file.mode&0o777)!==0o600)throw Error('APP_CONTROL_ENDPOINT_UNTRUSTED');
 return socket;
}
function processIdentity(pid,executable){
 if(!Number.isSafeInteger(pid)||pid<1)throw Error('APP_CONTROL_PROCESS_INVALID');
 const helper=resolve(dirname(dirname(executable)),'Resources/macos-platform/mac-resource-sampler');
 let actual;try{actual=JSON.parse(execFileSync(helper,['identity',String(pid)],{encoding:'utf8',timeout:3000,env:{PATH:'/usr/bin:/bin',HOME:process.env.HOME||''}}));}catch{throw Error('APP_CONTROL_IDENTITY_UNCONFIRMED');}
 if(actual.state==='EXITED')return null;
 if(actual.pid!==pid||actual.ownerUid!==process.geteuid()||actual.image!==executable||typeof actual.created!=='string')throw Error('APP_CONTROL_EXECUTABLE_MISMATCH');
 return {pid,started:actual.created,executable};
}
const delay=ms=>new Promise(r=>setTimeout(r,ms));
async function start(app,executable){
 // An unavailable socket does not prove the app is absent. Inspect same-user
 // executable identities before asking LaunchServices to start anything.
 const rows=execFileSync('/bin/ps',['-axo','pid=,uid=,comm='],{encoding:'utf8',timeout:3000}).split('\n');
 for(const row of rows){const match=row.match(/^\s*(\d+)\s+(\d+)\s+(.+)$/);if(!match||Number(match[2])!==process.geteuid()||match[3]!==executable)continue;const state=await exchange(endpoint(),'STATUS');if(state.pid!==Number(match[1])||!processIdentity(state.pid,executable))throw Error('APP_CONTROL_EXECUTABLE_MISMATCH');return state;}
 // Preserve only the existing non-secret home/native-CLI process overrides.
 // LaunchServices otherwise may discard the caller's isolated/native context.
 const context=['HOME','CODEX_HOME','AI_WORK_ROUTER_CODEX_EXECUTABLE'].flatMap(key=>process.env[key]===undefined?[]:['--env',`${key}=${process.env[key]}`]);
 const result=spawnSync('/usr/bin/open',['-a',app,...context],{timeout:5000,stdio:'ignore'});if(result.status!==0)throw Error('APP_CONTROL_START_UNCONFIRMED');
 for(let n=0;n<120;n++){try{const state=await exchange(endpoint(),'STATUS');if(processIdentity(state.pid,executable))return state;}catch(error){if(error.message==='APP_CONTROL_EXECUTABLE_MISMATCH')throw error;}await delay(250);}
 throw Error('APP_CONTROL_START_UNCONFIRMED');
}
async function stop(executable,expectedInstance,requestId){
 if(!/^[0-9a-f]{8}-[0-9a-f-]{27}$/.test(expectedInstance||''))throw Error('APP_CONTROL_EXPECTED_INSTANCE_REQUIRED');
 const socket=endpoint(),state=await exchange(socket,'STATUS');if(state.instanceId!==expectedInstance)throw Error('APP_CONTROL_INSTANCE_CHANGED');
 const identity=processIdentity(state.pid,executable);if(!identity)throw Error('APP_CONTROL_PROCESS_UNAVAILABLE');
 let receiptUncertain=false;try{await exchange(socket,'SHUTDOWN',{expectedInstance,requestId});}catch{receiptUncertain=true;}
 for(let n=0;n<120;n++){
  let current;try{current=processIdentity(state.pid,executable);}catch(error){if(error.message==='APP_CONTROL_EXECUTABLE_MISMATCH')return {state:'STOPPED',instanceId:expectedInstance,pid:state.pid,receiptUncertain};throw error;}
  if(!current||current.started!==identity.started)return {state:'STOPPED',instanceId:expectedInstance,pid:state.pid,receiptUncertain};await delay(250);
 }
 throw Error('APP_CONTROL_SHUTDOWN_UNCONFIRMED');
}
export async function main(argv){
 if(process.platform!=='darwin')throw Error('APP_CONTROL_PLATFORM_UNSUPPORTED');
 const command=argv.shift()||'Status',options={};while(argv.length){const key=argv.shift();if(!['--app','--expected-instance','--request-id','--folder','--title','--review-port','--notice-binding-file'].includes(key)||!argv.length||options[key])throw Error('APP_CONTROL_ARGUMENT_INVALID');options[key]=argv.shift();}
 if(!['Status','Capabilities','Diagnostics','Start','Stop','Restart','PublishBoard','PublishReviewBoard','RegisterCodexNotices'].includes(command))throw Error('APP_CONTROL_COMMAND_UNSUPPORTED');
 const app=realpathSync(options['--app']||resolve(dirname(fileURLToPath(import.meta.url)),'../../..'));
 const binary=resolve(app,'Contents/MacOS/ai-work-router');
 if(!app.endsWith('/Agbrio.app')||lstatSync(binary).isSymbolicLink()||!lstatSync(binary).isFile())throw Error('APP_CONTROL_EXECUTABLE_INVALID');
 const executable=realpathSync(binary);
 const requestId=options['--request-id']||randomUUID(),expectedInstance=options['--expected-instance']||null;
 if(command==='Start')return start(app,executable);
 if(command==='Stop')return stop(executable,expectedInstance,requestId);
 if(command==='Restart'){await stop(executable,expectedInstance,requestId);return start(app,executable);}
 const reads={Status:'STATUS',Capabilities:'CAPABILITIES',Diagnostics:'DIAGNOSTICS'};if(reads[command])return exchange(endpoint(),reads[command],{expectedInstance,requestId});
 const socket=endpoint(),state=await exchange(socket,'STATUS');if(!processIdentity(state.pid,executable))throw Error('APP_CONTROL_PROCESS_UNAVAILABLE');
 const payload={expectedInstance:state.instanceId,requestId};
 if(['PublishBoard','PublishReviewBoard'].includes(command)){
  if(!options['--folder'])throw Error('PUBLISHED_BOARD_FOLDER_REQUIRED');payload.folder=options['--folder'];
  if(command==='PublishReviewBoard'){payload.reviewPort=Number(options['--review-port']||47816);payload.title=options['--title']||'互动审批看板';}
  return exchange(socket,command==='PublishBoard'?'PUBLISH_BOARD':'PUBLISH_REVIEW_BOARD',payload);
 }
 if(command==='RegisterCodexNotices'){payload.noticeBinding=JSON.parse(readFileSync(options['--notice-binding-file'],'utf8'));return exchange(socket,'REGISTER_CODEX_NOTICES',payload);}
 throw Error('APP_CONTROL_COMMAND_UNSUPPORTED');
}
if(process.argv[1]&&pathToFileURL(resolve(process.argv[1])).href===import.meta.url){
 try{console.log(JSON.stringify(await main(process.argv.slice(2))));}catch(error){const errorCode=/^(APP_|PUBLISHED_BOARD_|REVIEW_BOARD_|CLAUDE_)[A-Z_]+$/.test(error.message)?error.message:'APP_CONTROL_UNREACHABLE';console.log(JSON.stringify({errorCode,state:'UNCONFIRMED',automaticWriteRetry:false}));process.exitCode=1;}
}
