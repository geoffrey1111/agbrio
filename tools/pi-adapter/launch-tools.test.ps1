$ErrorActionPreference='Stop'
$ast=[System.Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'elevated.ps1'),[ref]$null,[ref]$null)
$fn=$ast.Find({param($node) $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Test-AgbrioPiToolList'},$true)
if(!$fn){throw 'Missing launch tool validator'}
. ([scriptblock]::Create($fn.Extent.Text))
$resources=[pscustomobject]@{mcpServers=@('unityMCP')}
foreach($case in @(
 @('read,powershell,mcp__unityMCP__*',$true,$true,$true),
 @('read,grep,find,ls',$false,$false,$true),
 @('read,mcp__unityMCP__*',$false,$true,$false),
 @('read,mcp__unityMCP__*',$true,$false,$false),
 @('read,mcp__other__*',$true,$true,$false),
 @('read,mcp__unityMCPevil__*',$true,$true,$false),
 @('read,mcp__unityMCP__*,mcp__other__*',$true,$true,$false),
 @('read,mcp__*',$true,$true,$false),
 @('read,*',$true,$true,$false),
 @('read,',$true,$true,$false),
 @('read,bash',$true,$true,$false)
)) {
 if((Test-AgbrioPiToolList $case[0] $resources $case[1] $case[2]) -ne $case[3]) {throw "Unexpected tool validation: $($case[0])"}
}
'11 launch tool namespace/permission cases PASS; no child process or UAC started.'
