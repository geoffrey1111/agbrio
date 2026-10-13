[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$BaselineInstaller,[Parameter(Mandatory=$true)][string]$CandidateInstaller,[Parameter(Mandatory=$true)][string]$Version)
$ErrorActionPreference='Stop'
# An actual installer changes shell folders/registry. This harness is explicitly
# confined to the disposable project Windows runner, never the owner's machine.
if($env:GITHUB_ACTIONS -cne 'true' -or $env:RUNNER_OS -cne 'Windows' -or $env:GITHUB_REPOSITORY -cne 'geoffrey1111/agbrio'){throw 'DISPOSABLE_GITHUB_WINDOWS_RUNNER_REQUIRED'}
if($Version -notmatch '^0\.1\.[0-9]+-rc\.[0-9]+$'){throw 'CANDIDATE_VERSION_REQUIRED'}
$workspace=[IO.Path]::GetFullPath((Get-Location).Path)
$baseline=(Resolve-Path -LiteralPath $BaselineInstaller).Path
$candidate=(Resolve-Path -LiteralPath $CandidateInstaller).Path
foreach($file in @($baseline,$candidate)){if(!$file.StartsWith($workspace+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)){throw 'INSTALLER_OUTSIDE_QA_WORKSPACE'}}
if(Get-Process -Name ai-work-router,ChatGPT -ErrorAction SilentlyContinue){throw 'RUNNER_MUST_HAVE_NO_EXISTING_HOST_OR_DESKTOP'}
$registration='HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Agbrio'
if(Test-Path -LiteralPath $registration){throw 'RUNNER_ALREADY_HAS_AGBRIO'}
$root=Join-Path $workspace 'runtime/shared-install'
$install=Join-Path $root 'Installed App With Spaces'
if(Test-Path -LiteralPath $root){throw 'PRESERVE_EXISTING_QA_ARTIFACTS'}
New-Item -ItemType Directory -Path $root|Out-Null
$data=Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'AIWorkRouter'
if(Test-Path -LiteralPath $data){throw 'RUNNER_USER_DATA_NOT_EMPTY'}
$programs=[Environment]::GetFolderPath('Programs')
$shortcut=Join-Path $programs 'Agbrio/Codex (Agbrio shared).lnk'
if(Test-Path -LiteralPath $shortcut){throw 'RUNNER_ALREADY_HAS_SHARED_ENTRY'}
$normalMarker=Join-Path $programs 'Codex-Agbrio-QA-preserve.lnk'
if(Test-Path -LiteralPath $normalMarker){throw 'RUNNER_MARKER_ALREADY_EXISTS'}
function Install-QA([string]$File,[switch]$Update){
 $arguments=@('/S');if($Update){$arguments+='/UPDATE'};$arguments+=('/D='+$install)
 $run=Start-Process -FilePath $File -ArgumentList $arguments -WindowStyle Hidden -Wait -PassThru
 if($run.ExitCode -ne 0){throw ('INSTALLER_FAILED_'+$run.ExitCode)}
 if(Get-Process -Name ai-work-router,ChatGPT -ErrorAction SilentlyContinue){throw 'INSTALLER_UNEXPECTEDLY_STARTED_AN_APPLICATION'}
}
Install-QA $baseline
if((Get-ItemProperty -LiteralPath $registration).DisplayVersion -cne '0.1.34'){throw 'BASELINE_VERSION_MISMATCH'}
if(Test-Path -LiteralPath $shortcut){throw 'BASELINE_UNEXPECTED_SHARED_ENTRY'}
New-Item -ItemType Directory -Path (Join-Path $data 'codex-shared/native/qa-preservation')|Out-Null
$marker=Join-Path $data 'qa-preservation.txt'
$cache=Join-Path $data 'codex-shared/native/qa-preservation/preserved.txt'
[IO.File]::WriteAllText($marker,'fictional user data; no credentials')
[IO.File]::WriteAllText($cache,'fictional native cache; never executed')
[IO.File]::WriteAllText($normalMarker,'unrelated shortcut marker')
$hashes=@{data=(Get-FileHash -LiteralPath $marker).Hash;cache=(Get-FileHash -LiteralPath $cache).Hash;normal=(Get-FileHash -LiteralPath $normalMarker).Hash}
Install-QA $candidate -Update
if((Get-ItemProperty -LiteralPath $registration).DisplayVersion -cne $Version){throw 'UPGRADED_VERSION_MISMATCH'}
if(!(Test-Path -LiteralPath $shortcut)){throw 'INSTALLED_SHARED_ENTRY_MISSING'}
$shell=New-Object -ComObject WScript.Shell
try{
 $link=$shell.CreateShortcut($shortcut)
 $expectedArguments='-NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File "'+(Join-Path $install 'codex-shared/launch.ps1')+'"'
 if($link.Arguments -cne $expectedArguments -or !$link.TargetPath.EndsWith('\WindowsPowerShell\v1.0\powershell.exe',[StringComparison]::OrdinalIgnoreCase)){throw 'INSTALLED_SHORTCUT_ARGUMENTS_MISMATCH'}
}finally{if($link){[void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($link)};[void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($shell)}
$shared=Join-Path $install 'codex-shared'
$sharedNames=@(Get-ChildItem -LiteralPath (Join-Path $workspace 'src-tauri/resources/codex-shared') -File)
$normalizedFiles=0
$utf8=[Text.UTF8Encoding]::new($false,$true)
foreach($source in $sharedNames){
 $installedFile=Join-Path $shared $source.Name
 if((Get-FileHash -LiteralPath $installedFile).Hash -cne (Get-FileHash -LiteralPath $source.FullName).Hash){
  # Git enforces LF; the signed local package may retain CRLF. The installer
  # itself is checked against its exact SHA before this harness starts. Permit
  # only this text transport difference, preserving BOM and every other byte.
  $installedText=$utf8.GetString([IO.File]::ReadAllBytes($installedFile)).Replace("`r`n","`n")
  $sourceText=$utf8.GetString([IO.File]::ReadAllBytes($source.FullName)).Replace("`r`n","`n")
  if($installedText -cne $sourceText){throw 'INSTALLED_RESOURCE_MISMATCH'}
  $normalizedFiles++
 }
}
$node=Join-Path $install 'browser-executor-node.exe'
if((& $node --version) -cne 'v24.16.0' -or $LASTEXITCODE -ne 0){throw 'INSTALLED_NODE_VERSION_MISMATCH'}
if([BitConverter]::ToString([IO.File]::ReadAllBytes((Join-Path $shared 'launch.ps1'))[0..2]) -cne 'EF-BB-BF'){throw 'INSTALLED_LAUNCHER_ENCODING_MISMATCH'}
foreach($pair in @(@($marker,'data'),@($cache,'cache'),@($normalMarker,'normal'))){if((Get-FileHash -LiteralPath $pair[0]).Hash -cne $hashes[$pair[1]]){throw 'UPGRADE_CHANGED_PRESERVED_DATA'}}
$uninstaller=Join-Path $install 'uninstall.exe'
$remove=Start-Process -FilePath $uninstaller -ArgumentList @('/S',('_?='+$install)) -WindowStyle Hidden -Wait -PassThru
if($remove.ExitCode -ne 0){throw 'UNINSTALL_FAILED'}
if((Test-Path -LiteralPath $registration) -or (Test-Path -LiteralPath $shortcut) -or (Test-Path -LiteralPath (Join-Path $install 'ai-work-router.exe'))){throw 'UNINSTALL_LEFT_REGISTRATION_OR_ENTRY'}
foreach($pair in @(@($marker,'data'),@($cache,'cache'),@($normalMarker,'normal'))){if((Get-FileHash -LiteralPath $pair[0]).Hash -cne $hashes[$pair[1]]){throw 'UNINSTALL_CHANGED_PRESERVED_DATA'}}
$proof=@{actualBaseline='0.1.34';actualUpgrade=$Version;installedSharedEntry=$true;installedSharedFiles=$sharedNames.Count;gitNewlineNormalizedResourceFiles=$normalizedFiles;quotedPathWithSpaces=$true;nativeNodeVersion='24.16.0';launcherBomPreserved=$true;fictionalDataAndCachePreserved=$true;unrelatedShortcutPreserved=$true;actualUninstall=$true;productRegistrationRemoved=$true;sharedEntryRemoved=$true;providerCalls=0;desktopStarts=0;realUserDataUsed=$false;desktopSharedConnectionAcceptance=$false}
$proof|ConvertTo-Json -Depth 3|Set-Content -LiteralPath (Join-Path $root 'proof.json') -Encoding utf8
$proof|ConvertTo-Json -Compress
