# Installed paired recovery entry, independent of the Agbrio Host.
$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.Windows.Forms
$scripts=$PSScriptRoot
$node=Join-Path (Split-Path -Parent $PSScriptRoot) 'browser-executor-node.exe'
$directory=[IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'AIWorkRouter/codex-shared'))
$sha=[Security.Cryptography.SHA256]::Create();try{$key=([BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($directory)))).Replace('-','').ToLowerInvariant()}finally{$sha.Dispose()}
$mutex=[Threading.Mutex]::new($false,('Local\Agbrio.SharedCodex.'+$key));$held=$false
function Invoke-NormalDisable {
 $previousPreference=$ErrorActionPreference
 try{$ErrorActionPreference='Continue';$output=& $node (Join-Path $scripts 'control.mjs') --disable $directory 2>&1;$nativeExit=$LASTEXITCODE}finally{$ErrorActionPreference=$previousPreference}
 if($nativeExit -ne 0){$code=[regex]::Match(([string]($output -join ' ')),'SHARED_[A-Z_]+').Value;if(!$code){$code='SHARED_RECOVERY_NOT_COMPLETED'};throw $code}
}
try{
 try{$held=$mutex.WaitOne(0)}catch [Threading.AbandonedMutexException]{$held=$true}
 if(!$held){throw 'SHARED_START_UNAVAILABLE'}
 if(!(Test-Path -LiteralPath $directory -PathType Container)){$message='共享连接尚未配置。请从原来的官方入口打开普通 Codex。'}
 else{
  try{Invoke-NormalDisable}catch{
   if($_.Exception.Message -cne 'SHARED_CONNECTION_UNCERTAIN'){throw}
   $state=& (Join-Path $scripts 'desktop-state.ps1')|ConvertFrom-Json
   if($state.count -ne 0){throw 'SHARED_DESKTOP_CLOSE_REQUIRED'}
   . (Join-Path $scripts 'shared-offline-recovery.ps1')
   $preview=Get-AgbrioUnavailableSnapshot $directory $scripts
   $answer=[Windows.Forms.MessageBox]::Show('共享后台没有响应。已核对本次共享进程及其子进程身份。确认关闭这些进程并恢复普通启动吗？未完成的共享执行可能被中断；不会重发消息或结束其他 Node。','Agbrio 故障恢复',[Windows.Forms.MessageBoxButtons]::YesNo,[Windows.Forms.MessageBoxIcon]::Warning,[Windows.Forms.MessageBoxDefaultButton]::Button2)
   if($answer -ne [Windows.Forms.DialogResult]::Yes){throw 'SHARED_RECOVERY_CANCELLED'}
   $state=& (Join-Path $scripts 'desktop-state.ps1')|ConvertFrom-Json
   if($state.count -ne 0){throw 'SHARED_DESKTOP_CLOSE_REQUIRED'}
   $null=Stop-AgbrioUnavailableOwner $directory $scripts $preview -OwnerConfirmed
   Invoke-NormalDisable
  }
  $message='共享连接已停用。请从原来的官方入口打开普通 Codex。'
 }
}catch{
 $code=$_.Exception.Message;if($code -notmatch '^SHARED_[A-Z_]+$'){$code='SHARED_RECOVERY_NOT_COMPLETED'}
 $message='共享连接尚未停用 ['+$code+']。请先退出共享 Codex；仍在运行的任务需先结束。取消或身份不明时不会强制结束进程。'
}finally{if($held){$mutex.ReleaseMutex()};$mutex.Dispose()}
[void][Windows.Forms.MessageBox]::Show($message,'Agbrio')