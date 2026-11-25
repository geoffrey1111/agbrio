// Native, read-only ACP discovery. This module never starts or resumes a turn.
// Conversation writes must be added behind Host authorization and durable receipts.
import {spawn} from 'node:child_process';
import {isAbsolute} from 'node:path';

export class AcpDiscovery {
  constructor(child, {timeoutMs=15_000, maxBytes=1_048_576}={}) {
    this.child=child;this.timeoutMs=timeoutMs;this.maxBytes=maxBytes;
    this.pending=new Map();this.nextId=1;this.buffer=Buffer.alloc(0);this.closed=false;
    child.stdout.on('data',chunk=>this.consume(Buffer.from(chunk)));
    child.on('error',()=>this.fail('ANTIGRAVITY_ACP_START_FAILED'));
    child.on('exit',()=>this.fail('ANTIGRAVITY_ACP_DISCONNECTED'));
    // Provider diagnostics can contain private data; do not relay raw stderr.
    child.stderr?.resume();
  }
  fail(code) {
    this.closed=true;
    for(const item of this.pending.values()){clearTimeout(item.timer);item.reject(new Error(code));}
    this.pending.clear();this.buffer=Buffer.alloc(0);
  }
  consume(chunk) {
    if(this.closed)return;
    this.buffer=Buffer.concat([this.buffer,chunk]);
    let end;
    while((end=this.buffer.indexOf(10))>=0){
      if(end>this.maxBytes){this.fail('ANTIGRAVITY_ACP_FRAME_TOO_LARGE');return;}
      const line=this.buffer.subarray(0,end);this.buffer=this.buffer.subarray(end+1);
      if(!line.toString('utf8').trim())continue;
      let message;
      try{message=JSON.parse(line.toString('utf8'));}catch{this.fail('ANTIGRAVITY_ACP_INVALID_FRAME');return;}
      if(message.jsonrpc!=='2.0'){this.fail('ANTIGRAVITY_ACP_INVALID_FRAME');return;}
      if(message.method){
        // Discovery advertises no filesystem, terminal or permission capabilities.
        if(message.id!==undefined)this.child.stdin.write(JSON.stringify({jsonrpc:'2.0',id:message.id,error:{code:-32601,message:'Read-only discovery client'}})+'\n');
        continue;
      }
      const item=this.pending.get(message.id);
      if(!item)continue;
      this.pending.delete(message.id);clearTimeout(item.timer);
      if(message.error)item.reject(new Error(`ANTIGRAVITY_ACP_REMOTE_ERROR:${Number.isInteger(message.error.code)?message.error.code:'UNKNOWN'}`));
      else if(Object.hasOwn(message,'result'))item.resolve(message.result);
      else item.reject(new Error('ANTIGRAVITY_ACP_INVALID_RESPONSE'));
    }
    if(this.buffer.length>this.maxBytes)this.fail('ANTIGRAVITY_ACP_FRAME_TOO_LARGE');
  }
  request(method,params) {
    if(!['initialize','session/list'].includes(method))return Promise.reject(new Error('ANTIGRAVITY_DISCOVERY_READ_ONLY'));
    if(this.closed)return Promise.reject(new Error('ANTIGRAVITY_ACP_DISCONNECTED'));
    const id=this.nextId++;
    return new Promise((resolve,reject)=>{
      const timer=setTimeout(()=>{this.pending.delete(id);reject(new Error('ANTIGRAVITY_ACP_READ_TIMEOUT'));},this.timeoutMs);
      this.pending.set(id,{resolve,reject,timer});
      try{this.child.stdin.write(JSON.stringify({jsonrpc:'2.0',id,method,params})+'\n',error=>{if(error)this.fail('ANTIGRAVITY_ACP_DISCONNECTED');});}
      catch{this.fail('ANTIGRAVITY_ACP_DISCONNECTED');}
    });
  }
  async initialize() {
    const result=await this.request('initialize',{protocolVersion:1,clientCapabilities:{},clientInfo:{name:'agbrio',version:'0.1.0'}});
    if(result?.protocolVersion!==1)throw new Error('ANTIGRAVITY_ACP_PROTOCOL_UNSUPPORTED');
    this.capabilities=result.agentCapabilities??{};
    return result;
  }
  async sessions(cursor) {
    if(!this.capabilities?.sessionCapabilities?.list&&!this.capabilities?.session_capabilities?.list)throw new Error('ANTIGRAVITY_ACP_LIST_UNSUPPORTED');
    const result=await this.request('session/list',cursor?{cursor}:{});
    if(!Array.isArray(result?.sessions)||result.sessions.length>1000)throw new Error('ANTIGRAVITY_ACP_CATALOG_INVALID');
    for(const row of result.sessions)if(typeof row.sessionId!=='string'||!row.sessionId||typeof row.cwd!=='string')throw new Error('ANTIGRAVITY_ACP_IDENTITY_INVALID');
    return result;
  }
  close() {
    // Only this read-only child: closing stdin, never process-name killing.
    this.child.stdin.end();this.fail('ANTIGRAVITY_ACP_CLOSED');
  }
}

export function startDiscovery(executable,options={}) {
  if(!isAbsolute(executable)||!executable.toLowerCase().endsWith('.exe'))throw new Error('ANTIGRAVITY_CLI_EXACT_PATH_REQUIRED');
  // Signature/hash validation belongs to the owner-managed executable discovery.
  // Never download, authenticate, install, set PATH or relax permissions here.
  return new AcpDiscovery(spawn(executable,['acp','--stdio','--disable-telemetry'],{stdio:['pipe','pipe','pipe'],windowsHide:true,shell:false}),options);
}
