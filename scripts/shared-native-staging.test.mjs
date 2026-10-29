// Tiny text-only fixtures for the bounded native staging cleanup.
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {mkdir,mkdtemp,writeFile,readFile,readdir,unlink,rmdir,lstat,copyFile} from 'node:fs/promises';
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

async function pinFixture(scenario){
 await mkdir(scratch,{recursive:true});const root=await mkdtemp(join(scratch,'native-pin-order-')),source=join(root,'source'),dest=join(root,'owned');await mkdir(source);for(const n of ['codex.exe','codex-code-mode-host.exe','codex-command-runner.exe','codex-windows-sandbox-setup.exe'])await writeFile(join(source,n),'fictional signed bytes '+n);
 const q=v=>"'"+v.replaceAll("'","''")+"'",script=join(root,'probe.ps1'),log=join(root,'executed.txt');
 await writeFile(script,`$ErrorActionPreference='Stop'
. ${q(resolve('src-tauri/resources/codex-shared/native-runtime.ps1'))}
$source=${q(source)};$dest=${q(dest)};$log=${q(log)};$scenario=${q(scenario)}
function Get-AuthenticodeSignature {param($LiteralPath) @{Status=$(if($scenario -eq 'bad-signature' -and (Split-Path -Leaf $LiteralPath) -eq 'codex-code-mode-host.exe'){'NotTrusted'}else{'Valid'});SignerCertificate=@{Subject='CN=OpenAI, O="OpenAI, LLC"';Thumbprint='fixture-signer'}}}
function Read-NativeCohort([string]$Path){if([IO.Path]::GetFullPath($Path) -eq [IO.Path]::GetFullPath($source)){throw 'FIXTURE_SOURCE_EXECUTION_ACCESS_DENIED'};Add-Content -LiteralPath $log -Value $Path;$files=Read-NativeFiles $Path;@{format=1;version='fixture';signer=$files.signer;files=$files.files}}
try{$result=Pin-NativeCohort $source $dest;if($scenario -eq 'tampered-cache'){Set-Content -LiteralPath $log -Value '';Set-Content -LiteralPath $result.binary -Value 'changed bytes';try{Pin-NativeCohort $source $dest|Out-Null;throw 'EXPECTED_REJECTION'}catch{if($_.Exception.Message -ne 'SHARED_NATIVE_COHORT_MISMATCH'){throw}}};@{passed=$true;binary=$result.binary;probes=@(Get-Content -LiteralPath $log -ErrorAction SilentlyContinue|Where-Object{$_}).Count}|ConvertTo-Json -Compress}catch{if($scenario -eq 'bad-signature' -and $_.Exception.Message -eq 'SHARED_NATIVE_SIGNATURE_INVALID'){@{passed=$true;probes=0;sourceRejected=$true}|ConvertTo-Json -Compress}else{throw}}
`);
 try{return JSON.parse((await exec('powershell.exe',['-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',script],{windowsHide:true,timeout:15000})).stdout);}finally{await clean(root);}
}
test('pinning verifies then materializes before executing; inaccessible package-source execution is never attempted',async()=>{const r=await pinFixture('source-denied');assert.equal(r.passed,true);assert.equal(r.probes,2);assert.ok(r.binary.includes('native'));});
test('invalid source signature and a modified retained cohort both reject before any execution',async()=>{const source=await pinFixture('bad-signature');assert.equal(source.sourceRejected,true);const altered=await pinFixture('tampered-cache');assert.equal(altered.passed,true);assert.equal(altered.probes,0);});

test('failed entry releases its mutex before displaying the error',async()=>{
 await mkdir(scratch,{recursive:true});const root=await mkdtemp(join(scratch,'failure-dialog-lock-'));const shared=join(root,'codex-shared');await mkdir(shared);await copyFile(process.execPath,join(root,'browser-executor-node.exe'));await writeFile(join(shared,'launch-entry.mjs'),"process.stderr.write('SHARED_NATIVE_EXECUTION_DENIED');process.exitCode=1;");
 const probe=join(root,'mutex.ps1');await writeFile(probe,"param([string]$Name) $m=[Threading.Mutex]::new($false,$Name);$held=$m.WaitOne(0);if($held){$m.ReleaseMutex()};$m.Dispose();if(!$held){exit 9}");
 let launch=await readFile(resolve('src-tauri/resources/codex-shared/launch.ps1'),'utf8');launch=launch.replace("[void][Windows.Forms.MessageBox]::Show($message,'Agbrio')","& powershell.exe -NoProfile -NonInteractive -File (Join-Path (Split-Path -Parent $PSScriptRoot) 'mutex.ps1') ('Local\\Agbrio.SharedCodex.'+$key);if($LASTEXITCODE -ne 0){throw 'ERROR_DIALOG_STILL_HOLDS_LOCK'};Write-Output 'ERROR_DIALOG_MUTEX_RELEASED'");await writeFile(join(shared,'launch.ps1'),launch);
 try{await assert.rejects(exec('powershell.exe',['-NoProfile','-NonInteractive','-File',join(shared,'launch.ps1')],{windowsHide:true,timeout:15000,env:{...process.env,LOCALAPPDATA:join(root,'isolated-local')}}),e=>e.code===1&&e.stdout.includes('ERROR_DIALOG_MUTEX_RELEASED')&&!e.stderr.includes('ERROR_DIALOG_STILL_HOLDS_LOCK'));}finally{await clean(root);}
});
