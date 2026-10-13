// Real PowerShell discovery with fictional package/process queries only.
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {mkdir,mkdtemp,writeFile,copyFile,readdir,lstat,unlink,rmdir} from 'node:fs/promises';
import {join,resolve} from 'node:path';
const exec=promisify(execFile),scratch=resolve('runtime/shared-native-compatibility');
async function fixture(scenario){
 await mkdir(scratch,{recursive:true});const root=await mkdtemp(join(scratch,'desktop-discovery-'));
 const quote=value=>"'"+value.replaceAll("'","''")+"'";
 try{
  await copyFile(resolve('src-tauri/resources/codex-shared/desktop-runtime.ps1'),join(root,'desktop-runtime.ps1'));
  await writeFile(join(root,'native-runtime.ps1'),`param([string]$Action,[string]$Binary)\n@{version='fixture';binary=$Binary}|ConvertTo-Json -Compress`);
  const helper=join(root,'fixture.ps1');
  await writeFile(helper,`$ErrorActionPreference='Stop'
$sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$package='D:\\WindowsApps\\OpenAI.Codex_fixture'
$native=Join-Path $env:LOCALAPPDATA 'OpenAI/Codex/bin/fixture/codex.exe'
$scenario=${quote(scenario)}
function Get-AppxPackage {@{InstallLocation=$package}}
function Test-Path {param($LiteralPath) return $true}
function Get-CimInstance {param($ClassName,$Filter)
 if($Filter -eq "Name='codex.exe'"){
  if($scenario -eq 'closed'){return @()}
  $rows=@([pscustomobject]@{ProcessId=10;ParentProcessId=20;CommandLine='app-server';ExecutablePath=$native;Owner=$sid})
  if($scenario -eq 'ambiguous'){$rows+=([pscustomobject]@{ProcessId=11;ParentProcessId=21;CommandLine='app-server';ExecutablePath=$native;Owner=$sid})}
  if($scenario -eq 'foreign-backend'){$rows[0].Owner='S-1-5-18'}
  return $rows
 }
 return [pscustomobject]@{ProcessId=20;Name='ChatGPT.exe';ExecutablePath=$(if($scenario -eq 'other-app'){'C:\\Other\\ChatGPT.exe'}else{Join-Path $package 'app/ChatGPT.exe'});Owner=$(if($scenario -eq 'foreign-parent'){'S-1-5-18'}else{$sid})}
}
function Invoke-CimMethod {param($InputObject,$MethodName) @{Sid=$InputObject.Owner}}
& (Join-Path $PSScriptRoot 'desktop-runtime.ps1')
`);
  return JSON.parse((await exec('powershell.exe',['-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',helper],{windowsHide:true,timeout:10000,maxBuffer:16384})).stdout);
 }finally{for(const name of await readdir(root)){assert.equal((await lstat(join(root,name))).isSymbolicLink(),false);await unlink(join(root,name));}await rmdir(root);}
}
test('registered D-drive Desktop uses its exact current cached backend',async()=>{const value=await fixture('running');assert.ok(value.desktop.startsWith('D:\\WindowsApps\\'));assert.ok(value.binary.endsWith('OpenAI\\Codex\\bin\\fixture\\codex.exe'));});
test('foreign owners and lookalike Desktop paths cannot select a backend',async()=>{for(const scenario of ['foreign-parent','foreign-backend','other-app']){const value=await fixture(scenario);assert.equal(value.binary,'D:\\WindowsApps\\OpenAI.Codex_fixture\\app\\resources\\codex.exe');}});
test('closed registered Desktop uses its bundled cohort without guessing caches',async()=>{assert.equal((await fixture('closed')).binary,'D:\\WindowsApps\\OpenAI.Codex_fixture\\app\\resources\\codex.exe');});
test('multiple exact registered owner backends fail closed',async()=>{await assert.rejects(fixture('ambiguous'),e=>e.stderr.includes('SHARED_DESKTOP_BACKEND_UNRESOLVED'));});
