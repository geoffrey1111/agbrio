// Ordinary tiny text fixtures only. No installed/native binary copying.
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {mkdir,writeFile,readFile,readdir,unlink,rmdir,lstat,rename} from 'node:fs/promises';
import {join,resolve,basename} from 'node:path';
import {randomUUID} from 'node:crypto';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {stageSharedResources,sharedResourcesMatch} from '../src-tauri/resources/codex-shared/launch-entry.mjs';
const exec=promisify(execFile),entry=resolve('src-tauri/resources/codex-shared/launch-entry.mjs');
async function discard(path){for(const e of await readdir(path,{withFileTypes:true})){const file=join(path,e.name);assert.equal((await lstat(file)).isSymbolicLink(),false);if(e.isDirectory())await discard(file);else await unlink(file);}await rmdir(path);}
async function fixture(run){
 const root=resolve('runtime/shared-native-compatibility','stage-'+randomUUID()),source=join(root,'install/codex-shared'),directory=join(root,'owner');
 const ws=join(root,'install/browser-executor/node_modules/ws');await mkdir(source,{recursive:true});await mkdir(ws,{recursive:true});await mkdir(join(directory,'runtime'),{recursive:true});
 const node=join(root,'install/fixture-node.txt');await writeFile(node,'not an executable');await writeFile(join(source,'control.mjs'),'new text');await writeFile(join(ws,'index.js'),'ws fixture');await writeFile(join(directory,'runtime/control.mjs'),'previous text');
 try{await run({root,source,directory,node});}finally{await discard(root);}
}
test('resource promotion installs one complete fixture and retains the prior version',()=>fixture(async o=>{
 const moves=[];
 try{await stageSharedResources(o,{move:async(from,to)=>{try{await rename(from,to);moves.push({from:basename(from),to:basename(to),result:'OK'});}catch(error){moves.push({from:basename(from),to:basename(to),code:error.code});throw error;}}});}
 catch(error){throw Error(error.message+': '+JSON.stringify(moves),{cause:error});}
 assert.equal(await readFile(join(o.directory,'runtime/control.mjs'),'utf8'),'new text');assert.equal(await readFile(join(o.directory,'runtime/ws/index.js'),'utf8'),'ws fixture');
 const names=await readdir(o.directory);assert.equal(names.filter(n=>n.startsWith('runtime-next-')).length,0);const previous=names.find(n=>n.startsWith('runtime-previous-'));assert.equal(await readFile(join(o.directory,previous,'control.mjs'),'utf8'),'previous text');
}));
test('promotion failure restores old resources and discards only its staging fixture',()=>fixture(async o=>{
 await assert.rejects(stageSharedResources(o,{move:async(from,to)=>{if(from.includes('runtime-next-'))throw Error('fixture locked');return rename(from,to);}}),/SHARED_RUNTIME_PROMOTION_FAILED/);
 assert.equal(await readFile(join(o.directory,'runtime/control.mjs'),'utf8'),'previous text');assert.deepEqual(await readdir(o.directory),['runtime']);
}));
test('missing source component does not replace existing resources or leave a partial staging directory',()=>fixture(async o=>{
 await unlink(o.node);await assert.rejects(stageSharedResources(o));assert.equal(await readFile(join(o.directory,'runtime/control.mjs'),'utf8'),'previous text');assert.deepEqual(await readdir(o.directory),['runtime']);
}));

test('actual app staging command promotes text fixtures and writes the version before promotion',()=>fixture(async o=>{
 const stamp='a'.repeat(64);
 const result=await exec(process.execPath,[entry,'--stage-only',o.source,o.directory,o.node,stamp],{windowsHide:true,timeout:10000});
 assert.deepEqual(JSON.parse(result.stdout),{staged:true});
 assert.equal(await readFile(join(o.directory,'runtime/version'),'utf8'),stamp);
 assert.equal(await readFile(join(o.directory,'runtime/control.mjs'),'utf8'),'new text');
 assert.equal((await readdir(o.directory)).filter(name=>name.startsWith('runtime-next-')).length,0);
}));

test('actual staging command rejects a missing node and preserves old runtime without scratch leftovers',()=>fixture(async o=>{
 await unlink(o.node);
 await assert.rejects(exec(process.execPath,[entry,'--stage-only',o.source,o.directory,o.node,'b'.repeat(64)],{windowsHide:true,timeout:10000}),error=>error.code===1&&error.stderr==='SHARED_RUNTIME_COPY_FAILED');
 assert.equal(await readFile(join(o.directory,'runtime/control.mjs'),'utf8'),'previous text');
 assert.deepEqual(await readdir(o.directory),['runtime']);
}));

test('invalid version stamp is rejected before any resource staging',()=>fixture(async o=>{
 await assert.rejects(stageSharedResources({...o,stamp:'not a version hash'}),/SHARED_RUNTIME_STAMP_INVALID/);
 assert.deepEqual(await readdir(o.directory),['runtime']);
}));

test('idle resource comparison verifies runtime, node, ws bytes and file identities despite an unchanged stamp',()=>fixture(async o=>{
 await stageSharedResources({...o,stamp:'c'.repeat(64)});assert.equal(await sharedResourcesMatch(o),true);
 await writeFile(join(o.directory,'runtime/control.mjs'),'changed');assert.equal(await sharedResourcesMatch(o),false);
 await writeFile(join(o.directory,'runtime/control.mjs'),'new text');assert.equal(await sharedResourcesMatch(o),true);
 await writeFile(join(o.directory,'runtime/node.exe'),'changed node');assert.equal(await sharedResourcesMatch(o),false);
 await writeFile(join(o.directory,'runtime/node.exe'),'not an executable');assert.equal(await sharedResourcesMatch(o),true);
 await rename(join(o.directory,'runtime/ws/index.js'),join(o.directory,'runtime/ws/renamed.js'));assert.equal(await sharedResourcesMatch(o),false);
 await rename(join(o.directory,'runtime/ws/renamed.js'),join(o.directory,'runtime/ws/index.js'));assert.equal(await sharedResourcesMatch(o),true);
 await unlink(join(o.directory,'runtime/control.mjs'));assert.equal(await sharedResourcesMatch(o),false);
}));
