import {test} from 'node:test';
import assert from 'node:assert/strict';
import {EventEmitter} from 'node:events';
import {PassThrough,Writable} from 'node:stream';
import {AcpDiscovery} from './discovery.mjs';
function fixture(options){
 const child=new EventEmitter();child.stdout=new PassThrough();child.stderr=new PassThrough();const sent=[];
 child.stdin=new Writable({write(data,_enc,done){sent.push(JSON.parse(data));done();}});
 const client=new AcpDiscovery(child,options);return{child,sent,client,reply(id,result){child.stdout.write(JSON.stringify({jsonrpc:'2.0',id,result})+'\n');}};
}
test('discovery never permits conversation mutations or authentication',async()=>{
 const f=fixture();for(const method of ['session/new','session/load','session/prompt','authenticate'])await assert.rejects(f.client.request(method,{}),/READ_ONLY/);
 assert.equal(f.sent.length,0);f.client.close();
});
test('split frames correlate exact IDs and preserve native identities',async()=>{
 const f=fixture();const init=f.client.initialize();f.reply(1,{protocolVersion:1,agentCapabilities:{sessionCapabilities:{list:{}}}});await init;
 const listing=f.client.sessions();const line=JSON.stringify({jsonrpc:'2.0',id:2,result:{sessions:[{sessionId:'native-id',cwd:'D:/project',title:'Native title'}]}})+'\n';
 f.child.stdout.write(line.slice(0,19));f.child.stdout.write(line.slice(19));assert.equal((await listing).sessions[0].sessionId,'native-id');f.client.close();
});
test('rejects unbounded frames and does not retry timed-out reads',async()=>{
 const f=fixture({maxBytes:32});const p=f.client.request('initialize',{});f.child.stdout.write('x'.repeat(33));await assert.rejects(p,/FRAME_TOO_LARGE/);f.client.close();
 const g=fixture({timeoutMs:5});await assert.rejects(g.client.request('initialize',{}),/READ_TIMEOUT/);assert.equal(g.sent.length,1);g.client.close();
});
test('native server requests cannot grant filesystem or tool permissions',async()=>{
 const f=fixture();f.child.stdout.write(JSON.stringify({jsonrpc:'2.0',id:'permission',method:'session/request_permission',params:{}})+'\n');
 assert.equal(f.sent[0].error.code,-32601);assert.equal(f.sent[0].id,'permission');f.client.close();
});
