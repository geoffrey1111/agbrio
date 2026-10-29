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
function Copy-NativeBytes([string]$SourceFile,[string]$TargetFile){
 # WindowsApps files can carry package encryption metadata that Copy-Item tries
 # to preserve. Read the authorized executable bytes into a new ordinary file;
 # never modify source encryption/ACLs or overwrite an existing cache member.
 $inputStream=[IO.File]::Open($SourceFile,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
 try{
  $outputStream=[IO.File]::Open($TargetFile,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
  try{$inputStream.CopyTo($outputStream)}finally{$outputStream.Dispose()}
 }finally{$inputStream.Dispose()}
}
function Read-NativeFiles([string]$Path) {
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
 @{format=1;signer=$signer;files=$files}
}
function Native-ExecutionError($Failure,[string]$Prefix){$error=$Failure.Exception;for($i=0;$error -and $i -lt 8;$i++){if(($error -is [ComponentModel.Win32Exception] -and $error.NativeErrorCode -eq 5) -or ($error.HResult -band 65535) -eq 5){return ($Prefix+'_DENIED')};$error=$error.InnerException};return ($Prefix+'_FAILED')}
function Read-NativeCohort([string]$Path) {
 $root=[IO.Path]::GetFullPath($Path);$identity=Read-NativeFiles $root
 try{$output=& (Join-Path $root 'codex.exe') --version}catch{throw (Native-ExecutionError $_ 'SHARED_NATIVE_EXECUTION')}
 if($LASTEXITCODE -ne 0 -or $output -notmatch '^codex-cli ([0-9]+\.[0-9]+\.[0-9]+[^ ]*)$'){throw 'SHARED_NATIVE_BINARY_UNVERIFIED'}
 $actualVersion=$Matches[1]
 if($Version -and $actualVersion -cne $Version){throw 'SHARED_NATIVE_VERSION_MISMATCH'}
 try{$null=& (Join-Path $root 'codex-code-mode-host.exe') --help}catch{throw (Native-ExecutionError $_ 'SHARED_NATIVE_TOOL_HOST_EXECUTION')}
 if($LASTEXITCODE -ne 0){throw 'SHARED_NATIVE_TOOL_HOST_FAILED'}
 @{format=1;version=$actualVersion;signer=$identity.signer;files=$identity.files}
}
function Assert-NativeFiles($Expected,$Actual){
 if($Expected.signer -cne $Actual.signer -or @($Expected.files).Count -ne $NativeNames.Count -or @($Actual.files).Count -ne $NativeNames.Count){throw 'SHARED_NATIVE_COHORT_MISMATCH'}
 foreach($file in $Actual.files){$match=@($Expected.files|Where-Object name -CEQ $file.name);if($match.Count -ne 1 -or $match[0].sha256 -cne $file.sha256 -or $match[0].bytes -ne $file.bytes){throw 'SHARED_NATIVE_COHORT_MISMATCH'}}
}
function Assert-NativeManifest($Manifest,$Actual) {
 if($Manifest.format -ne 1 -or $Manifest.version -cne $Actual.version -or $Manifest.signer -cne $Actual.signer -or @($Manifest.files).Count -ne $NativeNames.Count){throw 'SHARED_NATIVE_COHORT_MISMATCH'}
 foreach($file in $Actual.files){
  $expected=@($Manifest.files | Where-Object name -eq $file.name)
  if($expected.Count -ne 1 -or $expected[0].sha256 -cne $file.sha256 -or $expected[0].bytes -ne $file.bytes){throw 'SHARED_NATIVE_COHORT_MISMATCH'}
 }
}
function Pin-NativeCohort([string]$Source,[string]$Destination){
 if(!$Destination){throw 'SHARED_NATIVE_RUNTIME_DIRECTORY_REQUIRED'}
 # Never execute an AppX-encrypted source as a prerequisite for materializing it.
 $expected=Read-NativeFiles $Source
 $nativeRoot=Join-Path ([IO.Path]::GetFullPath($Destination)) 'native';New-Item -ItemType Directory -Path $nativeRoot -Force|Out-Null
 if((Get-Item -LiteralPath $nativeRoot).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'SHARED_NATIVE_RUNTIME_LINK_REJECTED'}
 $hashText=($expected.files|ForEach-Object{$_.name+':'+$_.sha256})-join ';';$sha=[Security.Cryptography.SHA256]::Create();try{$cohort=([BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($hashText)))).Replace('-','').ToLowerInvariant()}finally{$sha.Dispose()}
 $target=Join-Path $nativeRoot $cohort
 if(!(Test-Path -LiteralPath $target)){
  $staged=Join-Path $nativeRoot ('staging-'+[guid]::NewGuid().ToString('N'));New-Item -ItemType Directory -Path $staged|Out-Null
  try{
   foreach($name in $NativeNames){Copy-NativeBytes (Join-Path $Source $name) (Join-Path $staged $name)}
   Assert-NativeFiles $expected (Read-NativeFiles $staged)
   $manifest=Read-NativeCohort $staged
   Assert-NativeFiles $expected $manifest
   [IO.File]::WriteAllText((Join-Path $staged 'manifest.json'),($manifest|ConvertTo-Json -Depth 5),[Text.UTF8Encoding]::new($false))
   Move-Item -LiteralPath $staged -Destination $target
  }finally{if(Test-Path -LiteralPath $staged){Remove-NativeStaging $staged $nativeRoot}}
 }
 # An existing retained set must match the registered source before execution.
 Assert-NativeFiles $expected (Read-NativeFiles $target)
 $saved=Get-Content -LiteralPath (Join-Path $target 'manifest.json') -Raw|ConvertFrom-Json
 $actual=Read-NativeCohort $target;Assert-NativeManifest $saved $actual
 @{binary=(Join-Path $target 'codex.exe');version=$actual.version;cohortVerified=$true;files=$actual.files}
}
if($MyInvocation.InvocationName -ne '.'){
 if(!$Binary -or !(Test-Path -LiteralPath $Binary -PathType Leaf)){throw 'SHARED_NATIVE_BINARY_UNVERIFIED'}
 $source=Split-Path -Parent ([IO.Path]::GetFullPath($Binary))
 if($Action -eq 'Pin'){$result=Pin-NativeCohort $source $Directory}
 else{$manifest=Read-NativeCohort $source;if($Action -eq 'Verify'){$path=Join-Path $source 'manifest.json';if(!(Test-Path -LiteralPath $path)){throw 'SHARED_NATIVE_MANIFEST_MISSING'};Assert-NativeManifest (Get-Content -LiteralPath $path -Raw|ConvertFrom-Json) $manifest};$result=@{binary=(Join-Path $source 'codex.exe');version=$manifest.version;cohortVerified=$true;files=$manifest.files}}
 $result|ConvertTo-Json -Depth 5 -Compress
}
