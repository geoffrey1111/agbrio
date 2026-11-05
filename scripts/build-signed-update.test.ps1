$ErrorActionPreference='Stop'
$parseErrors=$null
$ast=[System.Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'build-signed-update.ps1'),[ref]$null,[ref]$parseErrors)
if($parseErrors.Count){throw 'Publisher script parse failed'}
$fn=$ast.Find({param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq 'Get-SnapshotFrontendDirectory'},$true)
if(!$fn){throw 'Snapshot path function missing'}
. ([scriptblock]::Create($fn.Extent.Text))
foreach($case in @(
 @('D:/project','D:/project/runtime/review-v7','../runtime/review-v7'),
 @('D:/project with spaces','D:/project with spaces/runtime/ui v7','../runtime/ui v7'),
 @('D:/project','D:/project/runtime/snapshot/sub','../runtime/snapshot/sub')
)) {
 $relative=Get-SnapshotFrontendDirectory $case[0] $case[1]
 if($relative -cne $case[2] -or $relative.Contains(':') -or [IO.Path]::IsPathRooted($relative)){throw 'Snapshot became an absolute frontend URL'}
 if([IO.Path]::GetFullPath((Join-Path (Join-Path $case[0] 'src-tauri') $relative)) -ine [IO.Path]::GetFullPath($case[1])){throw 'Snapshot resolved to a different directory'}
}
$rejected=$false
try{Get-SnapshotFrontendDirectory 'C:/project' 'D:/project/runtime/review' | Out-Null}catch{$rejected=$true}
if(!$rejected){throw 'Cross-drive absolute frontend URL was not rejected'}
'PASS: snapshot directory roundtrip, spaces and nested assets; cross-drive URL rejected. No build, signature or installer invoked.'
