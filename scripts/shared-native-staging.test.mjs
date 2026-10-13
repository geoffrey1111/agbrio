// Tiny text-only fixtures for the bounded native staging cleanup.
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {mkdir,mkdtemp,writeFile,readFile,readdir,unlink,rmdir,lstat} from 'node:fs/promises';
import {join,resolve} from 'node:path';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
const exec=promisify(execFile),scratch=resolve('runtime/shared-native-compatibility');
async function clean(path){for(const item of await readdir(path,{withFileTypes:true})){const p=join(path,item.name);assert.equal((await lstat(p)).isSymbolicLink(),false);if(item.isDirectory())await clean(p);else await unlink(p);}await rmdir(path);}
async function fixture(run){await mkdir(scratch,{recursive:true});const root=await mkdtemp(join(scratch,'native-staging-')),stage=join(root,'staging-'+'a'.repeat(32));await mkdir(stage);try{await run({root,stage});}finally{await clean(root);}}
async function cleanup(root,stage){const source=resolve('src-tauri/resources/codex-shared/native-runtime.ps1');const quote=s=>"'"+s.replaceAll("'","''")+"'";return exec('powershell.exe',['-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-Command',`. ${quote(source)}; Remove-NativeStaging ${quote(stage)} ${quote(root)}`],{windowsHide:true,timeout:10000});}
test('cleanup removes only allowed new staging leaves and its empty directory',()=>fixture(async({root,stage})=>{await writeFile(join(stage,'codex.exe'),'not executable');await writeFile(join(stage,'manifest.json'),'{}');await cleanup(root,stage);assert.deepEqual(await readdir(root),[]);}));
test('unexpected user file rejects cleanup before deleting any allowed leaf',()=>fixture(async({root,stage})=>{await writeFile(join(stage,'codex.exe'),'fixture');await writeFile(join(stage,'notes.txt'),'preserve');await assert.rejects(cleanup(root,stage),e=>e.stderr.includes('SHARED_NATIVE_CLEANUP_REJECTED'));assert.equal(await readFile(join(stage,'codex.exe'),'utf8'),'fixture');assert.equal(await readFile(join(stage,'notes.txt'),'utf8'),'preserve');}));
test('old cache name and nested directories are rejected without recursive cleanup',()=>fixture(async({root,stage})=>{await mkdir(join(stage,'nested'));await assert.rejects(cleanup(root,stage),e=>e.stderr.includes('SHARED_NATIVE_CLEANUP_REJECTED'));await assert.rejects(cleanup(root,root),e=>e.stderr.includes('SHARED_NATIVE_CLEANUP_REJECTED'));assert.deepEqual(await readdir(stage),['nested']);}));

test('native byte copy preserves exact content and refuses to overwrite an existing destination',()=>fixture(async({root,stage})=>{
 const source=join(root,'source.txt'),target=join(stage,'codex.exe');await writeFile(source,Buffer.from([0,255,13,10,239,187,191]));
 const script=resolve('src-tauri/resources/codex-shared/native-runtime.ps1'),quote=s=>"'"+s.replaceAll("'","''")+"'";
 const command=`. ${quote(script)}; Copy-NativeBytes ${quote(source)} ${quote(target)}`;
 await exec('powershell.exe',['-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-Command',command],{windowsHide:true});assert.deepEqual(await readFile(target),await readFile(source));
 await writeFile(source,'changed');await assert.rejects(exec('powershell.exe',['-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-Command',command],{windowsHide:true}));assert.deepEqual(await readFile(target),Buffer.from([0,255,13,10,239,187,191]));
}));
