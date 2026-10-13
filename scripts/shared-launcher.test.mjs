// The installed shortcut uses Windows PowerShell, independent of default pwsh.
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {join,resolve} from 'node:path';
const exec=promisify(execFile);
test('Windows PowerShell parses all packaged scripts and retains Chinese launcher literals',async()=>{
 const source=resolve('src-tauri/resources/codex-shared').replaceAll("'","''");
 const command=`$ErrorActionPreference='Stop';$errors=@();$chinese=$false;Get-ChildItem -LiteralPath '${source}' -Filter *.ps1|ForEach-Object {$tokens=$null;$parse=$null;$ast=[Management.Automation.Language.Parser]::ParseFile($_.FullName,[ref]$tokens,[ref]$parse);$errors+=@($parse|ForEach-Object {$_.ErrorId});if($_.Name -eq 'launch.ps1'){$chinese=@($ast.FindAll({param($n) $n -is [Management.Automation.Language.StringConstantExpressionAst] -and $n.Value.StartsWith([char]0x666e+[string][char]0x901a)},$true)).Count -gt 0}};@{errors=$errors;chinese=$chinese}|ConvertTo-Json -Compress`;
 const result=JSON.parse((await exec(join(process.env.WINDIR,'System32/WindowsPowerShell/v1.0/powershell.exe'),['-NoProfile','-NonInteractive','-Command',command],{windowsHide:true,timeout:15000,maxBuffer:16384})).stdout);
 assert.deepEqual(result.errors,[]);assert.equal(result.chinese,true);
});
