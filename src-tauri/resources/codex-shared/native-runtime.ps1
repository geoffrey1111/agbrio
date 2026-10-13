# Verify and retain a complete, signed cohort; never mix companion versions.
param([ValidateSet('Inspect','Verify','Pin')][string]$Action='Verify',[string]$Binary,[string]$Version,[string]$Directory)
$ErrorActionPreference='Stop'
# Node may preserve a PowerShell7 PSModulePath when invoking Windows PowerShell.
# Resolve its own built-in verifier explicitly instead of autoloading a different
# edition's Security module. Keep signature verification mandatory.
$env:PSModulePath=(Join-Path $PSHOME 'Modules')+[IO.Path]::PathSeparator+$env:PSModulePath
try{Import-Module -Name (Join-Path $PSHOME 'Modules/Microsoft.PowerShell.Security/Microsoft.PowerShell.Security.psd1') -ErrorAction Stop}catch{throw 'SHARED_NATIVE_SIGNATURE_VERIFIER_UNAVAILABLE'}
$NativeNames=@('codex.exe','codex-code-mode-host.exe','codex-command-runner.exe','codex-windows-sandbox-setup.exe')
function Remove-NativeStaging([string]$Staged,[string]$NativeRoot){
 $root=[IO.Path]::GetFullPath($NativeRoot);$path=[IO.Path]::GetFullPath($Staged)
 if(![string]::Equals((Split-Path -Parent $path),$root,[StringComparison]::OrdinalIgnoreCase) -or (Split-Path -Leaf $path) -notmatch '^staging-[a-f0-9]{32}$'){throw 'SHARED_NATIVE_CLEANUP_REJECTED'}
 if((Get-Item -LiteralPath $path).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'SHARED_NATIVE_CLEANUP_REJECTED'}
 $entries=@(Get-ChildItem -LiteralPath $path -Force)
 foreach($entry in $entries){if($entry.PSIsContainer -or ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $entry.Name -cnotin ($NativeNames+@('manifest.json'))){throw 'SHARED_NATIVE_CLEANUP_REJECTED'}}
 # Exact new-staging leaves only. No recursive or forced deletion, old cache or
 # user files; reject all unexpected entries before deleting even the first leaf.
 foreach($entry in $entries){Remove-Item -LiteralPath $entry.FullName -ErrorAction Stop}
 Remove-Item -LiteralPath $path -ErrorAction Stop
}
function Read-NativeCohort([string]$Path) {
 $root=[IO.Path]::GetFullPath($Path);$files=@();$signer=$null
 if((Get-Item -LiteralPath $root).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'SHARED_NATIVE_RUNTIME_LINK_REJECTED'}
 foreach($name in $NativeNames){
  $file=Join-Path $root $name
  if(!(Test-Path -LiteralPath $file -PathType Leaf)){throw 'SHARED_NATIVE_RUNTIME_INCOMPLETE'}
  $info=Get-Item -LiteralPath $file
  if($info.Length -le 0 -or ($info.Attributes -band [IO.FileAttributes]::ReparsePoint)){throw 'SHARED_NATIVE_RUNTIME_INCOMPLETE'}
  $sig=Get-AuthenticodeSignature -LiteralPath $file
  if($sig.Status -ne 'Valid' -or $sig.SignerCertificate.Subject -notmatch '(?:^|,\s*)O="OpenAI(?: OpCo)?, LLC"(?:,|$)'){throw 'SHARED_NATIVE_SIGNATURE_INVALID'}
  if(!$signer){$signer=$sig.SignerCertificate.Thumbprint}
  if($sig.SignerCertificate.Thumbprint -ne $signer){throw 'SHARED_NATIVE_COHORT_MISMATCH'}
  $files+=@{name=$name;sha256=(Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant();bytes=$info.Length}
 }
 $output=& (Join-Path $root 'codex.exe') --version
 if($LASTEXITCODE -ne 0 -or $output -notmatch '^codex-cli ([0-9]+\.[0-9]+\.[0-9]+[^ ]*)$'){throw 'SHARED_NATIVE_BINARY_UNVERIFIED'}
 $actualVersion=$Matches[1]
 if($Version -and $actualVersion -cne $Version){throw 'SHARED_NATIVE_VERSION_MISMATCH'}
 $null=& (Join-Path $root 'codex-code-mode-host.exe') --help
 if($LASTEXITCODE -ne 0){throw 'SHARED_NATIVE_TOOL_HOST_FAILED'}
 @{format=1;version=$actualVersion;signer=$signer;files=$files}
}
function Assert-NativeManifest($Manifest,$Actual) {
 if($Manifest.format -ne 1 -or $Manifest.version -cne $Actual.version -or $Manifest.signer -cne $Actual.signer -or @($Manifest.files).Count -ne $NativeNames.Count){throw 'SHARED_NATIVE_COHORT_MISMATCH'}
 foreach($file in $Actual.files){
  $expected=@($Manifest.files | Where-Object name -eq $file.name)
  if($expected.Count -ne 1 -or $expected[0].sha256 -cne $file.sha256 -or $expected[0].bytes -ne $file.bytes){throw 'SHARED_NATIVE_COHORT_MISMATCH'}
 }
}
if($MyInvocation.InvocationName -ne '.'){
 if(!$Binary -or !(Test-Path -LiteralPath $Binary -PathType Leaf)){throw 'SHARED_NATIVE_BINARY_UNVERIFIED'}
 $source=Split-Path -Parent ([IO.Path]::GetFullPath($Binary));$manifest=Read-NativeCohort $source
 if($Action -eq 'Pin'){
  if(!$Directory){throw 'SHARED_NATIVE_RUNTIME_DIRECTORY_REQUIRED'}
  $nativeRoot=Join-Path ([IO.Path]::GetFullPath($Directory)) 'native';New-Item -ItemType Directory -Path $nativeRoot -Force | Out-Null
  if((Get-Item -LiteralPath $nativeRoot).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'SHARED_NATIVE_RUNTIME_LINK_REJECTED'}
  $hashText=($manifest.files | ForEach-Object {$_.name+':'+$_.sha256}) -join ';'
  $sha=[Security.Cryptography.SHA256]::Create();try{$cohort=([BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($hashText)))).Replace('-','').ToLowerInvariant()}finally{$sha.Dispose()}
  $target=Join-Path $nativeRoot $cohort
  if(!(Test-Path -LiteralPath $target)){
   $staged=Join-Path $nativeRoot ('staging-'+[guid]::NewGuid().ToString('N'));New-Item -ItemType Directory -Path $staged | Out-Null
   try{
    foreach($name in $NativeNames){Copy-Item -LiteralPath (Join-Path $source $name) -Destination (Join-Path $staged $name)}
    Assert-NativeManifest $manifest (Read-NativeCohort $staged)
    [IO.File]::WriteAllText((Join-Path $staged 'manifest.json'),($manifest | ConvertTo-Json -Depth 5),[Text.UTF8Encoding]::new($false))
    Move-Item -LiteralPath $staged -Destination $target
   }finally{
    if(Test-Path -LiteralPath $staged){
     Remove-NativeStaging $staged $nativeRoot
    }
   }
  }
  $saved=Get-Content -LiteralPath (Join-Path $target 'manifest.json') -Raw | ConvertFrom-Json
  Assert-NativeManifest $saved (Read-NativeCohort $target);$source=$target
 }elseif($Action -eq 'Verify'){
  $path=Join-Path $source 'manifest.json'
  if(!(Test-Path -LiteralPath $path)){throw 'SHARED_NATIVE_MANIFEST_MISSING'}
  Assert-NativeManifest (Get-Content -LiteralPath $path -Raw | ConvertFrom-Json) $manifest
 }
 @{binary=(Join-Path $source 'codex.exe');version=$manifest.version;cohortVerified=$true;files=$manifest.files}|ConvertTo-Json -Depth 5 -Compress
}
