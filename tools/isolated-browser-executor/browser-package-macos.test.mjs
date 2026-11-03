import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {mkdtemp,mkdir,writeFile,readFile,rm,chmod,symlink} from 'node:fs/promises';
import path from 'node:path';
import {tmpdir} from 'node:os';
import {macosPolicy,verifyMacosPackage,macosBrowserPackage} from './browser-package-macos.mjs';
const executable='Chromium.app/Contents/MacOS/Chromium';
function manifest(){const sha=createHash('sha256').update('fixture').digest('hex');return{schema:'AIWR_CHROMIUM_PACKAGE_V2',platform:'darwin-arm64',snapshot:'1707745',version:'157.0.8079.0',source:'https://storage.googleapis.com/chromium-browser-snapshots/Mac_Arm/1707745/chrome-mac.zip',executable,archiveSha256:'1'.repeat(64),executableSha256:sha,files:{[executable]:sha},modes:{[executable]:0o755},links:{}};}
test('Mac package policy refuses moving versions, Windows archives and traversal',()=>{
 const good=manifest();assert.equal(macosPolicy(good),good);
 for(const bad of [{...good,source:good.source+'?latest'},{...good,platform:'win32-x64'},{...good,snapshot:'latest'},{...good,files:{...good.files,'../escape':'1'.repeat(64)}},{...good,links:{'link':'../../outside'}}])assert.throws(()=>macosPolicy(bad));
});
test('ordinary Mac package reads never create a cache or download a browser',async t=>{
 const root=await mkdtemp(path.join(tmpdir(),'agbrio-mac-package-'));t.after(()=>rm(root,{recursive:true,force:true}));await writeFile(path.join(root,'chromium-manifest-macos.json'),JSON.stringify(manifest()));let calls=0;
 await assert.rejects(macosBrowserPackage({resources:root,data:path.join(root,'data')},{fetch:()=>{calls++;throw Error('forbidden');}}),/CHROMIUM_COMPONENT_NOT_INSTALLED/);assert.equal(calls,0);
});
test('Mac full-file verification preserves declared links and rejects all extra or changed bytes',{skip:process.platform!=='darwin'},async t=>{
 const root=await mkdtemp(path.join(tmpdir(),'agbrio-mac-files-'));t.after(()=>rm(root,{recursive:true,force:true}));const binary=path.join(root,executable);await mkdir(path.dirname(binary),{recursive:true});await writeFile(binary,'fixture');await chmod(binary,0o755);const policy=manifest();policy.links['Chromium.app/Contents/MacOS/current']='Chromium';await symlink('Chromium',path.join(path.dirname(binary),'current'));
 assert.equal(await verifyMacosPackage(root,policy),binary);
 await writeFile(path.join(root,'extra'),'unexpected');await assert.rejects(verifyMacosPackage(root,policy),/CHROMIUM_PACKAGE_HASH_MISMATCH/);await rm(path.join(root,'extra'));
 await writeFile(binary,'tampered');await assert.rejects(verifyMacosPackage(root,policy),/CHROMIUM_PACKAGE_HASH_MISMATCH/);await writeFile(binary,'fixture');await chmod(binary,0o600);await assert.rejects(verifyMacosPackage(root,policy),/CHROMIUM_PACKAGE_MODE_MISMATCH/);
});
test('Mac package verification rejects changed symlinks without reading their targets',{skip:process.platform!=='darwin'},async t=>{
 const root=await mkdtemp(path.join(tmpdir(),'agbrio-mac-link-'));t.after(()=>rm(root,{recursive:true,force:true}));const binary=path.join(root,executable);await mkdir(path.dirname(binary),{recursive:true});await writeFile(binary,'fixture');await chmod(binary,0o755);
 const foreign=await mkdtemp(path.join(tmpdir(),'agbrio-mac-protected-'));t.after(()=>rm(foreign,{recursive:true,force:true}));const outside=path.join(foreign,'preserve');await writeFile(outside,'preserve');const policy=manifest();policy.links['Chromium.app/Contents/MacOS/current']='Chromium';await symlink(outside,path.join(path.dirname(binary),'current'));
 await assert.rejects(verifyMacosPackage(root,policy),/CHROMIUM_COMPONENT_LINK_REJECTED/);assert.equal(await readFile(outside,'utf8'),'preserve');
});
