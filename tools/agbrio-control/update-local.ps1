#Requires -Version 7.2
[CmdletBinding()]
param(
 [Parameter(Mandatory)][string]$Installer,
 [Parameter(Mandatory)][ValidatePattern('^[a-fA-F0-9]{64}$')][string]$InstallerSha256,
 [Parameter(Mandatory)][string]$Version,
 [Parameter(Mandatory)][ValidatePattern('^[a-fA-F0-9]{64}$')][string]$NativeSha256,
 [string]$Executable=(Join-Path $PSScriptRoot '../ai-work-router.exe'),
 [switch]$ValidateOnly
)
$ErrorActionPreference='Stop'
function Assert-Hash([string]$Path,[string]$Expected){
 if((Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash -ine $Expected){throw "Checksum mismatch: $Path"}
}
function Read-Control([string]$Operation,[string]$Expected=''){
 $global:LASTEXITCODE=0
 $lines=& $control -Command $Operation -Executable $exe -ExpectedInstance $Expected
 if($LASTEXITCODE -ne 0){throw "Agbrio CLI failed: $Operation. $lines"}
 $value=($lines -join "`n")|ConvertFrom-Json
 if($value.errorCode){throw [string]$value.errorCode}
 return $value
}
function Matching-Processes{
 @(Get-CimInstance Win32_Process -Filter "Name='ai-work-router.exe'" | Where-Object {$_.ExecutablePath -ieq $exe})
}
$package=(Resolve-Path -LiteralPath $Installer).Path
$exe=(Resolve-Path -LiteralPath $Executable).Path
$installRoot=Split-Path $exe -Parent
$control=Join-Path $installRoot 'app-control/control.ps1'
if([IO.Path]::GetFileName($exe) -ine 'ai-work-router.exe' -or !(Test-Path -LiteralPath $control -PathType Leaf)){throw 'Existing Agbrio installation and CLI required'}
foreach($path in @($package,$exe,$installRoot,$control)){
 if((Get-Item -LiteralPath $path).Attributes -band [IO.FileAttributes]::ReparsePoint){throw "Unexpected linked path: $path"}
}
$desired=[semver]$Version
$installed=[semver](Get-Item -LiteralPath $exe).VersionInfo.ProductVersion
if($installed -gt $desired){throw 'A newer Agbrio is installed; refusing downgrade'}
if($package.StartsWith($installRoot+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)){throw 'Keep the update package outside the installation directory'}
Assert-Hash $package $InstallerSha256
if($ValidateOnly){[pscustomobject]@{state='VALIDATED_ONLY';installedVersion="$installed";targetVersion=$Version;installer=$package;stopped=$false;installed=$false}|ConvertTo-Json;return}

$updateRoot=Join-Path $env:LOCALAPPDATA 'AIWorkRouter/updates'
New-Item -ItemType Directory -Path $updateRoot -Force | Out-Null
# An exclusive local file lock prevents two copies from stopping/installing together.
$lock=[IO.File]::Open((Join-Path $updateRoot 'local-update.lock'),[IO.FileMode]::OpenOrCreate,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
$backup=$null
try{
 if($installed -eq $desired){
  Assert-Hash $exe $NativeSha256
  $running=Read-Control 'Start'
  if($running.appVersion -cne $Version){throw 'Running Host version differs'}
  Write-Host "Agbrio $Version is already installed and running."
  return
 }
 Write-Host "Updating Agbrio $installed -> $Version. Please keep this window open."
 if((Matching-Processes).Count){
  $old=Read-Control 'Status'
  if($old.appVersion -cne "$installed"){throw 'Installed/running version differs; no shutdown requested'}
  $stopped=Read-Control 'Stop' $old.instanceId
  if($stopped.state -ne 'STOPPED'){throw 'Old Host exit is unconfirmed'}
 }
 $deadline=[DateTime]::UtcNow.AddSeconds(30)
 while((Matching-Processes).Count){
  if([DateTime]::UtcNow -ge $deadline){throw 'An Agbrio process still holds the installation. No force kill or installation attempted.'}
  Start-Sleep -Milliseconds 250
 }
 $backup=Join-Path $updateRoot ('backups/'+$installed+'-'+[DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss')+'-'+[guid]::NewGuid().ToString('N'))
 New-Item -ItemType Directory -Path $backup | Out-Null
 Copy-Item -LiteralPath $installRoot -Destination (Join-Path $backup 'installation') -Recurse
 $profile=Join-Path $env:LOCALAPPDATA 'AIWorkRouter/data'
 if(Test-Path -LiteralPath $profile){Copy-Item -LiteralPath $profile -Destination (Join-Path $backup 'data') -Recurse}
 if((Matching-Processes).Count){throw 'Agbrio restarted during backup; installation cancelled'}
 Assert-Hash $package $InstallerSha256
 # NSIS /D must be last and unquoted: all remaining characters are the path.
 $setup=Start-Process -FilePath $package -ArgumentList "/S /UPDATE /D=$installRoot" -WindowStyle Hidden -Wait -PassThru
 if($setup.ExitCode -ne 0){throw "Installer exited with code $($setup.ExitCode)"}
 Assert-Hash $exe $NativeSha256
 if((Get-Item -LiteralPath $exe).VersionInfo.ProductVersion -cne $Version){throw 'Installed executable version differs'}
 $new=Read-Control 'Start'
 if($new.state -ne 'RUNNING' -or $new.appVersion -cne $Version){throw 'New Host readiness is unconfirmed'}
 $receipt=[ordered]@{state='INSTALLED_AND_RUNNING';version=$Version;installedAt=[DateTime]::UtcNow.ToString('o');installerSha256=$InstallerSha256;nativeSha256=$NativeSha256;instanceId=$new.instanceId;pid=$new.pid;backup=$backup;taskContinuationVerified=$false}
 $receipt|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $backup 'receipt.json') -Encoding utf8
 $receipt|ConvertTo-Json
}catch{
 Write-Warning "Update stopped: $($_.Exception.Message)"
 if($backup){Write-Warning "Backup retained at $backup. No automatic rollback or task replay was attempted."}
 throw
}finally{$lock.Dispose()}
