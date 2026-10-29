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
if($Directory -and !$ProbeNode){throw 'SHARED_NATIVE_PROBE_REQUIRED'}
# Pin authenticates readable source bytes, then executes the ordinary retained set.
# Inspecting the encrypted package first made the compatibility copy unreachable.
$mode=if($Directory){'Pin'}else{'Inspect'}
$inspection=& (Join-Path $PSScriptRoot 'native-runtime.ps1') -Action $mode -Binary $binary -Directory $Directory | ConvertFrom-Json
if(!$inspection.binary -or !$inspection.version){throw 'SHARED_NATIVE_BINARY_UNVERIFIED'}
$binary=$inspection.binary;$version=$inspection.version
if($ProbeNode){
 $proof=& $ProbeNode (Join-Path $PSScriptRoot 'code-mode-probe.mjs') (Join-Path (Split-Path -Parent $binary) 'codex-code-mode-host.exe') | ConvertFrom-Json
 if($LASTEXITCODE -ne 0 -or $proof.javascriptCellsExecuted -ne 2 -or !$proof.persistentStoreVerified -or $proof.delegateCalls -ne 0){throw 'SHARED_NATIVE_TOOL_HOST_EXEC_FAILED'}
}
@{binary=$binary;version=$version;desktop=$desktop}|ConvertTo-Json -Compress
