import {test} from 'node:test';
import assert from 'node:assert/strict';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {resolve} from 'node:path';
const exec=promisify(execFile);
test('actual installer lifecycle harness refuses local execution before inspecting or mutating files',async()=>{
 await assert.rejects(exec('pwsh.exe',['-NoProfile','-NonInteractive','-File',resolve('scripts/windows-shared-install-smoke.ps1'),'-BaselineInstaller','nonexistent-baseline','-CandidateInstaller','nonexistent-candidate','-Version','0.1.35-rc.2'],{windowsHide:true,timeout:10000,env:{...process.env,GITHUB_ACTIONS:'false',RUNNER_OS:'Windows',GITHUB_REPOSITORY:'geoffrey1111/agbrio'}}),e=>e.stderr.includes('DISPOSABLE_GITHUB_WINDOWS_RUNNER_REQUIRED'));
});
