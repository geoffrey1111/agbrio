import {test} from 'node:test';
import assert from 'node:assert/strict';
import {EventEmitter} from 'node:events';
import {PassThrough,Writable} from 'node:stream';
import {probeCodeMode,encodeFrame} from '../src-tauri/resources/codex-shared/code-mode-probe.mjs';
function host({fragment=0,wrongMarker=false,wrongCell=false,delegate=false,fixtureDelegate=false,wrongDelegate=false,oversize=false,invalidObject=false,silent=false}={}){
 const child=new EventEmitter();child.stdout=new PassThrough();child.stderr=new PassThrough();let input=Buffer.alloc(0),marker='',ended=false,kills=0;const requests=[];
 let deferred=null;const close=()=>{if(ended)return;ended=true;child.stdout.end();child.stderr.end();child.emit('close',0);};child.kill=()=>{++kills;close();};
 const emit=value=>{const frame=encodeFrame(value);if(fragment){for(let i=0;i<frame.length;i+=fragment)child.stdout.write(frame.subarray(i,i+fragment));}else child.stdout.write(frame);};
 child.stdin=new Writable({write(chunk,_encoding,done){input=Buffer.concat([input,chunk]);while(input.length>=4&&input.length>=input.readUInt32LE(0)+4){const n=input.readUInt32LE(0);const message=JSON.parse(input.subarray(4,n+4));input=input.subarray(n+4);requests.push(message);queueMicrotask(()=>{
  if(silent)return;
  if(oversize){const header=Buffer.alloc(4);header.writeUInt32LE(2*1024*1024);child.stdout.write(header);return;}
  if(invalidObject){emit(null);return;}
  if(message.type==='connection/hello'){emit({type:'connection/ready',selectedVersion:1,capabilities:[]});return;}
  if(message.type==='delegate/response'){
   assert.equal(message.result.value.result,marker);assert.ok(deferred);const {id,cellId}=deferred;deferred=null;
   emit({type:'execute/initialResponse',id,result:{status:'ok',value:{Result:{cell_id:cellId,content_items:[{type:'input_text',text:marker}],error_text:null}}}});return;
  }
  const {request,id}=message;
  if(request.method==='session/execute'){
   marker=request.request.source.match(/AGBRIO_NATIVE_EVAL_[a-f0-9-]+/)?.[0]??marker;
   if(fixtureDelegate&&request.request.enabled_tools.length){const cellId='cell-'+id;deferred={id,cellId};emit({type:'operation/response',id,result:{status:'ok',value:{type:'execution/started',cellId}}});emit({type:'delegate/request',id:90,sessionId:request.sessionId,request:{type:'tool/invoke',invocation:{tool_name:{name:'echo',namespace:wrongDelegate?'unrelated':'agbrio_probe'},tool_kind:'function',input:{nonce:marker}}}});return;}
   assert.deepEqual(request.request.enabled_tools,[]);
   if(delegate){emit({type:'delegate/request',id:9,request:{type:'tool/invoke'}});return;}
   const cellId='cell-'+id;
   // Initial may race the execution acknowledgement; the client must retain it.
   emit({type:'execute/initialResponse',id,result:{status:'ok',value:{Result:{cell_id:wrongCell?'other-cell':cellId,content_items:[{type:'input_text',text:wrongMarker?'STALE':marker}],error_text:null,code_mode_host_duration_ns:100}}}});
   emit({type:'operation/response',id,result:{status:'ok',value:{type:'execution/started',cellId}}});return;
  }
  emit({type:'operation/response',id,result:{status:'ok',value:{type:request.method==='session/open'?'session/ready':'session/closed',sessionId:request.sessionId}}});
 });}done();},final(done){close();done();}});
 return {spawnImpl:()=>child,requests,get ended(){return ended;},get kills(){return kills;}};
}
test('fragmented official frames, out-of-order acknowledgement and two nonce cells complete without delegates',async()=>{
 const f=host({fragment:1});const value=await probeCodeMode('fixture-host',{spawnImpl:f.spawnImpl});
 assert.equal(value.javascriptCellsExecuted,2);assert.equal(value.persistentStoreVerified,true);assert.equal(f.ended,true);
 assert.equal(f.requests.filter(r=>r.request?.method==='session/execute').length,2);assert.equal(f.requests.at(-1).request.method,'session/shutdown');
});
test('optional callback fixture uses exactly one nonce-only delegate and shuts down',async()=>{
 const f=host({fixtureDelegate:true});const v=await probeCodeMode('fixture-host',{spawnImpl:f.spawnImpl,fixtureDelegate:true});assert.equal(v.javascriptCellsExecuted,3);assert.equal(v.delegateCalls,1);assert.equal(f.ended,true);
});
test('callback fixture rejects another namespace instead of dispatching it',async()=>{
 const f=host({fixtureDelegate:true,wrongDelegate:true});await assert.rejects(probeCodeMode('fixture-host',{spawnImpl:f.spawnImpl,fixtureDelegate:true}),/SHARED_NATIVE_PROBE_UNEXPECTED_DELEGATE/);assert.equal(f.requests.some(v=>v.type==='delegate/response'),false);assert.equal(f.ended,true);
});
for(const [name,options,category] of [
 ['stale output',{wrongMarker:true},'SHARED_NATIVE_TOOL_HOST_EXEC_FAILED'],
 ['wrong cell identity',{wrongCell:true},'SHARED_NATIVE_TOOL_HOST_EXEC_FAILED'],
 ['unexpected nested tool',{delegate:true},'SHARED_NATIVE_PROBE_UNEXPECTED_DELEGATE'],
 ['oversized declared response',{oversize:true},'SHARED_NATIVE_PROBE_FRAME_TOO_LARGE'],
 ['non-object JSON',{invalidObject:true},'SHARED_NATIVE_PROTOCOL_MISMATCH'],
 ['unresponsive host',{silent:true},'SHARED_NATIVE_TOOL_HOST_TIMEOUT']
])test('probe rejects '+name+' and closes its own child',async()=>{const f=host(options);await assert.rejects(probeCodeMode('fixture-host',{spawnImpl:f.spawnImpl,timeoutMs:30}),new RegExp(category));assert.equal(f.ended,true);});
