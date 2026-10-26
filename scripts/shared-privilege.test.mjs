import {test} from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp,writeFile,unlink,rmdir} from 'node:fs/promises';
import {resolve,join,dirname} from 'node:path';
import {launchPreference,requiredPrivilege,dispatchEntry,dispatchLifecycle,preferredPowerShell} from '../src-tauri/resources/codex-shared/privilege.mjs';
import {mkdir} from 'node:fs/promises';
import {readFile} from 'node:fs/promises';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {rm} from 'node:fs/promises';

test('shared helpers prefer installed PowerShell 7 with a fixed Windows fallback and no PATH search',()=>{
 const env={ProgramFiles:'C:/Program Files',ProgramW6432:'D:/Program Files',WINDIR:'C:/Windows'};
 const core=join(env.ProgramW6432,'PowerShell/7/pwsh.exe');
 assert.equal(preferredPowerShell(env,p=>p===core),core);
 assert.equal(preferredPowerShell(env,()=>false),join(env.WINDIR,'System32/WindowsPowerShell/v1.0/powershell.exe'));
 assert.equal(preferredPowerShell({WINDIR:'C:/Windows'},()=>true),join(env.WINDIR,'System32/WindowsPowerShell/v1.0/powershell.exe'));
});

test('real PowerShell hosts apply the same protected owner ACL for preparation',async()=>{
 if(process.platform!=='win32')return;
 const parent=resolve('runtime/shared-cli-recovery-20261026');await mkdir(parent,{recursive:true});
 const root=await mkdtemp(join(parent,'acl-'));
 try{
  const hosts=new Set([preferredPowerShell(),join(process.env.WINDIR,'System32/WindowsPowerShell/v1.0/powershell.exe')]);
  for(const host of hosts){
   const output=await promisify(execFile)(host,['-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',resolve('src-tauri/resources/codex-shared/environment.ps1'),'-Action','Protect','-Directory',root],{windowsHide:true,timeout:20000});
   assert.equal(JSON.parse(output.stdout).ok,true);
   const path=root.replaceAll("'","''");const query=`$a=if($PSVersionTable.PSEdition -eq 'Core'){[IO.FileSystemAclExtensions]::GetAccessControl([IO.DirectoryInfo]::new('${path}'))}else{[IO.Directory]::GetAccessControl('${path}')};@{protected=$a.AreAccessRulesProtected;rules=@($a.GetAccessRules($true,$false,[Security.Principal.SecurityIdentifier])).Count}|ConvertTo-Json -Compress`;
   const acl=JSON.parse((await promisify(execFile)(host,['-NoProfile','-NonInteractive','-Command',query],{windowsHide:true,timeout:20000})).stdout);
   assert.equal(acl.protected,true);assert.equal(acl.rules,3);
  }
 }finally{await rmdir(root);}
});

