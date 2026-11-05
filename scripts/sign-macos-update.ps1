[CmdletBinding()]
param(
 [Parameter(Mandatory)][ValidatePattern('^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?$')][string]$Version,
 [Parameter(Mandatory)][ValidatePattern('^[a-f0-9]{40}$')][string]$SourceSha,
 [Parameter(Mandatory)][ValidatePattern('^[A-Za-z0-9_./-]+$')][string]$SourceRef,
 [Parameter(Mandatory)][long]$RunId,
 [Parameter(Mandatory)][long]$ArtifactId,
 [Parameter(Mandatory)][string]$ArtifactZip,
 [Parameter(Mandatory)][string]$OutputDirectory,
 [Parameter(Mandatory)][string]$SigningCredentialsDirectory,
 [string]$WindowsManifest=''
)
$ErrorActionPreference='Stop'
if($env:GITHUB_ACTIONS -eq 'true'){throw 'Signing is publisher-local only; no CI key access'}
if($PSVersionTable.PSVersion.Major -lt 7 -or !$IsWindows){throw 'Use publisher-local PowerShell 7 on Windows for the existing DPAPI credentials'}
$repoRoot=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
function Read-GhJson([string]$Route){
 $result=& gh api $Route 2>$null
 if($LASTEXITCODE -ne 0){throw 'GitHub provenance unavailable; no signing'}
 return (($result -join "`n")|ConvertFrom-Json)
}
function Run-PrivateChild([string]$Executable,[string[]]$Arguments,[hashtable]$Environment,[string]$ExpectedMarker=''){
 $info=[Diagnostics.ProcessStartInfo]::new($Executable)
 $info.UseShellExecute=$false;$info.CreateNoWindow=$true;$info.RedirectStandardOutput=$true;$info.RedirectStandardError=$true;$info.WorkingDirectory=$repoRoot
 foreach($name in @('TAURI_SIGNING_PRIVATE_KEY','TAURI_SIGNING_PRIVATE_KEY_PATH','TAURI_SIGNING_PRIVATE_KEY_PASSWORD')){$info.Environment.Remove($name)|Out-Null}
 foreach($name in $Environment.Keys){$info.Environment[$name]=[string]$Environment[$name]}
 foreach($argument in $Arguments){$info.ArgumentList.Add($argument)}
 $child=[Diagnostics.Process]::new();$child.StartInfo=$info
 try{
  if(!$child.Start()){throw 'Local signing/verification process did not start'}
  $stdout=$child.StandardOutput.ReadToEndAsync();$stderr=$child.StandardError.ReadToEndAsync()
  foreach($name in @('TAURI_SIGNING_PRIVATE_KEY','TAURI_SIGNING_PRIVATE_KEY_PATH','TAURI_SIGNING_PRIVATE_KEY_PASSWORD')){$info.Environment.Remove($name)|Out-Null}
  $child.WaitForExit();$text=$stdout.GetAwaiter().GetResult();$null=$stderr.GetAwaiter().GetResult()
  if($child.ExitCode -ne 0 -or ($ExpectedMarker -and !$text.Contains($ExpectedMarker))){throw 'Local signing/verification failed; no publication'}
 }finally{$child.Dispose()}
}
$head=(& git -C $repoRoot rev-parse HEAD).Trim()
if($LASTEXITCODE -ne 0 -or $head -cne $SourceSha){throw 'Local reviewed source SHA mismatch'}
& git -C $repoRoot diff --quiet HEAD -- src-tauri/tauri.conf.json src-tauri/src/desktop_update.rs scripts/sign-macos-update.ps1 scripts/macos-update-package.py scripts/create-macos-update-manifest.mjs package-lock.json src-tauri/Cargo.lock
if($LASTEXITCODE -ne 0){throw 'Signing/verification inputs must match the reviewed source commit'}
$run=Read-GhJson "repos/geoffrey1111/agbrio/actions/runs/$RunId"
$artifact=Read-GhJson "repos/geoffrey1111/agbrio/actions/artifacts/$ArtifactId"
if($run.status -ne 'completed' -or $run.conclusion -ne 'success' -or $run.head_sha -cne $SourceSha -or $run.head_branch -cne $SourceRef -or $run.path -ne '.github/workflows/macos-package.yml' -or $run.event -ne 'workflow_dispatch'){throw 'Exact successful Mac package run required'}
if($artifact.expired -or $artifact.workflow_run.id -ne $RunId -or $artifact.name -ne 'agbrio-macos-aarch64-update-unsigned' -or $artifact.digest -notmatch '^sha256:([a-f0-9]{64})$'){throw 'Artifact provenance mismatch'}
$zipDigest=$Matches[1]
$zipPath=(Resolve-Path -LiteralPath $ArtifactZip).Path
if((Get-Item -LiteralPath $zipPath).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Linked artifact input'}
if((Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant() -cne $zipDigest){throw 'GitHub artifact digest mismatch'}
$out=[IO.Path]::GetFullPath($OutputDirectory)
if(Test-Path -LiteralPath $out){throw 'Preserve existing candidate directory'}
$archiveName="Agbrio_${Version}_aarch64.app.tar.gz";$archive=Join-Path $out $archiveName;$manifest=Join-Path $out 'MACOS_UPDATE_MANIFEST.json'
& python (Join-Path $PSScriptRoot 'macos-update-package.py') unpack --zip $zipPath --archive $archive --manifest $manifest --version $Version --source $SourceSha --sha256 $zipDigest
if($LASTEXITCODE -ne 0){throw 'Artifact validation failed before credential access'}
$metadata=Get-Content -LiteralPath $manifest -Raw|ConvertFrom-Json
if(!(Test-Path -LiteralPath (Join-Path $repoRoot 'dist/index.html'))){throw 'Build the reviewed local frontend/resources before signing; credentials were not accessed'}
Run-PrivateChild (Get-Command cargo -ErrorAction Stop).Source @('test','--manifest-path','src-tauri/Cargo.toml','--lib','--locked','--no-run') @{}
$credentials=(Resolve-Path -LiteralPath $SigningCredentialsDirectory).Path
if($credentials.StartsWith('\\') -or ([IO.DriveInfo]::new([IO.Path]::GetPathRoot($credentials))).DriveType -eq [IO.DriveType]::Network){throw 'Signing credentials must remain on this local publisher machine'}
if($out.StartsWith($credentials+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase) -or $credentials.StartsWith($out+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase) -or $out -eq $credentials){throw 'Credentials and candidate staging must be separate'}
$key=Join-Path $credentials 'agbrio-updater.key';$password=Join-Path $credentials 'signing-password.dpapi'
foreach($file in @($key,$password)){if(!(Test-Path -LiteralPath $file) -or ((Get-Item -LiteralPath $file).Attributes -band [IO.FileAttributes]::ReparsePoint)){throw 'Existing Windows publisher credentials required'}}
$cli=Join-Path $repoRoot 'node_modules/@tauri-apps/cli/tauri.js'
if(!(Test-Path -LiteralPath $cli)){throw 'Install locked local Tauri CLI first'}
$securePassword=(Get-Content -LiteralPath $password -Raw).Trim()|ConvertTo-SecureString
$pointer=[Runtime.InteropServices.Marshal]::SecureStringToBSTR($securePassword)
try{
 $plain=[Runtime.InteropServices.Marshal]::PtrToStringBSTR($pointer)
 Run-PrivateChild (Get-Command node -ErrorAction Stop).Source @($cli,'signer','sign','--private-key-path',$key,$archive) @{TAURI_SIGNING_PRIVATE_KEY_PASSWORD=$plain}
}finally{[Runtime.InteropServices.Marshal]::ZeroFreeBSTR($pointer);$plain=$null;$securePassword.Dispose()}
if(!(Test-Path -LiteralPath ($archive+'.sig'))){throw 'Signer sidecar missing'}
Run-PrivateChild (Get-Command cargo -ErrorAction Stop).Source @('test','--manifest-path','src-tauri/Cargo.toml','--lib','desktop_update::tests::actual_macos_updater_signature','--locked','--','--ignored','--exact','--test-threads=1','--nocapture') @{AGBRIO_MAC_SIGNED_ARCHIVE=$archive;AGBRIO_MAC_EXPECTED_ARCHIVE_SHA256=$metadata.archive.sha256} ('AGBRIO_MAC_SIGNATURE_GATE_PASS:'+$metadata.archive.sha256)
$candidate=Join-Path $out 'macos-platform.candidate.json'
$argsList=@((Join-Path $PSScriptRoot 'create-macos-update-manifest.mjs'),'--version',$Version,'--archive',$archiveName,'--signature',($archive+'.sig'),'--output',$candidate)
if($WindowsManifest){$argsList+=@('--base',(Resolve-Path -LiteralPath $WindowsManifest).Path)}
& node @argsList
if($LASTEXITCODE -ne 0){throw 'Candidate manifest failed; signed bytes preserved'}
@{source=$SourceSha;runId=$RunId;artifactId=$ArtifactId;artifactZipSha256=$zipDigest;archiveSha256=$metadata.archive.sha256;version=$Version;signatureVerified=$true;tamperRejected=$true;published=$false}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $out 'LOCAL_SIGNING_RECEIPT.json') -Encoding utf8
Write-Output 'LOCAL_SIGNED_MAC_CANDIDATE_READY; not published or installed.'
