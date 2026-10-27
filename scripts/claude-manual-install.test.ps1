$ErrorActionPreference='Stop'
$project=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$source=Join-Path $project 'src/features/workbench/claudeManualInstall.ps1'
$tokens=$null;$parseErrors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile($source,[ref]$tokens,[ref]$parseErrors)
if($parseErrors.Count){throw $parseErrors[0]}
# Evaluate only exact function definitions, never the real installation entry.
foreach($definition in $ast.FindAll({param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst]},$false)) {
 . ([scriptblock]::Create($definition.Extent.Text))
}
$boundary=Join-Path $project 'runtime/claude-onboarding-recovery-20261027'
$fixture=Join-Path $boundary ('manual-fixture-'+[guid]::NewGuid())
$savedLocal=$env:LOCALAPPDATA;$savedRoaming=$env:APPDATA
try {
 $env:LOCALAPPDATA=Join-Path $fixture 'Local';$env:APPDATA=Join-Path $fixture 'Roaming'
 $app=Join-Path $fixture 'Agbrio'
 $manifestRoot=Join-Path $app 'claude-code-plugin/.claude-plugin'
 New-Item -ItemType Directory -Path $manifestRoot -Force | Out-Null
 '{"name":"agbrio-bridge","version":"0.1.12"}'|Set-Content (Join-Path $manifestRoot 'plugin.json')
 '{"name":"agbrio-observation"}'|Set-Content (Join-Path $manifestRoot 'marketplace.json')
 foreach($path in @((Join-Path $env:LOCALAPPDATA 'Claude-Data/claude_pzs8sxrjxfjjc/claude-code/2.1.308/build/claude.exe'),(Join-Path $env:APPDATA 'Claude/claude-code/2.1.300/build/claude.exe'))) {
  New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($path)) -Force|Out-Null
  [IO.File]::WriteAllText($path,'not executable; CLI calls are mocked')
 }
 $script:apps=@([pscustomobject]@{ExecutablePath=(Join-Path $app 'ai-work-router.exe')})
 function Get-CimInstance {param($ClassName,$Filter) if($ClassName -ne 'Win32_Process' -or $Filter -ne "Name='ai-work-router.exe'"){throw 'Unexpected process query'};return $script:apps}
 $script:calls=[Collections.Generic.List[object]]::new();$script:installed=$false;$script:enabled=$false;$script:failInstall=$false
 function Invoke-AgbrioPluginCli {
  param([string]$Executable,[string[]]$CliArguments)
  if($Executable -notlike '*Claude-Data*2.1.308*'){throw 'Did not choose newest modern engine'}
  $script:calls.Add(@($CliArguments))
  if($CliArguments -contains 'configure'){throw 'Manual installation must not configure credentials'}
  switch($CliArguments[1]) {
   'marketplace' {return '{"ok":true}'}
   'list' {if(!$script:installed){return '[]'};return (ConvertTo-Json -Compress -InputObject @(@{id='agbrio-bridge@agbrio-observation';scope='user';enabled=$script:enabled;version='0.1.12'}))}
   'install' {if($script:failInstall){throw 'fixture original install error'};$script:installed=$true;return '{"ok":true}'}
   'update' {return '{"ok":true}'}
   'enable' {$script:enabled=$true;return '{"ok":true}'}
   default {throw 'Unexpected CLI operation'}
  }
 }
 Install-AgbrioClaudePlugin | Out-Null
 if(!$script:installed -or !$script:enabled -or @($script:calls|Where-Object {$_[1] -eq 'install'}).Count -ne 1){throw 'Fresh install not completed'}
 $script:calls.Clear();Install-AgbrioClaudePlugin|Out-Null
 if(@($script:calls|Where-Object {$_[1] -eq 'update'}).Count -ne 1 -or @($script:calls|Where-Object {$_[1] -eq 'install'}).Count){throw 'Existing installation was not updated'}
 $script:calls.Clear();$script:installed=$false;$script:failInstall=$true;$failed=$false
 try{Install-AgbrioClaudePlugin|Out-Null}catch{if($_.Exception.Message -ne 'fixture original install error'){throw};$failed=$true}
 if(!$failed -or $script:calls[-1][1] -ne 'install'){throw 'Failed install continued'}
 $script:failInstall=$false;$script:calls.Clear()
 '{"name":"foreign-plugin"}'|Set-Content (Join-Path $manifestRoot 'plugin.json')
 $failed=$false;try{Install-AgbrioClaudePlugin|Out-Null}catch{if($_.Exception.Message -notlike 'Unexpected bundled plugin*'){throw};$failed=$true}
 if(!$failed -or $script:calls.Count){throw 'Foreign package executed'}
 '{"name":"agbrio-bridge"}'|Set-Content (Join-Path $manifestRoot 'plugin.json')
 $script:apps=@();$failed=$false;try{Install-AgbrioClaudePlugin|Out-Null}catch{if($_.Exception.Message -notlike 'Keep one installed*'){throw};$failed=$true}
 if(!$failed -or $script:calls.Count){throw 'Missing app was ignored'}
 $script:apps=@([pscustomobject]@{ExecutablePath=(Join-Path $app 'ai-work-router.exe')})
 $modern=Join-Path $env:LOCALAPPDATA 'Claude-Data/claude_pzs8sxrjxfjjc/claude-code/2.1.308/build/claude.exe'
 Remove-Item -LiteralPath $modern
 $legacy=Join-Path $env:APPDATA 'Claude/claude-code/2.1.300/build/claude.exe';Remove-Item -LiteralPath $legacy
 $failed=$false;try{Install-AgbrioClaudePlugin|Out-Null}catch{if($_.Exception.Message -notlike 'Open a local Code*'){throw};$failed=$true}
 if(!$failed -or $script:calls.Count){throw 'Missing engine was ignored'}
 Write-Output '{"checks":6,"nativeCommandsExecuted":0,"credentialWrites":0}'
}finally {
 $env:LOCALAPPDATA=$savedLocal;$env:APPDATA=$savedRoaming
 if(Test-Path -LiteralPath $fixture){
  $resolved=(Resolve-Path -LiteralPath $fixture).Path
  if([IO.Path]::GetDirectoryName($resolved) -cne (Resolve-Path -LiteralPath $boundary).Path){throw 'Cleanup boundary mismatch'}
  Remove-Item -LiteralPath $resolved -Recurse -Force
  if(Test-Path -LiteralPath $resolved){throw 'Fixture cleanup incomplete'}
 }
}
