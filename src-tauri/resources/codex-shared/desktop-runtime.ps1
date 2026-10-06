$ErrorActionPreference='Stop'
$prefix=Join-Path $env:LOCALAPPDATA 'OpenAI/Codex/bin'
$matches=@(Get-CimInstance Win32_Process -Filter "Name='codex.exe'" | Where-Object {$_.CommandLine -match '\bapp-server\b' -and $_.ExecutablePath -and $_.ExecutablePath.StartsWith($prefix,[StringComparison]::OrdinalIgnoreCase)} | Where-Object {$parent=Get-CimInstance Win32_Process -Filter ('ProcessId='+$_.ParentProcessId);$parent.Name -eq 'ChatGPT.exe' -and $parent.ExecutablePath -match '^C:\\Program Files\\WindowsApps\\OpenAI\.Codex_'})
if($matches.Count -ne 1){throw 'SHARED_DESKTOP_BACKEND_UNRESOLVED'}
$binary=$matches[0].ExecutablePath
$versionOutput=& $binary --version
if($LASTEXITCODE -ne 0 -or $versionOutput -notmatch '^codex-cli ([0-9]+\.[0-9]+\.[0-9]+(?:[^ ]*)?)$'){throw 'SHARED_NATIVE_BINARY_UNVERIFIED'}
$version=$Matches[1]
$app=Get-AppxPackage -Name 'OpenAI.Codex'
$desktop=Join-Path $app.InstallLocation 'app/ChatGPT.exe'
if(!(Test-Path -LiteralPath $desktop)){throw 'SHARED_DESKTOP_BINARY_UNRESOLVED'}
@{binary=$binary;version=$version;desktop=$desktop}|ConvertTo-Json -Compress
