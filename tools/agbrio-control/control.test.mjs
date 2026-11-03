import {test} from 'node:test';
import assert from 'node:assert/strict';
import net from 'node:net';
import {randomUUID} from 'node:crypto';
import {mkdtempSync,rmSync} from 'node:fs';
import {exchange} from './control.mjs';

async function fixture(t,handle){
 const directory=process.platform==='win32'?null:mkdtempSync('/tmp/agbrio-cli-');
 const path=directory?`${directory}/control.sock`:`\\\\.\\pipe\\agbrio-cli-test-${randomUUID()}`;
 const server=net.createServer(handle);await new Promise((ok,fail)=>server.listen(path,ok).once('error',fail));
 t.after(async()=>{await new Promise(ok=>server.close(ok));if(directory)rmSync(directory,{recursive:true});});return path;
}
test('client pins request identity and consumes response before ACK',async t=>{
 let received,ack='';const socket=await fixture(t,stream=>{let buffer='';stream.on('data',chunk=>{
  buffer+=chunk;if(!received&&buffer.includes('\n')){received=JSON.parse(buffer.trim());buffer='';stream.write(JSON.stringify({apiVersion:1,requestId:received.requestId,data:{state:'RUNNING'}})+'\n');}
  else if(received&&buffer==='ACK\n'){ack=buffer;stream.end();}
 });});
 const requestId=randomUUID();assert.deepEqual(await exchange(socket,'STATUS',{requestId}),{state:'RUNNING'});assert.equal(received.requestId,requestId);assert.equal(ack,'ACK\n');
});
test('lost shutdown response is unconfirmed and never automatically replayed',async t=>{
 let calls=0;const socket=await fixture(t,stream=>stream.once('data',()=>{calls++;stream.end();}));
 await assert.rejects(exchange(socket,'SHUTDOWN',{expectedInstance:randomUUID()}),/APP_CONTROL_RESPONSE_UNCONFIRMED/);assert.equal(calls,1);
});
test('mismatched response identity is rejected without acknowledgement',async t=>{
 let ack=false;const socket=await fixture(t,stream=>{let first=true;stream.on('data',chunk=>{if(first){first=false;stream.write(JSON.stringify({apiVersion:1,requestId:randomUUID(),data:{state:'RUNNING'}})+'\n');}else{ack=chunk.toString().includes('ACK');}});});
 await assert.rejects(exchange(socket,'STATUS'),/APP_CONTROL_RESPONSE_INVALID/);assert.equal(ack,false);
});
test('server refuses an obsolete Host epoch without a second mutation',async t=>{
 let calls=0;const socket=await fixture(t,stream=>stream.once('data',()=>{calls++;stream.end(JSON.stringify({apiVersion:1,errorCode:'APP_CONTROL_INSTANCE_CHANGED'})+'\n');}));
 await assert.rejects(exchange(socket,'SHUTDOWN',{expectedInstance:randomUUID()}),/APP_CONTROL_INSTANCE_CHANGED/);assert.equal(calls,1);
});