test('the real control CLI settles its own readonly lifecycle instead of awaiting its own module',async()=>{
 const parent=resolve('runtime/shared-cli-recovery-20261026');await mkdir(parent,{recursive:true});const root=await mkdtemp(join(parent,'control-cli-'));
 const cli=resolve('src-tauri/resources/codex-shared/control.mjs'),input=join(root,'request.json');
 try{
  await writeFile(input,JSON.stringify({action:'status',directory:root,scripts:root}));
  const result=await promisify(execFile)(process.execPath,[cli,input],{windowsHide:true,timeout:10000});
  assert.equal(JSON.parse(result.stdout).state,'INDEPENDENT');assert.doesNotMatch(result.stderr,/unsettled top-level await/);
  await writeFile(join(root,'settings.json'),'{invalid');
  await assert.rejects(promisify(execFile)(process.execPath,[cli,input],{windowsHide:true,timeout:10000}),error=>error.code===1&&error.stderr==='SHARED_SETTINGS_INVALID');
  await unlink(join(root,'settings.json'));
  if(process.platform==='win32'){
   await writeFile(join(root,'desktop-state.ps1'),`'${JSON.stringify({desktop:'C:/fixture/ChatGPT.exe',count:1})}'`);
   await writeFile(input,JSON.stringify({action:'prepare-check',directory:root,scripts:root}));
   await assert.rejects(promisify(execFile)(process.execPath,[cli,input],{windowsHide:true,timeout:15000}),error=>error.code===1&&error.stderr==='SHARED_DESKTOP_CLOSE_REQUIRED');
  }
 }finally{assert.equal(resolve(dirname(root)),parent);await rm(root,{recursive:true,force:true});}
});
test('native UAC wrapper compiles with the correct Win32 structure without requesting elevation',async()=>{
 const text=await readFile(resolve('src-tauri/resources/codex-shared/privilege.ps1'),'utf8');
 const code=text.match(/Add-Type -TypeDefinition @'\r?\n([\s\S]*?)\r?\n'@/)[1];
 const quoted="'"+code.replaceAll("'","''")+"'";
 const ps=join(process.env.WINDIR,'System32/WindowsPowerShell/v1.0/powershell.exe');
 const command=`Add-Type -TypeDefinition ${quoted};$t=[AgbrioPrivilegeShell].GetNestedType('ExecuteInfo',[Reflection.BindingFlags]::NonPublic);@{compiled=$true;size=[Runtime.InteropServices.Marshal]::SizeOf([type]$t);pointer=[IntPtr]::Size}|ConvertTo-Json -Compress`;
 const result=JSON.parse((await promisify(execFile)(ps,['-NoProfile','-NonInteractive','-Command',command],{windowsHide:true,timeout:15000})).stdout);
 assert.equal(result.compiled,true);assert.equal(result.size,result.pointer===8?112:60);
});
test('missing preferences default to administrator and invalid values never silently select a token',async()=>{
 const parent=resolve('runtime/shared-admin-20261025');await mkdir(parent,{recursive:true});const root=await mkdtemp(join(parent,'preference-'));
 try{assert.equal(await launchPreference(root),true);for(const value of [false,true]){await writeFile(join(root,'launch-preferences.json'),JSON.stringify({launchAsAdministrator:value}));assert.equal(await launchPreference(root),value);}for(const value of [{launchAsAdministrator:'false'},{launchAsAdministrator:false,command:'arbitrary'},null]){await writeFile(join(root,'launch-preferences.json'),JSON.stringify(value));await assert.rejects(launchPreference(root),/PREFERENCES_INVALID/);}}
 finally{await unlink(join(root,'launch-preferences.json'));await rmdir(root);}
});
test('running cohorts never change tokens or become silently replaced',()=>{
 for(const desired of [true,false]){
  assert.equal(requiredPrivilege('setup',desired,!desired,{elevated:!desired}),!desired);
  assert.equal(requiredPrivilege('launch',desired,!desired,{elevated:!desired,desktopConnected:true}),!desired);
  assert.throws(()=>requiredPrivilege('launch',desired,desired,{elevated:!desired,desktopConnected:false}),/RESTART_REQUIRED/);
  assert.equal(requiredPrivilege('disable',desired,!desired,{elevated:!desired}),!desired);
  assert.equal(requiredPrivilege('setup',desired,!desired,null),desired);
 }
});
test('the independent entry checks ordinary Desktop and running-token mismatch before any UAC request',async()=>{
 const base={inspect:async()=>({elevated:false}),preference:async()=>true,runtime:async()=>null,desktop:async()=>({count:1}),submit:()=>assert.fail('No elevation before preflight')};
 await assert.rejects(dispatchEntry({directory:'fixture',source:'fixture'},()=>assert.fail('No launch'),base),/DESKTOP_CLOSE_REQUIRED/);
 await assert.rejects(dispatchEntry({directory:'fixture',source:'fixture'},()=>assert.fail('No launch'),{...base,runtime:async()=>({elevated:false,desktopConnected:false,activeTurns:1})}),/RESTART_REQUIRED/);
 let reused=0;await dispatchEntry({directory:'fixture'},async()=>{reused++;},{...base,runtime:async()=>({elevated:false,desktopConnected:true})});assert.equal(reused,1);
});
test('even a matching token uses the serialized wrapper for mutable control operations',async()=>{
 for(const elevated of [true,false]){let jobs=0;const value=await dispatchLifecycle({action:'setup',directory:'fixture'},{inspect:async()=>({elevated}),preference:async()=>elevated,runtime:async()=>null,run:()=>assert.fail('Do not bypass the cross-process mutation mutex'),submit:async job=>{jobs++;assert.equal(job.elevated,elevated);return 'SERIALIZED';}});assert.equal(value,'SERIALIZED');assert.equal(jobs,1);}
});
