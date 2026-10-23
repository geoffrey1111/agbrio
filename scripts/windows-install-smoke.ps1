param([string]$Installer,[string]$InstalledImage)
$ErrorActionPreference='Stop'
$agbrioRoot=Join-Path (Get-Location).Path 'runtime\clean-windows'
$agbrioInstall=Join-Path $agbrioRoot 'install'
$agbrioData=Join-Path $agbrioRoot 'local-appdata'
New-Item -ItemType Directory -Path $agbrioData -Force|Out-Null
if(Get-Process -Name ai-work-router -ErrorAction SilentlyContinue){throw 'Fresh-runner gate: another Host is running.'}
if($Installer){
 $agbrioInstaller=(Resolve-Path -LiteralPath $Installer).Path
 $agbrioSetup=Start-Process -FilePath $agbrioInstaller -ArgumentList @('/S',('/D='+$agbrioInstall)) -WindowStyle Hidden -Wait -PassThru
 if($agbrioSetup.ExitCode -ne 0){throw ('Installer failed '+$agbrioSetup.ExitCode)}
}elseif($InstalledImage){
 # Diagnostic-only reuse of a proven runner-owned installed image. Final release
 # still runs the actual installer on a separate clean Windows runner.
 $agbrioImage=(Resolve-Path -LiteralPath $InstalledImage).Path
 if(!$agbrioImage.StartsWith((Get-Location).Path+[IO.Path]::DirectorySeparatorChar)){throw 'Image must be inside the QA workspace.'}
 Copy-Item -LiteralPath $agbrioImage -Destination $agbrioInstall -Recurse
}else{throw 'Installer or diagnostic InstalledImage required.'}
$agbrioExecutable=Join-Path $agbrioInstall 'ai-work-router.exe'
if(!(Test-Path -LiteralPath $agbrioExecutable)){throw 'Installed application missing.'}
$agbrioStart=[Diagnostics.ProcessStartInfo]::new($agbrioExecutable)
$agbrioStart.UseShellExecute=$false
$agbrioStart.Environment['LOCALAPPDATA']=$agbrioData
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
 # Publish a disposable local board through the installed current-user CLI.
 $boardFolder=Join-Path $agbrioRoot 'board-fixture'
 New-Item -ItemType Directory -Path $boardFolder -Force | Out-Null
 '<!doctype html><html><head><link rel="stylesheet" href="style.css"></head><body><h1>QA published result</h1><p id="parent">Checking isolation</p><p id="native">Checking native API</p><img src="pixel.png" alt="Published diagram" onload="document.getElementById(&quot;image&quot;).textContent=&quot;Published image readable&quot;"><p id="image"></p><script src="app.js"></script></body></html>' | Set-Content (Join-Path $boardFolder 'index.html') -Encoding utf8
 'body{font:18px system-ui;padding:30px;background:#fffcf8;color:#342d27}img{width:32px;height:32px}' | Set-Content (Join-Path $boardFolder 'style.css') -Encoding utf8
 "document.querySelector('h1').textContent='QA board script loaded';try{void parent.document.body;document.getElementById('parent').textContent='Parent escaped'}catch{document.getElementById('parent').textContent='Parent isolated'};(async()=>{try{const v=await Promise.race([window.__TAURI_INTERNALS__.invoke('app_capabilities').then(()=>true),new Promise(r=>setTimeout(()=>r(false),2000))]);document.getElementById('native').textContent=v?'Native API escaped':'Native API isolated'}catch{document.getElementById('native').textContent='Native API isolated'}})();" | Set-Content (Join-Path $boardFolder 'app.js') -Encoding utf8
 [IO.File]::WriteAllBytes((Join-Path $boardFolder 'pixel.png'),[Convert]::FromBase64String('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a0u8AAAAASUVORK5CYII='))
 @{schemaVersion=1;title='QA project board';entry='index.html';files=@('index.html','style.css','app.js','pixel.png')} | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $boardFolder 'agbrio-board.json') -Encoding utf8
 $published=& pwsh -NoProfile -File (Join-Path $agbrioInstall 'app-control/control.ps1') -Command PublishBoard -Folder $boardFolder
 if($LASTEXITCODE -ne 0){throw 'Installed board CLI failed'}
 $publishedBoard=$published | ConvertFrom-Json
 if(!$publishedBoard.id -or !$publishedBoard.revision){throw 'Installed publication receipt missing'}
 $published | Set-Content (Join-Path $agbrioRoot 'published-board-proof.json') -Encoding utf8
 $agbrioWindowType=@'
using System;using System.Runtime.InteropServices;
public static class AgbrioSmokeWindow{[DllImport("user32.dll")]public static extern bool ShowWindow(IntPtr window,int command);}
'@
 Add-Type -TypeDefinition $agbrioWindowType
 for($agbrioAttempt=0;$agbrioAttempt -lt 20;$agbrioAttempt++){
  $agbrioProcess.Refresh()
  if($agbrioProcess.MainWindowHandle -ne [IntPtr]::Zero){break}
  Start-Sleep -Milliseconds 500
 }
 if($agbrioProcess.MainWindowHandle -eq [IntPtr]::Zero){throw 'Native application window did not appear.'}
 [AgbrioSmokeWindow]::ShowWindow($agbrioProcess.MainWindowHandle,3)|Out-Null
 $agbrioWebviews=Get-CimInstance Win32_Process -Filter "Name='msedgewebview2.exe'"
 @($agbrioWebviews)|ForEach-Object{@{pid=$_.ProcessId;parent=$_.ParentProcessId;qaFlags=@([regex]::Matches($_.CommandLine,'--(?:remote-debugging[^ ]*|disable-devtools[^ ]*|enable-features=[^ ]*)')|ForEach-Object Value)}}|ConvertTo-Json -Compress
 $agbrioCompiler=Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
 $agbrioWpf=Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\WPF'
 $agbrioHarness=Join-Path $agbrioRoot 'native-ui-qa.exe'
 & $agbrioCompiler /nologo /target:exe ('/out:'+$agbrioHarness) /r:System.Drawing.dll /r:System.Web.Extensions.dll ('/r:'+(Join-Path $agbrioWpf 'UIAutomationClient.dll')) ('/r:'+(Join-Path $agbrioWpf 'UIAutomationTypes.dll')) ('/r:'+(Join-Path $agbrioWpf 'WindowsBase.dll')) (Join-Path $PSScriptRoot 'windows-uia-smoke.cs')
 if($LASTEXITCODE -ne 0){throw 'Native accessibility harness compile failed.'}
 & $agbrioHarness ($agbrioProcess.MainWindowHandle.ToInt64()) $agbrioRoot
 if($LASTEXITCODE -ne 0){throw 'Fresh installed desktop UI gate failed.'}
}finally{
 # Only this isolated runner-owned process, never an existing user installation.
 if(!$agbrioProcess.HasExited){Stop-Process -Id $agbrioProcess.Id -Force}
}
