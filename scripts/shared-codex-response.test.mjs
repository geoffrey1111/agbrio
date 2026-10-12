import test from 'node:test';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {spawn} from 'node:child_process';
import {createHash} from 'node:crypto';
import {mkdir,mkdtemp,copyFile,symlink,unlink,rmdir} from 'node:fs/promises';
import {fileURLToPath} from 'node:url';
import {join,dirname} from 'node:path';
const root=fileURLToPath(new URL('../',import.meta.url));
const require=createRequire(import.meta.url);
const wsRoot=dirname(require.resolve('ws/package.json',{paths:[join(root,'tools/isolated-browser-executor')]}));
const {WebSocketServer}=require(wsRoot);

async function fixture(run){
  const scratch=join(root,'runtime');await mkdir(scratch,{recursive:true});
  const directory=await mkdtemp(join(scratch,'shared-response-test-'));
  const transport=join(directory,'transport.mjs'),link=join(directory,'ws');
  await copyFile(join(root,'src-tauri/resources/codex-shared/transport.mjs'),transport);
  await symlink(wsRoot,link,process.platform==='win32'?'junction':'dir');
  const server=new WebSocketServer({host:'127.0.0.1',port:0});
  await new Promise(resolve=>server.once('listening',resolve));
  const child=spawn(process.execPath,[transport,`ws://127.0.0.1:${server.address().port}/rpc`],{windowsHide:true,stdio:['pipe','pipe','pipe']});
  const closed=new Promise(resolve=>child.once('exit',code=>resolve(code)));
  try{await run({server,child,closed});}
  finally{
    child.stdin.destroy();if(child.exitCode===null)child.kill();await closed;
    for(const socket of server.clients)socket.terminate();
    await new Promise(resolve=>server.close(resolve));
    // Only the two fixture-owned entries; never follow/delete the shared ws target.
    await unlink(transport);if(process.platform==='win32')await rmdir(link);else await unlink(link);await rmdir(directory);
  }
}

test('actual transport relays a 50.5 MiB history page exactly and stays usable',{timeout:20000},async()=>{
  await fixture(async({server,child})=>{
    const prefix='{"id":1,"result":{"history":"',suffix='"}}';
    const payload=prefix+'a'.repeat(52939021-prefix.length-suffix.length)+suffix;
    const expected=createHash('sha256').update(payload+'\n').digest('hex');
    let bytes=0;const hash=createHash('sha256');let lines=0;
    const received=new Promise((resolve,reject)=>{
      child.stdout.on('data',chunk=>{
        const newline=chunk.indexOf(10);if(!lines){if(newline>=0){hash.update(chunk.subarray(0,newline+1));bytes+=newline+1;lines++;assert.equal(hash.digest('hex'),expected);assert.equal(bytes,52939022);child.stdin.write('{"id":2,"method":"thread/read"}\n');}else{hash.update(chunk);bytes+=chunk.length;}}
        else if(newline>=0){assert.match(chunk.toString(),/"id":2/);lines++;resolve();}
      });child.once('exit',()=>{if(lines<2)reject(Error('transport closed before follow-up'));});
    });
    server.on('connection',socket=>socket.on('message',data=>{const request=JSON.parse(data);socket.send(request.id===1?payload:'{"id":2,"result":{"ok":true}}');}));
    child.stdin.write('{"id":1,"method":"thread/turns/list"}\n');await received;
  });
});

test('actual transport rejects a response above 100 MiB with a safe size category',{timeout:10000},async()=>{
  await fixture(async({server,child,closed})=>{
    let stderr='';child.stderr.on('data',chunk=>stderr+=chunk);
    server.on('connection',socket=>socket.on('message',()=>{
      // Oversized declared frame: rejection before allocating/sending its body.
      const header=Buffer.alloc(10);header[0]=0x81;header[1]=127;header.writeBigUInt64BE(BigInt(101*1024*1024),2);socket._socket.write(header);
    }));child.stdin.write('{"id":1,"method":"thread/turns/list"}\n');
    assert.equal(await closed,1);assert.match(stderr,/SHARED_BACKEND_RESPONSE_TOO_LARGE/);
  });
});

test('actual transport retains the smaller owner-request bound',{timeout:10000},async()=>{
  await fixture(async({server,child,closed})=>{
    let forwarded=false;server.on('connection',socket=>socket.on('message',()=>{forwarded=true;}));
    child.stdin.write('a'.repeat(16*1024*1024+1)+'\n');assert.equal(await closed,1);assert.equal(forwarded,false);
  });
});
