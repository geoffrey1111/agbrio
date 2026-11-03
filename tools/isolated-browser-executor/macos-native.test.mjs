import {test} from 'node:test';
import assert from 'node:assert/strict';
import {mkdir,mkdtemp,rm,writeFile,readFile} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import path from 'node:path';
import {pathToFileURL} from 'node:url';
async function fixture(t){
 const data=await mkdtemp(path.join(tmpdir(),'agbrio-native-'));t.after(()=>rm(data,{recursive:true,force:true}));const profile=path.join(data,'browser-profile/chatgpt');await mkdir(profile,{recursive:true,mode:0o700});
 const module=path.resolve('src-tauri/resources/browser-executor-lite/security-browser-macos.mjs');const{macosSecurityPlatform:platform}=await import(pathToFileURL(module).href);return{data,profile,platform};
}
test('native macOS profile guard is exclusive and releases on the exact child EOF',{skip:process.platform!=='darwin'},async t=>{
 const{data,profile,platform}=await fixture(t);await platform.ensureOwnedProfile(data,profile);assert.deepEqual(await platform.inventory(profile),[]);
 await platform.withExclusiveProfile(profile,async held=>{held();await assert.rejects(platform.withExclusiveProfile(profile,async()=>assert.fail('second owner')),/SECURITY_RECOVERY_BUSY/);held();});
 await platform.withExclusiveProfile(profile,async held=>held());assert.deepEqual(await platform.inventory(profile),[]);
 assert.equal(JSON.parse(await readFile(path.join(data,'owned-profile-location.json'),'utf8')).profile,profile);
});
test('unmarked nonempty profiles cannot be adopted and forged process actions stay refused',{skip:process.platform!=='darwin'},async t=>{
 const{data,profile,platform}=await fixture(t);const foreign=path.join(profile,'foreign-fixture');await writeFile(foreign,'preserve');await assert.rejects(platform.ensureOwnedProfile(data,profile),/PROFILE_OWNERSHIP_UNPROVEN/);assert.equal(await readFile(foreign,'utf8'),'preserve');
 await rm(foreign);await platform.ensureOwnedProfile(data,profile);await assert.rejects(platform.close({profile,pid:process.pid,binary:process.execPath,started:'forged'}),/SECURITY_BROWSER_IDENTITY_CHANGED/);
});
