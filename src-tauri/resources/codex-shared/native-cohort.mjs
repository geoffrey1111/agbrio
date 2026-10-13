import {createHash} from 'node:crypto';
import {createReadStream} from 'node:fs';
import {lstat} from 'node:fs/promises';
import {dirname,join} from 'node:path';
export const nativeNames=['codex.exe','codex-code-mode-host.exe','codex-command-runner.exe','codex-windows-sandbox-setup.exe'];
export async function assertNativeCohort(binary,files){
 if(!Array.isArray(files)||files.length!==nativeNames.length)throw Error('SHARED_NATIVE_RUNTIME_INCOMPLETE');
 for(const name of nativeNames){
  const matches=files.filter(f=>f.name===name);
  if(matches.length!==1||!/^[a-f0-9]{64}$/.test(matches[0].sha256))throw Error('SHARED_NATIVE_COHORT_MISMATCH');
  const path=join(dirname(binary),name),stat=await lstat(path).catch(()=>{throw Error('SHARED_NATIVE_RUNTIME_INCOMPLETE');});
  if(!stat.isFile()||stat.isSymbolicLink()||stat.size!==matches[0].bytes)throw Error('SHARED_NATIVE_COHORT_MISMATCH');
  const hash=createHash('sha256');for await(const chunk of createReadStream(path))hash.update(chunk);
  if(hash.digest('hex')!==matches[0].sha256)throw Error('SHARED_NATIVE_COHORT_MISMATCH');
 }
}
