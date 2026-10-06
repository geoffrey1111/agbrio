param([Parameter(Mandatory=$true)][string]$Installer)
$ErrorActionPreference='Stop'
$agbrioRoot=Join-Path (Get-Location).Path 'runtime\clean-windows'
$agbrioInstall=Join-Path $agbrioRoot 'install'
$agbrioData=Join-Path $agbrioRoot 'local-appdata'
New-Item -ItemType Directory -Path $agbrioData -Force|Out-Null
if(Get-Process -Name ai-work-router -ErrorAction SilentlyContinue){throw 'Fresh-runner gate: another Host is running.'}
$agbrioInstaller=(Resolve-Path -LiteralPath $Installer).Path
$agbrioSetup=Start-Process -FilePath $agbrioInstaller -ArgumentList @('/S',('/D='+$agbrioInstall)) -WindowStyle Hidden -Wait -PassThru
if($agbrioSetup.ExitCode -ne 0){throw ('Installer failed '+$agbrioSetup.ExitCode)}
$agbrioExecutable=Join-Path $agbrioInstall 'ai-work-router.exe'
if(!(Test-Path -LiteralPath $agbrioExecutable)){throw 'Installed application missing.'}
$agbrioStart=[Diagnostics.ProcessStartInfo]::new($agbrioExecutable)
$agbrioStart.UseShellExecute=$false
$agbrioStart.Environment['LOCALAPPDATA']=$agbrioData
$agbrioStart.Environment['WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS']='--remote-debugging-port=9227'
$agbrioStart.Environment['WEBVIEW2_USER_DATA_FOLDER']=(Join-Path $agbrioRoot 'webview-profile')
foreach($agbrioKey in @($agbrioStart.Environment.Keys)){
 if($agbrioKey.StartsWith('AI_WORK_ROUTER_')){$agbrioStart.Environment.Remove($agbrioKey)|Out-Null}
}
$agbrioStart.Environment['AI_WORK_ROUTER_MOBILE_PORT']='47114'
$agbrioProcess=[Diagnostics.Process]::Start($agbrioStart)
try{
 $agbrioDeadline=(Get-Date).AddSeconds(60)
 do{
  if($agbrioProcess.HasExited){throw 'Fresh installed application exited during startup.'}
  try{$agbrioSession=Invoke-RestMethod 'http://127.0.0.1:47114/v1/mobile/auth/session';break}catch{Start-Sleep -Milliseconds 500}
 }while((Get-Date) -lt $agbrioDeadline)
 if(!$agbrioSession){throw 'Fresh installed Host did not start.'}
 @'
using System;using System.Runtime.InteropServices;
public static class AgbrioSmokeWindow{[DllImport("user32.dll")]public static extern bool ShowWindow(IntPtr window,int command);}
'@|Add-Type
 for($agbrioAttempt=0;$agbrioAttempt -lt 20;$agbrioAttempt++){
  $agbrioProcess.Refresh()
  if($agbrioProcess.MainWindowHandle -ne [IntPtr]::Zero){break}
  Start-Sleep -Milliseconds 500
 }
 if($agbrioProcess.MainWindowHandle -eq [IntPtr]::Zero){throw 'Native application window did not appear.'}
 [AgbrioSmokeWindow]::ShowWindow($agbrioProcess.MainWindowHandle,3)|Out-Null
 node scripts/windows-ui-smoke.mjs
 if($LASTEXITCODE -ne 0){throw 'Fresh installed desktop UI gate failed.'}
}finally{
 # Only this isolated runner-owned process, never an existing user installation.
 if(!$agbrioProcess.HasExited){Stop-Process -Id $agbrioProcess.Id -Force}
}
