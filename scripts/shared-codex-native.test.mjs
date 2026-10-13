import {test} from 'node:test';
import assert from 'node:assert/strict';
import {mkdir,writeFile,unlink,rmdir} from 'node:fs/promises';
import {createHash,randomUUID} from 'node:crypto';
import {join,resolve} from 'node:path';
import {nativeNames,assertNativeCohort} from '../src-tauri/resources/codex-shared/native-cohort.mjs';
async function fixture(run){
 const dir=resolve('runtime/shared-native-compatibility',randomUUID());await mkdir(dir,{recursive:true});const files=[];
 try{
  for(const name of nativeNames){const bytes=Buffer.from('fixture:'+name);await writeFile(join(dir,name),bytes);files.push({name,bytes:bytes.length,sha256:createHash('sha256').update(bytes).digest('hex')});}
  await run(join(dir,'codex.exe'),files);
 }finally{for(const name of nativeNames)await unlink(join(dir,name)).catch(e=>{if(e.code!=='ENOENT')throw e;});await rmdir(dir);}
}
test('native restart rejects a missing tool host rather than borrowing another cohort',()=>fixture(async(binary,files)=>{
 await unlink(join(resolve(binary,'..'),'codex-code-mode-host.exe'));
 await assert.rejects(assertNativeCohort(binary,files),/SHARED_NATIVE_RUNTIME_INCOMPLETE/);
}));
test('same-size replaced tool host cannot pass a verified native cohort',()=>fixture(async(binary,files)=>{
 await assertNativeCohort(binary,files);
 const entry=files.find(f=>f.name==='codex-code-mode-host.exe');await writeFile(join(resolve(binary,'..'),entry.name),Buffer.alloc(entry.bytes,65));
 await assert.rejects(assertNativeCohort(binary,files),/SHARED_NATIVE_COHORT_MISMATCH/);
}));
test('manifest duplicates cannot hide a missing command runner',()=>fixture(async(binary,files)=>{
 const bad=files.map(f=>f.name==='codex-command-runner.exe'?files[0]:f);
 await assert.rejects(assertNativeCohort(binary,bad),/SHARED_NATIVE_COHORT_MISMATCH/);
}));
