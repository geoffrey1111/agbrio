// Actual registered WindowsApps -> byte retention -> automatic native Code Mode.
// No Desktop starts, credentials/profile copies, real conversations or model calls.
import assert from 'node:assert/strict';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {mkdir,mkdtemp,readdir,lstat,unlink,rmdir} from 'node:fs/promises';
import {join,resolve} from 'node:path';
const exec=promisify(execFile),source=resolve('src-tauri/resources/codex-shared'),scratch=resolve('runtime/shared-native-compatibility'),ps=join(process.env.WINDIR,'System32/WindowsPowerShell/v1.0/powershell.exe');
assert.equal(process.platform,'win32');await mkdir(scratch,{recursive:true});const root=await mkdtemp(join(scratch,'bundled-native-'));let safeToClean=false;
const quote=s=>"'"+s.replaceAll("'","''")+"'";
async function discard(path){for(const e of await readdir(path)){const p=join(path,e),s=await lstat(p);assert.equal(s.isSymbolicLink(),false);if(s.isDirectory())await discard(p);else await unlink(p);}await rmdir(path);}
try{
 const discovery=`$ErrorActionPreference='Stop';$p=@(Get-AppxPackage -Name OpenAI.Codex);if($p.Count -ne 1){throw 'SHARED_DESKTOP_BINARY_UNRESOLVED'};$b=Join-Path $p[0].InstallLocation 'app/resources/codex.exe';@{binary=$b;encrypted=([bool]((Get-Item -LiteralPath $b).Attributes -band [IO.FileAttributes]::Encrypted))}|ConvertTo-Json -Compress`;
 const native=JSON.parse((await exec(ps,['-NoProfile','-NonInteractive','-Command',discovery],{windowsHide:true,timeout:15000})).stdout);
 const pinned=JSON.parse((await exec(ps,['-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',join(source,'native-runtime.ps1'),'-Action','Pin','-Binary',native.binary,'-Directory',root],{windowsHide:true,timeout:120000,maxBuffer:262144})).stdout);
 assert.equal(pinned.cohortVerified,true);assert.equal(pinned.files.length,4);
 const metadata=`$ErrorActionPreference='Stop';@{encrypted=([bool]((Get-Item -LiteralPath ${quote(pinned.binary)}).Attributes -band [IO.FileAttributes]::Encrypted))}|ConvertTo-Json -Compress`;
 assert.equal(JSON.parse((await exec(ps,['-NoProfile','-NonInteractive','-Command',metadata],{windowsHide:true})).stdout).encrypted,false);
 const result=await exec(process.execPath,[resolve('scripts/shared-gateway-code-mode-smoke.mjs'),pinned.binary,'--stop-with-native-command','--execute-auto-host'],{windowsHide:true,timeout:120000,maxBuffer:262144});
 const gate=JSON.parse(result.stdout.trim().split('\n').at(-1));assert.equal(gate.nativeAutomaticHostExecution,'PASS');assert.equal(gate.ownedRuntimeStopped,true);assert.equal(gate.productionSettingsUnchanged,true);assert.equal(gate.modelRequests,0);assert.equal(gate.desktopStarts,0);safeToClean=true;
 console.log(JSON.stringify({bundledNativeRetention:'PASS',sourceKind:'REGISTERED_WINDOWS_APPS',sourceEncryptedAttribute:native.encrypted,copiedEncryptionMetadata:false,signatureAndFourHashesVerified:true,version:pinned.version,nativeAutomaticHostExecution:gate.nativeAutomaticHostExecution,automaticJavaScriptCells:gate.automaticJavaScriptCells,capturedNativeDescendants:gate.capturedNativeDescendants,modelRequests:0,desktopStarts:0,productionSettingsUnchanged:true}));
}finally{if(safeToClean)await discard(root);else console.error('Bundled fixture retained for diagnosis: '+root);}
