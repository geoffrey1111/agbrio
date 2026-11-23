#Requires -Version 7.2
param([Parameter(Mandatory)][string]$FixtureExecutable)
$ErrorActionPreference='Stop'
$scriptPath=Join-Path $PSScriptRoot '../tools/agbrio-control/update-local.ps1'
$errors=$null
$null=[System.Management.Automation.Language.Parser]::ParseFile($scriptPath,[ref]$null,[ref]$errors)
if($errors.Count){throw 'Update script parse failed'}
$root=Join-Path 'D:/desktop/临时处理' ('agbrio-update-fixture-'+[guid]::NewGuid().ToString('N'))
$install=Join-Path $root 'installation with spaces'
New-Item -ItemType Directory -Path (Join-Path $install 'app-control') -Force | Out-Null
$exe=Join-Path $install 'ai-work-router.exe'
Copy-Item -LiteralPath $FixtureExecutable -Destination $exe
$package=Join-Path $root 'fixture-installer.exe'
[IO.File]::WriteAllText($package,'fixture installer; must never run')
$control=Join-Path $install 'app-control/control.ps1'
@'
param($Command,$Executable,$ExpectedInstance)
if($Command -eq 'Status'){@{appVersion=(Get-Item -LiteralPath $Executable).VersionInfo.ProductVersion;instanceId='00000000-0000-4000-8000-000000000001'}|ConvertTo-Json -Compress;return}
if($Command -eq 'Stop'){@{state='UNCONFIRMED'}|ConvertTo-Json -Compress;return}
throw 'Unexpected fixture CLI operation'
'@|Set-Content -LiteralPath $control
$arguments=@{Installer=$package;InstallerSha256=(Get-FileHash -LiteralPath $package).Hash;Version='99.0.0';NativeSha256=('a'*64);Executable=$exe}
$savedLocalAppData=$env:LOCALAPPDATA
$env:LOCALAPPDATA=Join-Path $root 'isolated-profile'
# These process functions replace the real effects even if a test unexpectedly
# enters the installation branch. The fixture executable is never launched.
function Get-CimInstance {param($ClassName,$Filter) @([pscustomobject]@{ExecutablePath=$exe})}
function Start-Process {throw 'Fixture forbids all process launches'}
try{
 $value=(& $scriptPath @arguments -ValidateOnly)|ConvertFrom-Json
 if($value.state -ne 'VALIDATED_ONLY' -or $value.stopped -or $value.installed){throw 'Validation-only mode performed effects'}
 $bad=$arguments.Clone();$bad.InstallerSha256='0'*64
 $rejected=$false;try{& $scriptPath @bad -ValidateOnly|Out-Null}catch{if($_.Exception.Message -notlike 'Checksum mismatch:*'){throw};$rejected=$true}
 if(!$rejected){throw 'Tampered package was accepted'}
 $rejected=$false;try{& $scriptPath @arguments|Out-Null}catch{if($_.Exception.Message -ne 'Old Host exit is unconfirmed'){throw};$rejected=$true}
 if(!$rejected){throw 'Uncertain shutdown allowed installation'}
 'PASS: parse, validation-only, tamper rejection and uncertain shutdown fencing; fixture only, no installer or live Host operation.'
}finally{
 $env:LOCALAPPDATA=$savedLocalAppData
 $resolved=(Resolve-Path -LiteralPath $root).Path
 if((Split-Path $resolved -Parent) -ine (Resolve-Path 'D:/desktop/临时处理').Path){throw 'Unexpected fixture cleanup path'}
 $trash='D:/desktop/临时处理/_trash/'+(Get-Date -Format yyyyMMdd)+'/01a1dfe7'
 New-Item -ItemType Directory -Path $trash -Force|Out-Null
 Move-Item -LiteralPath $resolved -Destination (Join-Path $trash (Split-Path $resolved -Leaf))
}
