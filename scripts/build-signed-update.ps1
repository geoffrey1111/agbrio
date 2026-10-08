[CmdletBinding()]
param([Parameter(Mandatory=$true)][ValidatePattern('^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?$')][string]$Version,[string]$NotesFile='')
$ErrorActionPreference='Stop'
$baseVersion=[version]($Version.Split('-')[0])
if($baseVersion -lt [version]'0.1.1'){throw 'The first signed update must use version 0.1.1 or higher; installed 0.1.0 clients cannot update to a 0.1.0 prerelease'}
$repoRoot=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$releaseRoot=Join-Path $repoRoot ('runtime/signed-release-'+$Version)
if(Test-Path -LiteralPath $releaseRoot){throw 'Preserve the existing release directory; choose a new version'}
New-Item -ItemType Directory -Path $releaseRoot | Out-Null
$savedPrivate=$env:TAURI_SIGNING_PRIVATE_KEY;$savedPassword=$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD;$savedVersion=$env:VITE_AGBRIO_APP_VERSION
try{
 if(!$env:TAURI_SIGNING_PRIVATE_KEY){
  $privateRoot=Join-Path $repoRoot 'runtime/desktop-update-20261008/private'
  $keyPath=Join-Path $privateRoot 'agbrio-updater.key';$passwordPath=Join-Path $privateRoot 'signing-password.dpapi'
  if(!(Test-Path -LiteralPath $keyPath) -or !(Test-Path -LiteralPath $passwordPath)){throw 'Set TAURI_SIGNING_PRIVATE_KEY and TAURI_SIGNING_PRIVATE_KEY_PASSWORD securely before a publisher build'}
  $env:TAURI_SIGNING_PRIVATE_KEY=$keyPath
  $securePassword=(Get-Content -LiteralPath $passwordPath -Raw).Trim() | ConvertTo-SecureString
  $secretPointer=[Runtime.InteropServices.Marshal]::SecureStringToBSTR($securePassword)
  try{$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD=[Runtime.InteropServices.Marshal]::PtrToStringBSTR($secretPointer)}finally{[Runtime.InteropServices.Marshal]::ZeroFreeBSTR($secretPointer)}
 }
 $env:VITE_AGBRIO_APP_VERSION=$Version
 $overlay=Join-Path $releaseRoot 'signed-build.json'
 @{version=$Version;bundle=@{createUpdaterArtifacts=$true}} | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $overlay -Encoding utf8
 Push-Location $repoRoot
 try{
  & npm run build:windows -- --config $overlay
  if($LASTEXITCODE -ne 0){throw 'Signed build failed; nothing was published'}
  $installer=Join-Path $repoRoot ('src-tauri/target/release/bundle/nsis/Agbrio_'+$Version+'_x64-setup.exe')
  if(!(Test-Path -LiteralPath ($installer+'.sig'))){throw 'No updater signature was generated; do not publish as an in-app update'}
  Copy-Item -LiteralPath $installer,($installer+'.sig') -Destination $releaseRoot
  $arguments=@('scripts/create-update-manifest.mjs','--version',$Version,'--installer',(Join-Path $releaseRoot (Split-Path $installer -Leaf)),'--output',(Join-Path $releaseRoot 'latest.json'))
  if($NotesFile){$arguments+=@('--notes',$NotesFile)}
  & node @arguments
  if($LASTEXITCODE -ne 0){throw 'Manifest generation failed; nothing was published'}
 }finally{Pop-Location}
 Write-Output ('Signed artifacts prepared locally: '+$releaseRoot)
 Write-Output 'Not published. Upload the exact installer, its signature and latest.json only after reviewing the release.'
}finally{$env:TAURI_SIGNING_PRIVATE_KEY=$savedPrivate;$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD=$savedPassword;$env:VITE_AGBRIO_APP_VERSION=$savedVersion}
