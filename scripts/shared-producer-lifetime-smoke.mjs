// Real preparation process exits; verify its detached producer still serves.
// Empty home/ephemeral port, no Desktop or model; parent owns exact final cleanup.
import assert from 'node:assert/strict';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {readFile,readdir,lstat,unlink,rmdir} from 'node:fs/promises';
import {resolve,join,relative} from 'node:path';
import {lifecycle} from '../src-tauri/resources/codex-shared/control.mjs';
const exec=promisify(execFile),node=resolve(process.argv[3]??process.execPath),binary=resolve(process.argv[2]);
const out=await exec(node,[resolve('scripts/shared-cold-prepare-smoke.mjs'),binary,node,'--leave-prepared'],{windowsHide:true,timeout:60000,maxBuffer:262144});
const proof=JSON.parse(out.stdout),directory=resolve(proof.preparedDirectory),scratch=resolve('runtime/shared-native-compatibility');
assert.equal(relative(scratch,directory).startsWith('cold-prepare-'),true);assert.equal((await lstat(directory)).isSymbolicLink(),false);
let cleanable=false;
async function clean(path){assert.ok(!relative(directory,path).startsWith('..'));for(const name of await readdir(path)){const p=join(path,name),s=await lstat(p);assert.equal(s.isSymbolicLink(),false);if(s.isDirectory())await clean(p);else await unlink(p);}await rmdir(path);}
try{
 await new Promise(yes=>setTimeout(yes,1000));
 const state=await lifecycle({directory,scripts:resolve('src-tauri/resources/codex-shared'),action:'status'});
 assert.equal(state.ready,true,'Producer must outlive its original preparation process');
 await lifecycle({directory,scripts:resolve('src-tauri/resources/codex-shared'),action:'disable'});
 const cfg=JSON.parse(await readFile(join(directory,'runtime-config.json'),'utf8'));assert.equal(cfg.phase,'DISABLED');cleanable=true;
 console.log(JSON.stringify({producerAfterCreatorExit:'PASS',ownedShutdown:'PASS',desktopStarts:0,modelRequests:0}));
}finally{
 if(!cleanable)try{await lifecycle({directory,scripts:resolve('src-tauri/resources/codex-shared'),action:'disable'});cleanable=true;}catch{}
 if(cleanable)await clean(directory);else process.stderr.write('OWNED_PRODUCER_REQUIRES_IDENTITY_RECOVERY\n');
}
