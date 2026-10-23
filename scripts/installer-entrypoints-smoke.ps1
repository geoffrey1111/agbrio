param([string]$UtilsPath='src-tauri/target/release/nsis/x64/utils.nsh')
$ErrorActionPreference='Stop'
# Execute the actual production NSIS repair macro only in isolated fixture
# directories. Never use the real Desktop, registry, Startup or pinned folder.
$utils=(Resolve-Path -LiteralPath $UtilsPath).Path
$hook=(Resolve-Path -LiteralPath 'src-tauri/brand-installer-hooks.nsh').Path
$root=Join-Path (Get-Location).Path ('runtime/installer-entrypoints-'+[guid]::NewGuid().ToString('N'))
$old=Join-Path $root 'Legacy App With Spaces'
$current=Join-Path $root 'Current App With Spaces'
$links=Join-Path $root 'Fixture Links'
New-Item -ItemType Directory -Path $old,$current,$links -Force|Out-Null
$oldExe=Join-Path $old 'ai-work-router.exe'
$newExe=Join-Path $current 'ai-work-router.exe'
[IO.File]::WriteAllText($oldExe,'fixture old executable, never run')
[IO.File]::WriteAllText($newExe,'fixture new executable, never run')
$shell=New-Object -ComObject WScript.Shell
try {
 foreach($name in @('Agbrio','AI Work Router','Custom name','Pinned entry','Startup entry')){
  $link=$shell.CreateShortcut((Join-Path $links ($name+'.lnk')))
  $link.TargetPath=$oldExe;$link.Arguments='--fixture-preserve';$link.Save()
 }
 $foreign=Join-Path $links 'Unrelated.lnk'
 $link=$shell.CreateShortcut($foreign);$link.TargetPath=Join-Path $env:WINDIR 'System32/notepad.exe';$link.Save()
 $foreignHash=(Get-FileHash -LiteralPath $foreign).Hash
 $binary=Join-Path $root 'fixture.exe'
 $fixture=Join-Path $root 'fixture.nsi'
 $source=@'
Unicode true
RequestExecutionLevel user
SilentInstall silent
Name "Agbrio entrypoint fixture"
OutFile "__BINARY__"
!include LogicLib.nsh
!include FileFunc.nsh
!include "Win\COM.nsh"
!include "__UTILS__"
!define MAINBINARYNAME "ai-work-router"
!define MANUKEY "Software\AgbrioIsolatedFixture"
!include "__HOOK__"
Section
 !insertmacro AGBRIO_REPAIR_LINKS "__LINKS__" "__OLD__" "__NEW__"
SectionEnd
'@
 foreach($pair in @(@('__BINARY__',$binary),@('__UTILS__',$utils),@('__HOOK__',$hook),@('__LINKS__',$links),@('__OLD__',$old),@('__NEW__',$current))){$source=$source.Replace($pair[0],$pair[1])}
 [IO.File]::WriteAllText($fixture,$source,[Text.UTF8Encoding]::new($false))
 $targetDirectory=Split-Path (Split-Path (Split-Path (Split-Path $utils)))
 $makensis=Join-Path $targetDirectory '.tauri/NSIS/makensis.exe'
 if(!(Test-Path -LiteralPath $makensis)){$makensis=Join-Path $env:LOCALAPPDATA 'tauri/NSIS/makensis.exe'}
 & $makensis /V2 $fixture
 if($LASTEXITCODE -ne 0){throw 'NSIS_FIXTURE_COMPILE_FAILED'}
 foreach($round in 1..2){
  $run=Start-Process -FilePath $binary -WindowStyle Hidden -PassThru -Wait
  if($run.ExitCode -ne 0){throw 'NSIS_FIXTURE_FAILED'}
  foreach($file in Get-ChildItem -LiteralPath $links -Filter '*.lnk'){
   if($file.FullName -eq $foreign){continue}
   $link=$shell.CreateShortcut($file.FullName)
   if($link.TargetPath -ine $newExe -or $link.Arguments -cne '--fixture-preserve'){throw 'REPAIRED_LINK_TARGET_OR_ARGUMENTS_WRONG'}
  }
  if((Get-FileHash -LiteralPath $foreign).Hash -cne $foreignHash){throw 'FOREIGN_SHORTCUT_CHANGED'}
 }
 @{productionNSISRepair='PASS';repairedLinks=5;pathWithSpaces=$true;repeatSafe=$true;argumentsPreserved=$true;unrelatedPreserved=$true;realShellChanges=0}|ConvertTo-Json -Compress
} finally {
 if($link){[void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($link)}
 [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($shell)
}
# Fixture remains under runtime on failure for diagnosis; remove after PASS.
if((Resolve-Path -LiteralPath $root).Path.StartsWith((Join-Path (Get-Location).Path 'runtime/installer-entrypoints-'),[StringComparison]::OrdinalIgnoreCase)){
 Remove-Item -LiteralPath $root -Recurse -Force
 if(Test-Path -LiteralPath $root){throw 'FIXTURE_CLEANUP_FAILED'}
}
