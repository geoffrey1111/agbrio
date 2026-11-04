$ErrorActionPreference='Stop'
$scriptPath=Join-Path $PSScriptRoot 'sign-macos-update.ps1'
$tokens=$null;$errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile($scriptPath,[ref]$tokens,[ref]$errors)
if($errors.Count){throw 'Signer PowerShell syntax invalid'}
# Load only the task-owned child-process helper, never the publisher script body.
$helper=$ast.FindAll({param($node)$node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Run-PrivateChild'},$true)
if($helper.Count -ne 1){throw 'Expected private child helper'}
. ([scriptblock]::Create($helper[0].Extent.Text))
$nodePath=(Get-Command node -ErrorAction Stop).Source
$saved=$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD
try{
 $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD='fixture-only-not-a-credential'
 Run-PrivateChild $nodePath @('-e','if(process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD) process.exit(9); console.log("EXPECTED_VERIFIER_MARKER")') @{} 'EXPECTED_VERIFIER_MARKER'
 $rejected=$false
 try{Run-PrivateChild $nodePath @('-e','process.exit(0)') @{} 'EXPECTED_VERIFIER_MARKER'}catch{$rejected=$true}
 if(!$rejected){throw 'Zero-exit/zero-tests must not claim signature verification'}
 Run-PrivateChild $nodePath @('-e','if(process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD!=="fixture-child-only") process.exit(8)') @{TAURI_SIGNING_PRIVATE_KEY_PASSWORD='fixture-child-only'}
}finally{$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD=$saved}
$savedGithub=$env:GITHUB_ACTIONS
try{
 $env:GITHUB_ACTIONS='true'
 $output=& pwsh -NoProfile -File $scriptPath -Version 0.1.1 -SourceSha aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa -SourceRef codex/macos-support -RunId 1 -ArtifactId 1 -ArtifactZip D:/missing.zip -OutputDirectory D:/missing-output -SigningCredentialsDirectory D:/missing-credentials 2>&1
 if($LASTEXITCODE -eq 0 -or -not (($output -join "`n").Contains('publisher-local only'))){throw 'CI guard failed before credential access'}
}finally{$env:GITHUB_ACTIONS=$savedGithub}
'MAC_SIGNER_CHILD_ISOLATION_AND_MARKER_GUARDS_PASS; no signing credentials accessed.'
