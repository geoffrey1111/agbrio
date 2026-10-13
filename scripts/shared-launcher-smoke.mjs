// Executes the real Windows PowerShell launcher in an isolated installed layout.
// Only read-only entry resources are present: missing native/runtime files fence
// any activation even if the ordinary Desktop closes during the test.
import assert from 'node:assert/strict';
import {spawn,execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {mkdir,mkdtemp,copyFile,writeFile,readFile,readdir,lstat,unlink,rmdir} from 'node:fs/promises';
import {join,resolve,relative} from 'node:path';
const exec=promisify(execFile),source=resolve('src-tauri/resources/codex-shared');
const scratch=resolve('runtime/shared-native-compatibility'),ps=join(process.env.WINDIR,'System32/WindowsPowerShell/v1.0/powershell.exe');
assert.equal(process.platform,'win32');
const state=JSON.parse((await exec(ps,['-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',join(source,'desktop-state.ps1')],{windowsHide:true,timeout:15000})).stdout);
assert.ok(state.count>0,'Ordinary Desktop must already be open; do not launch it for this test');
await mkdir(scratch,{recursive:true});const directory=await mkdtemp(join(scratch,'launcher-layout-'));
const layout=join(directory,'Installed App With Spaces'),resources=join(layout,'codex-shared'),localData=join(directory,'Local Data');
const production=join(process.env.LOCALAPPDATA,'AIWorkRouter/codex-shared');
const snapshots=await Promise.all(['settings.json','runtime-config.json'].map(n=>readFile(join(production,n)).catch(e=>{if(e.code==='ENOENT')return null;throw e;})));
let child,closed,verified=false;
async function clean(path){assert.ok(!relative(directory,path).startsWith('..'));for(const name of await readdir(path)){const p=join(path,name),s=await lstat(p);assert.equal(s.isSymbolicLink(),false);if(s.isDirectory())await clean(p);else await unlink(p);}await rmdir(path);}
try{
 await mkdir(resources,{recursive:true});await mkdir(localData);
 await copyFile(process.execPath,join(layout,'browser-executor-node.exe'));
 for(const name of ['launch.ps1','launch-entry.mjs','control.mjs','desktop-state.ps1'])await copyFile(join(source,name),join(resources,name));
 const helper=join(directory,'inspect-dialog.ps1');
 await writeFile(helper,`param([uint32]$OwnedPid,[string]$Expected)
$ErrorActionPreference='Stop'
Add-Type -TypeDefinition @'
using System;using System.Text;using System.Collections.Generic;using System.Runtime.InteropServices;
public static class AgbrioFixtureDialog {
 public delegate bool EnumProc(IntPtr h,IntPtr p);
 [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc cb,IntPtr p);
 [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr h,EnumProc cb,IntPtr p);
 [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h,out uint pid);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr h,StringBuilder b,int n);
 [DllImport("user32.dll")] static extern int GetDlgCtrlID(IntPtr h);
 [DllImport("user32.dll")] static extern IntPtr SendMessage(IntPtr h,uint m,IntPtr w,IntPtr l);
 public static string InspectAndDismiss(uint pid){string result=null;
  EnumWindows((h,p)=>{uint owner;GetWindowThreadProcessId(h,out owner);var title=new StringBuilder(1024);GetWindowText(h,title,title.Capacity);
   if(owner!=pid||title.ToString()!="Agbrio")return true;
   var texts=new List<string>();IntPtr ok=IntPtr.Zero;
   EnumChildWindows(h,(c,q)=>{uint childOwner;GetWindowThreadProcessId(c,out childOwner);if(childOwner!=pid)return true;
    var text=new StringBuilder(4096);GetWindowText(c,text,text.Capacity);if(text.Length>0)texts.Add(text.ToString());if(GetDlgCtrlID(c)==1)ok=c;return true;},IntPtr.Zero);
   result=string.Join("\\n",texts);if(ok!=IntPtr.Zero)SendMessage(ok,0xf5,IntPtr.Zero,IntPtr.Zero);return false;
  },IntPtr.Zero);return result;
 }
}
'@
$until=[DateTime]::UtcNow.AddSeconds(20);$text=$null
while([DateTime]::UtcNow -lt $until){$text=[AgbrioFixtureDialog]::InspectAndDismiss($OwnedPid);if($null -ne $text){break};Start-Sleep -Milliseconds 100}
if($null -eq $text){throw 'FIXTURE_DIALOG_NOT_FOUND'}
@{expected=$text.Contains($Expected);text=$text}|ConvertTo-Json -Compress
`,{encoding:'utf8'});
 const results=[];
 for(const [culture,expected] of [['en-US','Ordinary Codex is still running.'],['zh-CN','普通 Codex 仍在运行']]){
  const launcher=join(resources,'launch.ps1');
  const command=`[Threading.Thread]::CurrentThread.CurrentUICulture=[Globalization.CultureInfo]::GetCultureInfo('${culture}'); & '${launcher.replaceAll("'","''")}'`;
  child=spawn(ps,['-NoProfile','-NonInteractive','-WindowStyle','Hidden','-ExecutionPolicy','Bypass','-Command',command],{windowsHide:true,stdio:'ignore',env:{...process.env,LOCALAPPDATA:localData}});
  closed=new Promise((yes,no)=>{child.once('error',no);child.once('close',code=>yes(code));});
  // Encoding the helper argument avoids a Windows code-page conversion in the
  // inspector; the production launcher is loaded normally with -File semantics.
  const inspect=`& '${helper.replaceAll("'","''")}' -OwnedPid ${child.pid} -Expected '${expected}'`;
  const value=JSON.parse((await exec(ps,['-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-EncodedCommand',Buffer.from(inspect,'utf16le').toString('base64')],{windowsHide:true,timeout:30000,maxBuffer:16384})).stdout);
  assert.equal(await closed,1);child=null;
  assert.equal(value.expected,true,`Incorrect ${culture} dialog: ${value.text}`);results.push({culture,ordinaryDesktopGuard:'PASS'});
 }
 assert.equal(await lstat(join(localData,'AIWorkRouter')).catch(e=>{if(e.code==='ENOENT')return null;throw e;}),null,'Launcher must not create a shared runtime');
 const after=await Promise.all(['settings.json','runtime-config.json'].map(n=>readFile(join(production,n)).catch(e=>{if(e.code==='ENOENT')return null;throw e;})));
 assert.deepEqual(after,snapshots);verified=true;
 console.log(JSON.stringify({realPowerShellLauncher:'PASS',pathWithSpaces:true,dialogs:results,productionConfigUnchanged:true,desktopStarts:0,modelRequests:0,temporaryNodeBytes:(await lstat(join(layout,'browser-executor-node.exe'))).size}));
}finally{
 if(child){child.kill();await closed;child=null;} // Only this test's direct child.
 if(verified)await clean(directory);
 else console.error('Launcher fixture retained for diagnosis: '+directory);
}
