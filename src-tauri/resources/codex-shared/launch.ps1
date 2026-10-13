# Explicit installed entry; does not launch Agbrio or change normal Codex startup.
$ErrorActionPreference='Stop'
$node=Join-Path (Split-Path -Parent $PSScriptRoot) 'browser-executor-node.exe'
$directory=[IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'AIWorkRouter/codex-shared'))
$sha=[Security.Cryptography.SHA256]::Create();try{$key=([BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($directory)))).Replace('-','').ToLowerInvariant()}finally{$sha.Dispose()}
$mutex=[Threading.Mutex]::new($false,('Local\Agbrio.SharedCodex.'+$key));$held=$false
try{
 try{$held=$mutex.WaitOne(0)}catch [Threading.AbandonedMutexException]{$held=$true}
 if(!$held){throw 'SHARED_START_UNAVAILABLE'}
 $output=& $node (Join-Path $PSScriptRoot 'launch-entry.mjs') $PSScriptRoot 2>&1
 if($LASTEXITCODE -ne 0){throw 'SHARED_LAUNCH_NOT_COMPLETED'}
}catch{
 Add-Type -AssemblyName System.Windows.Forms
 $code=[regex]::Match(([string]$output+' '+$_.Exception.Message),'SHARED_[A-Z_]+').Value
 $zh=[Globalization.CultureInfo]::CurrentUICulture.Name.StartsWith('zh')
 $message=if($zh){'共享 Codex 未打开。当前 Codex 保持原状，没有重发消息。'}else{'Shared Codex was not opened. Current Codex is unchanged; no messages were resent.'}
 switch -Wildcard ($code){
  'SHARED_DESKTOP_CLOSE_REQUIRED' {$message=if($zh){'普通 Codex 仍在运行，未启动第二个窗口。完成当前任务后，可退出它，再使用这个独立入口打开。'}else{'Ordinary Codex is still running. No second window was started. Finish your work, exit it, then use this independent entry.'}}
  'SHARED_NATIVE_*' {$message=if($zh){'Codex 本地工具验证失败，未切换连接。请继续使用普通 Codex。'}else{'Codex local tool verification failed. The connection was not changed. Keep using ordinary Codex.'}}
  'SHARED_START_UNAVAILABLE' {$message=if($zh){'另一个连接操作正在进行，请等待它完成。当前 Codex 保持原状。'}else{'Another connection operation is in progress. Wait for it to finish. Current Codex is unchanged.'}}
  'SHARED_CONNECTION_UNCERTAIN' {$message=if($zh){'无法确认共享后台状态，未结束进程。请继续使用普通 Codex。'}else{'The shared backend state could not be verified. No process was stopped. Keep using ordinary Codex.'}}
 }
 [void][Windows.Forms.MessageBox]::Show($message,'Agbrio')
 exit 1
}finally{if($held){$mutex.ReleaseMutex()};$mutex.Dispose()}
