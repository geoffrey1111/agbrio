[CmdletBinding()]
param([Parameter(Mandatory)][string]$Executable,[Parameter(Mandatory)][ValidatePattern('^[a-fA-F0-9]{64}$')][string]$ExpectedSha256)
$ErrorActionPreference='Stop'
$path=(Resolve-Path -LiteralPath $Executable).Path
$bytes=[IO.File]::ReadAllBytes($path)
$actual=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()
$transition=$false
if($actual -cne $ExpectedSha256.ToLowerInvariant()){
 $marker='__TAURI_BUNDLE_TYPE_VAR_NSS'
 $ascii=[Text.Encoding]::Latin1.GetString($bytes)
 $index=$ascii.IndexOf($marker,[StringComparison]::Ordinal)
 if($index -lt 0 -or $ascii.LastIndexOf($marker,[StringComparison]::Ordinal) -ne $index){throw 'NATIVE_BUNDLE_BYTES_CHANGED'}
 [Array]::Copy([Text.Encoding]::ASCII.GetBytes('UNK'),0,$bytes,$index+$marker.Length-3,3)
 $normalized=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()
 if($normalized -cne $ExpectedSha256.ToLowerInvariant()){throw 'NATIVE_BUNDLE_BYTES_CHANGED'}
 $transition=$true
}
[pscustomobject]@{expectedBuildSha256=$ExpectedSha256.ToLowerInvariant();bundledNativeSha256=$actual;onlyDocumentedBundleMarkerChanged=$transition;nativeBytesOtherwiseIdentical=$true}
