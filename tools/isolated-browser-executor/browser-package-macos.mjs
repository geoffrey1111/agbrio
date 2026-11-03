// Fixed, audited Apple Silicon package. This module never opens a browser.
import path from 'node:path';
import {constants,createReadStream,createWriteStream} from 'node:fs';
import {lstat,readFile,readlink,realpath,readdir,mkdir,open,rename,rm,mkdtemp} from 'node:fs/promises';
import {createHash,randomUUID} from 'node:crypto';
import {pipeline} from 'node:stream/promises';
import {Readable,Transform} from 'node:stream';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
const exec=promisify(execFile);
const executable='Chromium.app/Contents/MacOS/Chromium';
const source='https://storage.googleapis.com/chromium-browser-snapshots/Mac_Arm/1707745/chrome-mac.zip';
async function exists(file){try{await lstat(file);return true;}catch(e){if(e.code==='ENOENT')return false;throw e;}}
async function unlinked(file){for(let p=path.resolve(file);;p=path.dirname(p)){try{if((await lstat(p)).isSymbolicLink())throw Error('CHROMIUM_COMPONENT_LINK_REJECTED');}catch(e){if(e.code!=='ENOENT')throw e;}if(path.dirname(p)===p)break;}}
async function sha(file){const result=createHash('sha256');const handle=await open(file,constants.O_RDONLY|constants.O_NOFOLLOW);try{const stat=await handle.stat();if(!stat.isFile()||stat.size>1200*1024*1024)throw Error('CHROMIUM_PACKAGE_FILE_INVALID');for await(const part of handle.createReadStream({autoClose:false}))result.update(part);return result.digest('hex');}finally{await handle.close();}}
function relative(value){return typeof value==='string'&&value.length>0&&!value.startsWith('/')&&!value.includes('\\')&&!value.includes(':')&&value.split('/').every(p=>p&&p!=='.'&&p!=='..');}
export function macosPolicy(manifest){
 if(manifest.schema!=='AIWR_CHROMIUM_PACKAGE_V2'||manifest.platform!=='darwin-arm64'||manifest.snapshot!=='1707745'||manifest.version!=='157.0.8079.0'||manifest.source!==source||manifest.executable!==executable||!/^([a-f0-9]{64})$/.test(manifest.archiveSha256)||!/^([a-f0-9]{64})$/.test(manifest.executableSha256)||manifest.files?.[executable]!==manifest.executableSha256||!manifest.links||!manifest.modes)throw Error('CHROMIUM_COMPONENT_POLICY_INVALID');
 for(const [name,hash] of Object.entries(manifest.files??{}))if(!relative(name)||!/^([a-f0-9]{64})$/.test(hash)||!Number.isInteger(manifest.modes?.[name])||manifest.modes[name]<0||manifest.modes[name]>0o777)throw Error('CHROMIUM_MANIFEST_PATH_INVALID');
 for(const [name,target] of Object.entries(manifest.links??{}))if(!relative(name)||typeof target!=='string'||target.startsWith('/')||target.includes('\\')||!relative(path.posix.normalize(path.posix.join(path.posix.dirname(name),target)))||Object.hasOwn(manifest.files,name))throw Error('CHROMIUM_MANIFEST_LINK_INVALID');
 return manifest;
}
export async function verifyMacosPackage(root,manifest){
 macosPolicy(manifest);await unlinked(root);const base=await realpath(root),seen=new Set();let count=0;
 async function walk(directory){
  for(const entry of await readdir(directory,{withFileTypes:true})){
   if(++count>10000)throw Error('CHROMIUM_PACKAGE_TOO_MANY_FILES');
   const file=path.join(directory,entry.name),name=path.relative(root,file).split(path.sep).join('/'),stat=await lstat(file);
   if(stat.isSymbolicLink()){
    if(!Object.hasOwn(manifest.links,name)||await readlink(file)!==manifest.links[name])throw Error('CHROMIUM_COMPONENT_LINK_REJECTED');
    const resolved=await realpath(file);if(!resolved.startsWith(base+path.sep))throw Error('CHROMIUM_COMPONENT_LINK_REJECTED');seen.add(name);
   }else if(stat.isDirectory()){await walk(file);}
   else if(stat.isFile()){
    if(!Object.hasOwn(manifest.files,name)||await sha(file)!==manifest.files[name])throw Error('CHROMIUM_PACKAGE_HASH_MISMATCH');
    if(process.platform==='darwin'&&(stat.mode&0o777)!==manifest.modes[name])throw Error('CHROMIUM_PACKAGE_MODE_MISMATCH');seen.add(name);
   }else throw Error('CHROMIUM_PACKAGE_FILE_INVALID');
  }
 }
 await walk(root);if(seen.size!==Object.keys(manifest.files).length+Object.keys(manifest.links).length)throw Error('CHROMIUM_PACKAGE_INCOMPLETE');
 if(!(manifest.modes[executable]&0o111))throw Error('CHROMIUM_PACKAGE_MODE_MISMATCH');
 return path.join(root,executable);
}
export async function macosBrowserPackage({resources,data,download=false},io={}){
 if(process.arch!=='arm64'&&process.platform==='darwin')throw Error('CHROMIUM_PLATFORM_UNSUPPORTED');
 const manifest=macosPolicy(JSON.parse(await readFile(path.join(resources,'chromium-manifest-macos.json'),'utf8')));
 const bundled=path.join(resources,'chromium');if(await exists(bundled))return verifyMacosPackage(bundled,manifest);
 const base=path.join(data,'browser-runtime'),cache=path.join(base,'chromium-macos-1707745');await unlinked(base);
 if(await exists(cache))return verifyMacosPackage(cache,manifest);
 if(!download)throw Error('CHROMIUM_COMPONENT_NOT_INSTALLED');
 await mkdir(base,{recursive:true,mode:0o700});const lockPath=path.join(base,'.macos-1707745.lock');await unlinked(lockPath);let lock;
 try{lock=await open(lockPath,'wx',0o600);}catch(e){
  if(e.code!=='EEXIST')throw e;
  let old,raw;try{raw=await readFile(lockPath,'utf8');old=JSON.parse(raw);}catch{throw Error('CHROMIUM_COMPONENT_BUSY');}
  if(old.schema!=='AIWR_COMPONENT_LOCK_V1'||old.cache!==cache||!Number.isSafeInteger(old.pid)||old.pid<1||typeof old.nonce!=='string')throw Error('CHROMIUM_COMPONENT_BUSY');
  try{process.kill(old.pid,0);throw Error('CHROMIUM_COMPONENT_BUSY');}catch(error){if(error.code!=='ESRCH')throw Error('CHROMIUM_COMPONENT_BUSY');}
  if(await readFile(lockPath,'utf8')!==raw)throw Error('CHROMIUM_COMPONENT_BUSY');await rm(lockPath);lock=await open(lockPath,'wx',0o600);
 }
 let scratch;
 try{
  await lock.writeFile(JSON.stringify({schema:'AIWR_COMPONENT_LOCK_V1',pid:process.pid,cache,nonce:randomUUID()}));
  if(await exists(cache))return await verifyMacosPackage(cache,manifest);
  scratch=await mkdtemp(path.join(base,'.download-macos-1707745-'));const archive=path.join(scratch,'chrome.zip');
  const response=await(io.fetch??fetch)(source,{redirect:'error',signal:AbortSignal.timeout(900000)});if(!response.ok||!response.body)throw Error('CHROMIUM_COMPONENT_DOWNLOAD_FAILED');
  let bytes=0;const limit=new Transform({transform(chunk,_encoding,done){bytes+=chunk.length;done(bytes>1200*1024*1024?Error('CHROMIUM_COMPONENT_TOO_LARGE'):null,chunk);}});
  await pipeline(Readable.fromWeb(response.body),limit,createWriteStream(archive,{flags:'wx',mode:0o600}));if(await sha(archive)!==manifest.archiveSha256)throw Error('CHROMIUM_ARCHIVE_HASH_MISMATCH');
  const extracted=path.join(scratch,'extracted');await mkdir(extracted,{mode:0o700});
  if(io.extract)await io.extract(archive,extracted);else await exec('/usr/bin/ditto',['-x','-k',archive,extracted],{timeout:600000,maxBuffer:32768});
  const sourceRoot=path.join(extracted,'chrome-mac');await verifyMacosPackage(sourceRoot,manifest);await rename(sourceRoot,cache);return path.join(cache,executable);
 }finally{
  if(scratch&&path.dirname(scratch)===base&&path.basename(scratch).startsWith('.download-macos-1707745-'))await rm(scratch,{recursive:true,force:true});
  await lock.close();await rm(lockPath,{force:true});
 }
}
