// Compile and execute the production NSIS shortcut macros in a fixture folder.
// No product installation/registration, real Start Menu, Codex or service action.
import assert from 'node:assert/strict';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {mkdir,mkdtemp,writeFile,readFile,readdir,lstat,unlink,rmdir} from 'node:fs/promises';
import {join,resolve,relative} from 'node:path';
const exec=promisify(execFile),scratch=resolve('runtime/shared-native-compatibility');
assert.equal(process.platform,'win32');await mkdir(scratch,{recursive:true});
const directory=await mkdtemp(join(scratch,'shortcut-installer-'));
const programs=join(directory,'Fixture Programs With Spaces'),installed=join(directory,'Installed App With Spaces');
const shortcut=join(programs,'Codex (Agbrio shared).lnk'),foreign=join(programs,'Codex.lnk');
const fixture=join(directory,'fixture.nsi'),binary=join(directory,'shortcut-fixture.exe');
const makensis=join(process.env.LOCALAPPDATA,'tauri/NSIS/makensis.exe');
const quoteNSIS=value=>{assert.ok(!/["$\r\n]/.test(value));return value;};
let verified=false;
async function clean(path){assert.ok(!relative(directory,path).startsWith('..'));for(const name of await readdir(path)){const p=join(path,name),s=await lstat(p);assert.equal(s.isSymbolicLink(),false);if(s.isDirectory())await clean(p);else await unlink(p);}await rmdir(path);}
try{
 await mkdir(programs);await mkdir(join(installed,'codex-shared'),{recursive:true});
 await writeFile(foreign,'unrelated shortcut marker');
 const hook=resolve('src-tauri/brand-installer-hooks.nsh');
 await writeFile(fixture,`Unicode true
RequestExecutionLevel user
SilentInstall silent
Name "Agbrio isolated shortcut validation"
OutFile "${quoteNSIS(binary)}"
!include LogicLib.nsh
!include FileFunc.nsh
!include "${join(process.env.LOCALAPPDATA,'tauri/NSIS/Include/Win/COM.nsh')}"
!define MANUKEY "Software\\AgbrioFixtureNotRegistered"
!define MANUPRODUCTKEY "Software\\AgbrioFixtureNotRegistered\\Product"
!define MAINBINARYNAME "ai-work-router"
!include "${quoteNSIS(hook)}"
Section
 \${GetParameters} $R0
 StrCmp $R0 "/REMOVE" remove
 ClearErrors
 !insertmacro AGBRIO_INSTALL_SHARED_ENTRY "${quoteNSIS(programs)}" "${quoteNSIS(installed)}"
 IfErrors failed done
 remove:
 ClearErrors
 !insertmacro AGBRIO_REMOVE_SHARED_ENTRY "${quoteNSIS(programs)}" "${quoteNSIS(installed)}"
 IfErrors failed done
 failed:
 SetErrorLevel 1
 done:
SectionEnd
`,{encoding:'utf8'});
 await exec(makensis,['/V2',fixture],{windowsHide:true,timeout:30000,maxBuffer:16384});
 await exec(binary,[],{windowsHide:true,timeout:15000});
 assert.ok((await lstat(shortcut)).size>0);
 const recoveryShortcut=join(programs,'Codex (return to normal).lnk');assert.ok((await lstat(recoveryShortcut)).size>0);
 const inspect=join(directory,'inspect.ps1');
 await writeFile(inspect,`param([string]$Shortcut,[string]$ReplacementArguments,[string]$ReplacementTarget)
$ErrorActionPreference='Stop'
$shell=New-Object -ComObject WScript.Shell
try{$link=$shell.CreateShortcut($Shortcut);if($ReplacementArguments){$link.Arguments=$ReplacementArguments;$link.Save()};if($ReplacementTarget){$link.TargetPath=$ReplacementTarget;$link.Save()};@{target=$link.TargetPath;arguments=$link.Arguments;icon=$link.IconLocation}|ConvertTo-Json -Compress}finally{if($link){[void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($link)};[void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($shell)}
`);
 const value=JSON.parse((await exec('pwsh.exe',['-NoProfile','-NonInteractive','-File',inspect,'-Shortcut',shortcut],{windowsHide:true,timeout:15000,maxBuffer:16384})).stdout);
 assert.ok(value.target.toLowerCase().endsWith('\\windowspowershell\\v1.0\\powershell.exe'));
 assert.equal(value.arguments,`-NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File "${join(installed,'codex-shared/launch.ps1')}"`);
 assert.equal(value.icon,join(installed,'ai-work-router.exe')+',0');
 const recovery=JSON.parse((await exec('pwsh.exe',['-NoProfile','-NonInteractive','-File',inspect,'-Shortcut',recoveryShortcut],{windowsHide:true,timeout:15000,maxBuffer:16384})).stdout);
 assert.equal(recovery.arguments,`-NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File "${join(installed,'codex-shared/return-normal.ps1')}"`);
 assert.equal(recovery.target.toLowerCase(),value.target.toLowerCase());
 await exec('pwsh.exe',['-NoProfile','-NonInteractive','-File',inspect,'-Shortcut',shortcut,'-ReplacementArguments','"D:\\Another Installation\\launch.ps1"'],{windowsHide:true,timeout:15000});
 const replaced=await readFile(shortcut);await exec(binary,['/REMOVE'],{windowsHide:true,timeout:15000});assert.deepEqual(await readFile(shortcut),replaced);
 await exec(binary,[],{windowsHide:true,timeout:15000});
 await exec('pwsh.exe',['-NoProfile','-NonInteractive','-File',inspect,'-Shortcut',shortcut,'-ReplacementTarget',join(process.env.WINDIR,'System32/notepad.exe')],{windowsHide:true,timeout:15000});
 const otherTarget=await readFile(shortcut);await exec(binary,['/REMOVE'],{windowsHide:true,timeout:15000});assert.deepEqual(await readFile(shortcut),otherTarget);
 await exec(binary,[],{windowsHide:true,timeout:15000});
 await exec(binary,['/REMOVE'],{windowsHide:true,timeout:15000});
 assert.equal(await lstat(shortcut).catch(e=>{if(e.code==='ENOENT')return null;throw e;}),null);
 assert.equal(await lstat(recoveryShortcut).catch(e=>{if(e.code==='ENOENT')return null;throw e;}),null);
 assert.equal(await readFile(foreign,'utf8'),'unrelated shortcut marker');verified=true;
 console.log(JSON.stringify({compiledNSISShortcutMacros:'PASS',pathWithSpaces:true,quotedLauncherArguments:true,removeOwnShortcut:true,replacedArgumentsPreserved:true,replacedTargetPreserved:true,unrelatedShortcutPreserved:true,realStartMenuChanges:0,productRegistrations:0,desktopStarts:0}));
}finally{if(verified)await clean(directory);else console.error('Shortcut fixture retained for diagnosis: '+directory);}
