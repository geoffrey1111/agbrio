[CmdletBinding()]
param([Parameter(Mandatory=$true)][ValidatePattern('^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?$')][string]$Version,[string]$NotesFile='',[ValidateSet('lzma','zlib')][string]$Compression='lzma',[string]$HostedControlOrigin=$env:AGBRIO_HOSTED_CONTROL_ORIGIN,[switch]$SelfHostedOnly,[string]$SigningCredentialsDirectory='',[switch]$BundleOnly,[ValidatePattern('^[a-fA-F0-9]{64}$')][string]$ExpectedExecutableSha256)
$ErrorActionPreference='Stop'
$baseVersion=[version]($Version.Split('-')[0])
if($baseVersion -lt [version]'0.1.1'){throw 'The first signed update must use version 0.1.1 or higher; installed 0.1.0 clients cannot update to a 0.1.0 prerelease'}
if($SelfHostedOnly -and $HostedControlOrigin){throw 'Choose a hosted origin or explicit SelfHostedOnly, not both'}
if(!$SelfHostedOnly){
 if(!$HostedControlOrigin){throw 'Set HostedControlOrigin for a hosted-code publisher build, or explicitly choose SelfHostedOnly; never silently remove hosted activation'}
 $publisherOrigin=$null
 if(![Uri]::TryCreate($HostedControlOrigin,[UriKind]::Absolute,[ref]$publisherOrigin) -or $publisherOrigin.Scheme -ne 'https' -or !$publisherOrigin.IsDefaultPort -or $publisherOrigin.UserInfo -or $publisherOrigin.AbsolutePath -ne '/' -or $publisherOrigin.Query -or $publisherOrigin.Fragment){throw 'HostedControlOrigin must be an HTTPS origin without credentials, path or query'}
 $HostedControlOrigin=$publisherOrigin.GetLeftPart([UriPartial]::Authority)
}
$repoRoot=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
# An absolute existing Cargo target permits reuse without creating another huge
# target tree. Relative Cargo paths depend on its child cwd, so reject ambiguity.
if($env:CARGO_TARGET_DIR -and ![IO.Path]::IsPathRooted($env:CARGO_TARGET_DIR)){throw 'CARGO_TARGET_DIR must be absolute for signed artifact readback'}
$buildTarget=if($env:CARGO_TARGET_DIR){[IO.Path]::GetFullPath($env:CARGO_TARGET_DIR)}else{Join-Path $repoRoot 'src-tauri/target'}
if($BundleOnly){
 if(!$ExpectedExecutableSha256){throw 'BundleOnly requires the independently verified existing executable SHA256'}
 $existingExecutable=Join-Path $buildTarget 'release/ai-work-router.exe'
 if(!(Test-Path -LiteralPath $existingExecutable) -or (Get-FileHash -LiteralPath $existingExecutable).Hash.ToLowerInvariant() -cne $ExpectedExecutableSha256.ToLowerInvariant()){throw 'Existing executable identity mismatch; no bundling permitted'}
 if((Get-Item -LiteralPath $existingExecutable).VersionInfo.ProductVersion -cne $Version){throw 'Existing executable version differs; build the new native version first'}
}
$releaseRoot=Join-Path $repoRoot ('runtime/signed-release-'+$Version)
if(Test-Path -LiteralPath $releaseRoot){throw 'Preserve the existing release directory; choose a new version'}
New-Item -ItemType Directory -Path $releaseRoot | Out-Null
$savedPrivate=$env:TAURI_SIGNING_PRIVATE_KEY;$savedPassword=$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD;$savedVersion=$env:VITE_AGBRIO_APP_VERSION;$savedHostedOrigin=$env:AGBRIO_HOSTED_CONTROL_ORIGIN
try{
 $env:AGBRIO_HOSTED_CONTROL_ORIGIN=if($SelfHostedOnly){$null}else{$HostedControlOrigin}
 if(!$env:TAURI_SIGNING_PRIVATE_KEY){
  $privateRoot=if($SigningCredentialsDirectory){(Resolve-Path -LiteralPath $SigningCredentialsDirectory).Path}else{Join-Path $repoRoot 'runtime/desktop-update-20261008/private'}
  $keyPath=Join-Path $privateRoot 'agbrio-updater.key';$passwordPath=Join-Path $privateRoot 'signing-password.dpapi'
  if(!(Test-Path -LiteralPath $keyPath) -or !(Test-Path -LiteralPath $passwordPath)){throw 'Set TAURI_SIGNING_PRIVATE_KEY and TAURI_SIGNING_PRIVATE_KEY_PASSWORD securely before a publisher build'}
  $env:TAURI_SIGNING_PRIVATE_KEY=$keyPath
  $securePassword=(Get-Content -LiteralPath $passwordPath -Raw).Trim() | ConvertTo-SecureString
  $secretPointer=[Runtime.InteropServices.Marshal]::SecureStringToBSTR($securePassword)
  try{$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD=[Runtime.InteropServices.Marshal]::PtrToStringBSTR($secretPointer)}finally{[Runtime.InteropServices.Marshal]::ZeroFreeBSTR($secretPointer)}
 }
 $env:VITE_AGBRIO_APP_VERSION=$Version
 $overlay=Join-Path $releaseRoot 'signed-build.json'
 @{version=$Version;bundle=@{createUpdaterArtifacts=$true;windows=@{nsis=@{compression=$Compression}}}} | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $overlay -Encoding utf8
 Push-Location $repoRoot
 try{
  if($BundleOnly){& npm run tauri -- bundle --bundles nsis --config $overlay}else{& npm run build:windows -- --config $overlay}
  if($LASTEXITCODE -ne 0){throw 'Signed build failed; nothing was published'}
  if($BundleOnly -and (Get-FileHash -LiteralPath $existingExecutable).Hash.ToLowerInvariant() -cne $ExpectedExecutableSha256.ToLowerInvariant()){throw 'Bundling changed the native executable; inspect instead of publishing'}
  $installer=Join-Path $buildTarget ('release/bundle/nsis/Agbrio_'+$Version+'_x64-setup.exe')
  if(!(Test-Path -LiteralPath ($installer+'.sig'))){throw 'No updater signature was generated; do not publish as an in-app update'}
  Copy-Item -LiteralPath $installer,($installer+'.sig') -Destination $releaseRoot
  $arguments=@('scripts/create-update-manifest.mjs','--version',$Version,'--installer',(Join-Path $releaseRoot (Split-Path $installer -Leaf)),'--output',(Join-Path $releaseRoot 'latest.json'))
  if($NotesFile){$arguments+=@('--notes',$NotesFile)}
  & node @arguments
  if($LASTEXITCODE -ne 0){throw 'Manifest generation failed; nothing was published'}
 }finally{Pop-Location}
 Write-Output ('Signed artifacts prepared locally: '+$releaseRoot)
 Write-Output 'Not published. Upload the exact installer, its signature and latest.json only after reviewing the release.'
}finally{$env:TAURI_SIGNING_PRIVATE_KEY=$savedPrivate;$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD=$savedPassword;$env:VITE_AGBRIO_APP_VERSION=$savedVersion;$env:AGBRIO_HOSTED_CONTROL_ORIGIN=$savedHostedOrigin}
