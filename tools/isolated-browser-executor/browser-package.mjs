// Optional pinned Chromium package. No profiles, provider requests or browser launch.
import path from 'node:path';
import {createHash,randomUUID} from 'node:crypto';
import {createReadStream,createWriteStream} from 'node:fs';
import {readFile,lstat,mkdir,mkdtemp,rename,rm,open} from 'node:fs/promises';
import {Readable,Transform} from 'node:stream';
import {pipeline} from 'node:stream/promises';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
const exec=promisify(execFile);
async function unlinked(p){for(let q=path.resolve(p);;q=path.dirname(q)){try{if((await lstat(q)).isSymbolicLink())throw Error('CHROMIUM_COMPONENT_LINK_REJECTED');}catch(e){if(e.code!=='ENOENT')throw e;}if(path.dirname(q)===q)break;}}
async function exists(p){try{await lstat(p);return true;}catch(e){if(e.code==='ENOENT')return false;throw e;}}
async function sha(p){const h=createHash('sha256');for await(const b of createReadStream(p))h.update(b);return h.digest('hex');}
export async function verifyPackage(root,manifest){await unlinked(root);for(const [rel,hash] of Object.entries(manifest.files)){const p=path.resolve(root,rel);if(!p.startsWith(path.resolve(root)+path.sep)||rel.includes(':')||!/^([a-f0-9]{64})$/.test(hash))throw Error('CHROMIUM_MANIFEST_PATH_INVALID');await unlinked(p);if(await sha(p)!==hash)throw Error('CHROMIUM_PACKAGE_HASH_MISMATCH');}return path.join(root,'chrome.exe');}
export async function browserPackage({resources,data,download=false},io={}){
 const manifest=JSON.parse(await readFile(path.join(resources,'chromium-manifest.json'),'utf8'));
 const policy=JSON.parse(await readFile(path.join(resources,'runtime-policy.json'),'utf8'));
 const url=new URL(policy.source);if(url.origin!=='https://storage.googleapis.com'||url.pathname!==`/chromium-browser-snapshots/Win_x64/${policy.snapshot}/chrome-win.zip`||url.search||url.hash||!/^\d+$/.test(policy.snapshot)||manifest.archiveSha256!==policy.archiveSha256||manifest.executableSha256!==policy.executableSha256)throw Error('CHROMIUM_COMPONENT_POLICY_INVALID');
 const bundled=path.join(resources,'chromium');if(await exists(bundled))return verifyPackage(bundled,manifest);
 const base=path.join(data,'browser-runtime'),cache=path.join(base,`chromium-${policy.snapshot}`);await unlinked(base);
 if(await exists(cache))return verifyPackage(cache,manifest);
 if(!download)throw Error('CHROMIUM_COMPONENT_NOT_INSTALLED');
 await mkdir(base,{recursive:true});const lockPath=path.join(base,`.${policy.snapshot}.lock`);let lock;
 try{lock=await open(lockPath,'wx');}catch(e){
  if(e.code!=='EEXIST')throw e;
  // Recover only a marked lock whose owner process is definitely gone.
  let old,raw;try{raw=await readFile(lockPath,'utf8');old=JSON.parse(raw);}catch{throw Error('CHROMIUM_COMPONENT_BUSY');}
  if(old.schema!=='AIWR_COMPONENT_LOCK_V1'||old.cache!==cache||!Number.isSafeInteger(old.pid)||old.pid<1||typeof old.nonce!=='string')throw Error('CHROMIUM_COMPONENT_BUSY');
  try{process.kill(old.pid,0);throw Error('CHROMIUM_COMPONENT_BUSY');}catch(error){if(error.code!=='ESRCH')throw Error('CHROMIUM_COMPONENT_BUSY');}
  if(await readFile(lockPath,'utf8')!==raw)throw Error('CHROMIUM_COMPONENT_BUSY');await rm(lockPath);lock=await open(lockPath,'wx');
 }
 let scratch;
 try{
  await lock.writeFile(JSON.stringify({schema:'AIWR_COMPONENT_LOCK_V1',pid:process.pid,cache,nonce:randomUUID()}));
  if(await exists(cache))return await verifyPackage(cache,manifest);
  scratch=await mkdtemp(path.join(base,`.download-${policy.snapshot}-`));const archive=path.join(scratch,'chrome.zip');
  const response=await (io.fetch??fetch)(url.href,{redirect:'error',signal:AbortSignal.timeout(900000)});if(!response.ok||!response.body)throw Error('CHROMIUM_COMPONENT_DOWNLOAD_FAILED');
  let size=0;const limit=new Transform({transform(chunk,_enc,cb){size+=chunk.length;cb(size>1200*1024*1024?Error('CHROMIUM_COMPONENT_TOO_LARGE'):null,chunk);}});
  await pipeline(Readable.fromWeb(response.body),limit,createWriteStream(archive,{flags:'wx'}));if(await sha(archive)!==policy.archiveSha256)throw Error('CHROMIUM_ARCHIVE_HASH_MISMATCH');
  const extracted=path.join(scratch,'extracted');await mkdir(extracted);
  if(io.extract)await io.extract(archive,extracted);else{
   const systemRoot=process.env.SystemRoot;if(!systemRoot||!path.isAbsolute(systemRoot))throw Error('CHROMIUM_COMPONENT_EXTRACT_UNAVAILABLE');
   await exec(path.join(systemRoot,'System32/WindowsPowerShell/v1.0/powershell.exe'),['-NoProfile','-NonInteractive','-Command','Expand-Archive -LiteralPath $env:AGBRIO_BROWSER_ARCHIVE -DestinationPath $env:AGBRIO_BROWSER_EXTRACT'],{windowsHide:true,timeout:600000,maxBuffer:32768,env:{...process.env,AGBRIO_BROWSER_ARCHIVE:archive,AGBRIO_BROWSER_EXTRACT:extracted}});
  }
  const source=path.join(extracted,'chrome-win');await verifyPackage(source,manifest);await rm(path.join(source,'interactive_ui_tests.exe'),{force:true});await rename(source,cache);return path.join(cache,'chrome.exe');
 }finally{
  // Only this call's freshly-created scratch and lock, never a profile/cache tree.
  if(scratch&&path.dirname(scratch)===base&&path.basename(scratch).startsWith(`.download-${policy.snapshot}-`))await rm(scratch,{recursive:true,force:true});
  await lock.close();await rm(lockPath,{force:true});
 }
}
