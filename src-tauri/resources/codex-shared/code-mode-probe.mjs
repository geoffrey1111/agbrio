// Official code-mode V1 framed stdio protocol. No Codex threads, model, daemon
// or filesystem tools. Default: two cells, no delegates. Optional fixture mode
// echoes only the validation nonce through one tightly checked local callback.
import {spawn} from 'node:child_process';
import {randomUUID} from 'node:crypto';
import {resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
const MAX_FRAME=1024*1024;
export function encodeFrame(value){const body=Buffer.from(JSON.stringify(value));if(body.length>MAX_FRAME)throw Error('SHARED_NATIVE_PROBE_FRAME_TOO_LARGE');const header=Buffer.alloc(4);header.writeUInt32LE(body.length);return Buffer.concat([header,body]);}
export async function probeCodeMode(binary,{spawnImpl=spawn,timeoutMs=10000,fixtureDelegate=false}={}){
 const child=spawnImpl(binary,['--listen','stdio'],{windowsHide:true,stdio:['pipe','pipe','pipe']});
 let buffer=Buffer.alloc(0),fatal=null,closed=false,delegateCalls=0;const queue=[],waiters=[],delegates=new Set();
 const sessionId='agbrio-probe-'+randomUUID(),marker='AGBRIO_NATIVE_EVAL_'+randomUUID();let id=0;
 const fail=code=>{if(fatal)return;fatal=Error(code);for(const w of waiters.splice(0)){clearTimeout(w.timer);w.reject(fatal);}};
 const exited=new Promise(resolve=>{child.once('close',()=>{closed=true;resolve();});});
 child.once('error',()=>fail('SHARED_NATIVE_TOOL_HOST_FAILED'));
 child.once('close',()=>fail('SHARED_NATIVE_TOOL_HOST_CLOSED'));
 child.stderr.resume();child.stdin.on('error',()=>fail('SHARED_NATIVE_TOOL_HOST_FAILED'));
 child.stdout.on('data',chunk=>{
  if(fatal)return;buffer=Buffer.concat([buffer,chunk]);
  while(buffer.length>=4){
   const size=buffer.readUInt32LE(0);if(!size||size>MAX_FRAME){fail('SHARED_NATIVE_PROBE_FRAME_TOO_LARGE');return;}
   if(buffer.length<size+4)return;
   let v;try{v=JSON.parse(buffer.subarray(4,size+4).toString());}catch{fail('SHARED_NATIVE_PROTOCOL_MISMATCH');return;}
   buffer=buffer.subarray(size+4);
   if(!v||typeof v!=='object'||Array.isArray(v)||typeof v.type!=='string'){fail('SHARED_NATIVE_PROTOCOL_MISMATCH');return;}
   if(v.type==='delegate/request'){
    const call=v.request?.invocation;
    if(!fixtureDelegate||v.sessionId!==sessionId||v.request?.type!=='tool/invoke'||call?.tool_name?.name!=='echo'||call?.tool_name?.namespace!=='agbrio_probe'||call?.tool_kind!=='function'||call?.input?.nonce!==marker||Object.keys(call.input).length!==1||delegates.has(v.id)||delegateCalls!==0){fail('SHARED_NATIVE_PROBE_UNEXPECTED_DELEGATE');return;}
    delegates.add(v.id);++delegateCalls;
    child.stdin.write(encodeFrame({type:'delegate/response',id:v.id,result:{status:'ok',value:{type:'tool/result',result:marker}}}));continue;
   }
   if(v.type==='connection/rejected'){fail('SHARED_NATIVE_PROTOCOL_MISMATCH');return;}
   const at=waiters.findIndex(w=>w.type===v.type&&(w.id===undefined||w.id===v.id));
   if(at>=0){const w=waiters.splice(at,1)[0];clearTimeout(w.timer);w.resolve(v);}else if(v.type!=='cell/closed')queue.push(v);
   if(queue.length>32){fail('SHARED_NATIVE_PROTOCOL_MISMATCH');return;}
  }
 });
 const receive=(type,id)=>{if(fatal)return Promise.reject(fatal);const at=queue.findIndex(v=>v.type===type&&(id===undefined||v.id===id));if(at>=0)return Promise.resolve(queue.splice(at,1)[0]);return new Promise((resolve,reject)=>{const w={type,id,resolve,reject,timer:null};w.timer=setTimeout(()=>{waiters.splice(waiters.indexOf(w),1);reject(Error('SHARED_NATIVE_TOOL_HOST_TIMEOUT'));},timeoutMs);waiters.push(w);});};
 const send=v=>{if(fatal)throw fatal;child.stdin.write(encodeFrame(v));};
 const unwrap=v=>{if(v.result?.status!=='ok')throw Error('SHARED_NATIVE_TOOL_HOST_EXEC_FAILED');return v.result.value;};
 const request=async request=>{const current=++id;send({type:'operation/request',id:current,request});return unwrap(await receive('operation/response',current));};
 async function execute(source,tools=[]){
  const current=++id;send({type:'operation/request',id:current,request:{method:'session/execute',sessionId,request:{tool_call_id:'probe-'+current,enabled_tools:tools,source,yield_time_ms:1000,max_output_tokens:128}}});
  const started=unwrap(await receive('operation/response',current));if(started.type!=='execution/started')throw Error('SHARED_NATIVE_PROTOCOL_MISMATCH');
  const initial=unwrap(await receive('execute/initialResponse',current));const value=initial.Result;
  if(!value||value.cell_id!==started.cellId||value.error_text||!value.content_items?.some(item=>item.type==='input_text'&&item.text===marker))throw Error('SHARED_NATIVE_TOOL_HOST_EXEC_FAILED');
 }
 try{
  send({type:'connection/hello',supportedVersions:[1],requiredCapabilities:[],optionalCapabilities:[]});
  const hello=await receive('connection/ready');if(hello.selectedVersion!==1)throw Error('SHARED_NATIVE_PROTOCOL_MISMATCH');
  const opened=await request({method:'session/open',sessionId});if(opened.type!=='session/ready'||opened.sessionId!==sessionId)throw Error('SHARED_NATIVE_PROTOCOL_MISMATCH');
  await execute('store("agbrio_probe",'+JSON.stringify(marker)+');text(load("agbrio_probe"));');
  await execute('text(load("agbrio_probe"));');
  if(fixtureDelegate){
   const tools=[{name:'agbrio_probe_echo',tool_name:{name:'echo',namespace:'agbrio_probe'},description:'Return only this isolated validation nonce; no external I/O.',kind:'function',input_schema:{type:'object',properties:{nonce:{type:'string'}},required:['nonce'],additionalProperties:false},output_schema:null}];
   await execute('const echo=await tools.agbrio_probe_echo({nonce:'+JSON.stringify(marker)+'});text(echo);',tools);
   if(delegateCalls!==1)throw Error('SHARED_NATIVE_TOOL_HOST_EXEC_FAILED');
  }
  const stopped=await request({method:'session/shutdown',sessionId});if(stopped.type!=='session/closed'||stopped.sessionId!==sessionId)throw Error('SHARED_NATIVE_PROTOCOL_MISMATCH');
  return {protocolVersion:1,javascriptCellsExecuted:fixtureDelegate?3:2,persistentStoreVerified:true,delegateCalls,modelRequests:0};
 }finally{
  for(const w of waiters.splice(0))clearTimeout(w.timer);
  child.stdin.end();let timer;await Promise.race([exited,new Promise(r=>{timer=setTimeout(r,1500);})]);clearTimeout(timer);
  if(!closed){child.kill();await exited;}
 }
}
if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url)){
 try{if(!process.argv[2])throw Error('SHARED_NATIVE_BINARY_UNVERIFIED');console.log(JSON.stringify(await probeCodeMode(process.argv[2],{fixtureDelegate:process.argv.includes('--fixture-delegate')})));}
 catch(e){process.stderr.write(e.message.startsWith('SHARED_')?e.message:'SHARED_NATIVE_TOOL_HOST_FAILED');process.exitCode=1;}
}
