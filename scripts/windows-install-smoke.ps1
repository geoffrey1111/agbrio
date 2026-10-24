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
 # Reproduce upgrade mode without /D: registry must keep the existing directory,
 # missing branded entries must be recreated and legacy/pinned links retargeted.
 $qaDesktop=[Environment]::GetFolderPath('DesktopDirectory')
 $qaPrograms=[Environment]::GetFolderPath('Programs')
 $qaShortcut=Join-Path $qaDesktop 'Agbrio.lnk'
 $qaMenu=Join-Path $qaPrograms 'Agbrio/Agbrio.lnk'
 $qaOld=Join-Path $agbrioRoot 'Legacy App With Spaces'
 New-Item -ItemType Directory -Path $qaOld -Force|Out-Null
 [IO.File]::WriteAllText((Join-Path $qaOld 'ai-work-router.exe'),'fictional old install; never executed')
 $qaLegacyKey='HKCU:\Software\geoffrey\AI Work Router'
 New-Item -Path $qaLegacyKey -Force|Out-Null
 Set-Item -LiteralPath $qaLegacyKey -Value $qaOld
 $qaShell=New-Object -ComObject WScript.Shell
 $qaStale=Join-Path $qaDesktop 'AI Work Router.lnk'
 $qaLink=$qaShell.CreateShortcut($qaStale);$qaLink.TargetPath=Join-Path $qaOld 'ai-work-router.exe';$qaLink.Arguments='--fixture-preserve';$qaLink.Save()
 foreach($qaPath in @($qaShortcut,$qaMenu)){if(Test-Path -LiteralPath $qaPath){Remove-Item -LiteralPath $qaPath}}
 $qaUpgrade=Start-Process -FilePath $agbrioInstaller -ArgumentList @('/S','/UPDATE') -WindowStyle Hidden -Wait -PassThru
 if($qaUpgrade.ExitCode -ne 0){throw 'UPDATE_MODE_INSTALL_FAILED'}
 foreach($qaPath in @($qaShortcut,$qaMenu,$qaStale)){
  if(!(Test-Path -LiteralPath $qaPath)){throw 'UPDATE_MISSING_LAUNCH_ENTRY'}
  $qaLink=$qaShell.CreateShortcut($qaPath)
  if($qaLink.TargetPath -ine (Join-Path $agbrioInstall 'ai-work-router.exe')){throw 'UPDATE_LAUNCHES_WRONG_COPY'}
 }
 if($qaShell.CreateShortcut($qaStale).Arguments -cne '--fixture-preserve'){throw 'UPDATE_DROPPED_ARGUMENTS'}

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
 # Verify the actual installed reminder payload and route, without registering
 # a wait, creating a credential or contacting any native provider.
 $agbrioPlugin=Join-Path $agbrioInstall 'claude-code-plugin'
 $agbrioPluginManifest=Get-Content -LiteralPath (Join-Path $agbrioPlugin '.claude-plugin/plugin.json') -Raw | ConvertFrom-Json
 $agbrioExpectedPluginVersion=(Get-Content -LiteralPath 'tools/claude-code-plugin/.claude-plugin/plugin.json' -Raw | ConvertFrom-Json).version
if($agbrioPluginManifest.version -cne $agbrioExpectedPluginVersion){throw 'Installed Claude plugin version mismatch'}
 if(!(Test-Path -LiteralPath (Join-Path $agbrioPlugin 'hooks/codex-wait.mjs'))){throw 'Installed Codex wait module missing'}
 $agbrioProbe=@{sessionId='10000000-0000-4000-8000-000000000001';instanceId='20000000-0000-4000-8000-000000000001';cwd=$agbrioRoot;threadId='30000000-0000-4000-8000-000000000001';generation=1;turnId='40000000-0000-4000-8000-000000000001'} | ConvertTo-Json
 $agbrioWaitResponse=Invoke-WebRequest -Uri 'http://127.0.0.1:47114/v1/claude-code/codex-wait' -Method Post -ContentType 'application/json' -Body $agbrioProbe -SkipHttpErrorCheck
 if([int]$agbrioWaitResponse.StatusCode -ne 401){throw 'Installed Codex wait route missing or accepts unauthenticated requests'}
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

if($Installer){
 # Actual shell entry -> application -> exact version, after the initial app
 # has exited. No provider credentials or model activity on this fresh runner.
 Start-Process -FilePath $qaShortcut | Out-Null
 $qaDeadline=(Get-Date).AddSeconds(45)
 $qaRelaunched=$null
 do{
  Start-Sleep -Milliseconds 500
  $qaRelaunched=Get-Process -Name ai-work-router -ErrorAction SilentlyContinue | Where-Object {$_.Path -ieq $agbrioExecutable} | Select-Object -First 1
 }while(!$qaRelaunched -and (Get-Date) -lt $qaDeadline)
 if(!$qaRelaunched){throw 'DESKTOP_SHORTCUT_RELAUNCH_FAILED'}
 try{
  $qaControl=Join-Path $agbrioInstall 'app-control/control.ps1'
  $qaStatus=$null
  do{
   try{$qaStatus=(& $qaControl -Command Status | ConvertFrom-Json);if($qaStatus.state -eq 'RUNNING'){break}}catch{}
   Start-Sleep -Milliseconds 500
  }while((Get-Date) -lt $qaDeadline)
  $qaExpected=(Get-Content docs/RELEASE_BUILD.json -Raw | ConvertFrom-Json).version
  if($qaStatus.appVersion -cne $qaExpected -or $qaStatus.state -cne 'RUNNING'){throw 'SHORTCUT_RELAUNCHED_OLD_OR_UNHEALTHY_VERSION'}
  @{upgradeWithoutExplicitDirectory=$true;desktopRecreated=$true;startMenuRecreated=$true;legacyRetargeted=$true;actualDesktopRelaunch=$true;version=$qaStatus.appVersion} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $agbrioRoot 'entrypoints-proof.json')
  & $qaControl -Command Stop -Executable $agbrioExecutable -ExpectedInstance $qaStatus.instanceId | Out-Null
 }finally{
  $qaRelaunched.Refresh();if(!$qaRelaunched.HasExited){Stop-Process -Id $qaRelaunched.Id}
 }
}
