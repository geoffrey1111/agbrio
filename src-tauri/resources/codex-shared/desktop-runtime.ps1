param([string]$Directory,[string]$ProbeNode)
$ErrorActionPreference='Stop'
$apps=@(Get-AppxPackage -Name 'OpenAI.Codex')
if($apps.Count -ne 1 -or !$apps[0].InstallLocation){throw 'SHARED_DESKTOP_BINARY_UNRESOLVED'}
$desktop=Join-Path $apps[0].InstallLocation 'app/ChatGPT.exe'
if(!(Test-Path -LiteralPath $desktop)){throw 'SHARED_DESKTOP_BINARY_UNRESOLVED'}
$owner=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$prefix=Join-Path $env:LOCALAPPDATA 'OpenAI/Codex/bin'
# Appx installs can live on another drive. Match the registered Desktop and its
# owner rather than a guessed WindowsApps prefix or another user's process.
$backends=@(Get-CimInstance Win32_Process -Filter "Name='codex.exe'" | Where-Object {$_.CommandLine -match '\bapp-server\b' -and $_.ExecutablePath -and $_.ExecutablePath.StartsWith($prefix+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)} | Where-Object {
 $parent=Get-CimInstance Win32_Process -Filter ('ProcessId='+$_.ParentProcessId)
 $parent -and $parent.Name -eq 'ChatGPT.exe' -and [string]::Equals($parent.ExecutablePath,$desktop,[StringComparison]::OrdinalIgnoreCase) -and (Invoke-CimMethod -InputObject $parent -MethodName GetOwnerSid).Sid -ceq $owner -and (Invoke-CimMethod -InputObject $_ -MethodName GetOwnerSid).Sid -ceq $owner
})
if($backends.Count -gt 1){throw 'SHARED_DESKTOP_BACKEND_UNRESOLVED'}
if($backends.Count -eq 1){$binary=$backends[0].ExecutablePath}else{$binary=Join-Path $apps[0].InstallLocation 'app/resources/codex.exe'}
$inspection=& (Join-Path $PSScriptRoot 'native-runtime.ps1') -Action Inspect -Binary $binary | ConvertFrom-Json
$version=$inspection.version
if($ProbeNode){
 $proof=& $ProbeNode (Join-Path $PSScriptRoot 'code-mode-probe.mjs') (Join-Path (Split-Path -Parent $binary) 'codex-code-mode-host.exe') | ConvertFrom-Json
 if($LASTEXITCODE -ne 0 -or $proof.javascriptCellsExecuted -ne 2 -or !$proof.persistentStoreVerified -or $proof.delegateCalls -ne 0){throw 'SHARED_NATIVE_TOOL_HOST_EXEC_FAILED'}
}
if($Directory){
 if(!$ProbeNode){throw 'SHARED_NATIVE_PROBE_REQUIRED'}
 $pinned=& (Join-Path $PSScriptRoot 'native-runtime.ps1') -Action Pin -Binary $binary -Version $version -Directory $Directory | ConvertFrom-Json
 $binary=$pinned.binary;$version=$pinned.version
}
@{binary=$binary;version=$version;desktop=$desktop}|ConvertTo-Json -Compress
