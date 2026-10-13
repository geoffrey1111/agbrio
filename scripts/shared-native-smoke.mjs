// No threads, turns, model calls, credentials, Desktop or daemon operations.
import {spawn} from 'node:child_process';
import {mkdir,readdir,unlink,rmdir,lstat} from 'node:fs/promises';
import {join,dirname,resolve,relative} from 'node:path';
import {randomUUID} from 'node:crypto';
import {once} from 'node:events';
const binary=process.argv[2];if(!binary)throw Error('Pass an exact verified native binary path');
const scratch=resolve('runtime/shared-native-compatibility'),home=join(scratch,'smoke-'+randomUUID());await mkdir(home,{recursive:true});
const env={...process.env,CODEX_HOME:home};for(const name of Object.keys(env))if(/API_KEY|ACCESS_TOKEN|AUTH_TOKEN|CODEX_APP_SERVER_WS_URL/.test(name))delete env[name];
const child=spawn(binary,['-c','features.code_mode_host=true','app-server','--listen','stdio://'],{env,windowsHide:true,stdio:['pipe','pipe','pipe']});
const closed=once(child,'close'),pending=new Map();let buffer='',nextId=1,stderr='';
child.stderr.on('data',b=>{stderr=(stderr+b.toString()).slice(-8192);});
child.stdout.on('data',chunk=>{buffer+=chunk.toString();let at;while((at=buffer.indexOf('\n'))>=0){const line=buffer.slice(0,at);buffer=buffer.slice(at+1);let v;try{v=JSON.parse(line);}catch{continue;}const p=pending.get(v.id);if(p){clearTimeout(p.timer);pending.delete(v.id);v.error?p.reject(Error('RPC_'+v.error.code)):p.resolve(v.result);}else if(v.id!==undefined){child.stdin.write(JSON.stringify({id:v.id,error:{code:-32601,message:'Validation permits no approvals'}})+'\n');}}});
child.once('error',e=>{for(const p of pending.values()){clearTimeout(p.timer);p.reject(e);}pending.clear();});
function request(method,params){const id=nextId++;return new Promise((resolve,reject)=>{const timer=setTimeout(()=>{pending.delete(id);reject(Error('NATIVE_SMOKE_TIMEOUT'));},20000);pending.set(id,{resolve,reject,timer});child.stdin.write(JSON.stringify({id,method,params})+'\n');});}
async function clean(path){
 if(relative(scratch,path).startsWith('..'))throw Error('Cleanup outside scratch');
 for(const e of await readdir(path,{withFileTypes:true})){const p=join(path,e.name);if((await lstat(p)).isSymbolicLink())throw Error('Unexpected fixture link');if(e.isDirectory())await clean(p);else await unlink(p);}await rmdir(path);
}
try{
 await request('initialize',{clientInfo:{name:'agbrio_native_smoke',version:'0.1'},capabilities:{experimentalApi:true}});
 child.stdin.write(JSON.stringify({method:'initialized',params:{}})+'\n');
 const value=await request('command/exec',{command:[join(dirname(binary),'codex-code-mode-host.exe'),'--help'],cwd:home,sandboxPolicy:{type:'externalSandbox',networkAccess:'restricted'},timeoutMs:10000});
 if(value.exitCode!==0||!value.stdout.includes('codex-code-mode-host'))throw Error('NATIVE_TOOL_HOST_EXEC_FAILED');
 console.log(JSON.stringify({nativeInitialize:'PASS',toolHostViaCommandExec:'PASS',threadsCreated:0,modelRequests:0,desktopStarted:false,daemonOperations:0}));
}finally{
 for(const p of pending.values())clearTimeout(p.timer);
 child.stdin.end();const exited=await Promise.race([closed,new Promise(r=>setTimeout(()=>r(false),3000))]);if(exited===false&&child.exitCode===null){child.kill();await closed;}
 await clean(home);
}
