[CmdletBinding()]
param()
$ErrorActionPreference='Stop'
$repo=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$evidence=Join-Path $repo 'runtime/published-board-native'
New-Item -ItemType Directory -Path $evidence -Force | Out-Null
$buildLog=Join-Path $evidence 'build.jsonl'
Push-Location $repo
try {
 & cargo test --manifest-path src-tauri/Cargo.toml --lib --features native-board-acceptance --no-run --message-format=json 1> $buildLog
 if($LASTEXITCODE -ne 0){throw 'NATIVE_PRESENTATION_TEST_BUILD_FAILED'}
 $artifacts=@(Get-Content -LiteralPath $buildLog | ForEach-Object {try {$_ | ConvertFrom-Json} catch {}} | Where-Object {$_.reason -eq 'compiler-artifact' -and $_.target.name -eq 'ai_work_router_lib' -and $_.profile.test -and $_.executable})
 if($artifacts.Count -ne 1){throw 'NATIVE_PRESENTATION_TEST_IDENTITY_UNCONFIRMED'}
 $binary=[IO.Path]::GetFullPath([string]$artifacts[0].executable)
 if([IO.Path]::GetFileName($binary) -notmatch '^ai_work_router_lib-[a-f0-9]+\.exe$'){throw 'NATIVE_PRESENTATION_TEST_IDENTITY_UNCONFIRMED'}
 # Cargo lib tests omit the desktop executable's common-controls activation
 # manifest. Embed it into this disposable test binary, never a product package.
 $kits=(Get-ItemProperty 'HKLM:/SOFTWARE/Microsoft/Windows Kits/Installed Roots').KitsRoot10
 $mt=Get-ChildItem -Path (Join-Path $kits 'bin/*/x64/mt.exe') | Sort-Object FullName -Descending | Select-Object -First 1
 if(!$mt){throw 'WINDOWS_SDK_MANIFEST_TOOL_REQUIRED'}
 $manifest=Join-Path $evidence 'test.manifest'
 '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0"><assemblyIdentity type="win32" name="Agbrio.Presentation.Check" version="1.0.0.0"/><dependency><dependentAssembly><assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/></dependentAssembly></dependency></assembly>' | Set-Content -LiteralPath $manifest -Encoding utf8
 & $mt.FullName -nologo -manifest $manifest ('-outputresource:'+$binary+';#1')
 if($LASTEXITCODE -ne 0){throw 'NATIVE_PRESENTATION_TEST_MANIFEST_FAILED'}
 & $binary 'published_boards::native_acceptance::native_published_frame_reads_presentation_but_cannot_reach_parent_or_ipc' --ignored --exact --nocapture 2>&1 | Tee-Object -FilePath (Join-Path $evidence 'proof.log')
 if($LASTEXITCODE -ne 0 -or !(Select-String -LiteralPath (Join-Path $evidence 'proof.log') -Pattern 'NATIVE_PUBLISHED_BOARD_PASS' -Quiet)){throw 'NATIVE_PRESENTATION_GATE_FAILED'}
} finally {Pop-Location}
