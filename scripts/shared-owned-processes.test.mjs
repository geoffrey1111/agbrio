import {test} from 'node:test';
import assert from 'node:assert/strict';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {resolve} from 'node:path';
const exec=promisify(execFile);
const source=resolve('src-tauri/resources/codex-shared/owned-processes.ps1').replaceAll("'","''");
async function run(body){
 const script=`$ErrorActionPreference='Stop';. '${source}'
 $directory='D:\\fixture';$owner='S-1-fixture';$script:identities=@{};$script:stopped=@();$script:table=@()
 function Get-SharedProcessIdentity([uint32]$ProcessId,[string]$Directory){return $script:identities[$ProcessId]}
 function Get-SharedProcessTable{return $script:table}
 function Stop-SharedProcess([uint32]$ProcessId){$script:stopped+=@($ProcessId);$script:identities.Remove($ProcessId)}
 $root=@{pid=[uint32]100;createdAt='1000';ownerSid=$owner;imagePath='D:\\fixture\\runtime\\node.exe'}
 $child=@{pid=[uint32]101;createdAt='1001';ownerSid=$owner;imagePath='D:\\fixture\\native\\codex.exe'}
 $script:identities[[uint32]100]=$root;$script:identities[[uint32]101]=$child
 $script:table=@(@{ProcessId=[uint32]101;ParentProcessId=[uint32]100;Name='codex.exe'})
 ${body}`;
 const result=await exec('powershell.exe',['-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-Command',script],{windowsHide:true,timeout:15000,maxBuffer:16384});return JSON.parse(result.stdout);
}
test('captured child remains removable after its root exits; unrelated processes are absent',async()=>{
 const r=await run(`$snapshot=Get-SharedStopSnapshot $root $directory $owner;$script:identities.Remove([uint32]100);$result=Invoke-SharedStopSnapshot $snapshot $root $directory $owner;@{children=@($snapshot.children).Count;stopped=@($script:stopped);ok=$result.stopped}|ConvertTo-Json -Compress`);
 assert.deepEqual(r,{children:1,stopped:[101],ok:true});
});
test('recycled child PID is preserved even when its old parent has disappeared',async()=>{
 const r=await run(`$snapshot=Get-SharedStopSnapshot $root $directory $owner;$script:identities.Remove([uint32]100);$script:identities[[uint32]101]=@{pid=[uint32]101;createdAt='2000';ownerSid=$owner;imagePath=$child.imagePath};$result=Invoke-SharedStopSnapshot $snapshot $root $directory $owner;@{stopped=@($script:stopped);ok=$result.stopped;alive=$script:identities.ContainsKey([uint32]101)}|ConvertTo-Json -Compress`);
 assert.deepEqual(r,{stopped:[],ok:true,alive:true});
});
test('Desktop, foreign owners and stale parent identities are excluded from capture',async()=>{
 const r=await run(`$script:table=@(@{ProcessId=[uint32]101;ParentProcessId=[uint32]100;Name='ChatGPT.exe'},@{ProcessId=[uint32]102;ParentProcessId=[uint32]100;Name='node.exe'},@{ProcessId=[uint32]103;ParentProcessId=[uint32]100;Name='node.exe'});$script:identities[[uint32]102]=@{pid=[uint32]102;createdAt='1002';ownerSid='S-1-other';imagePath='D:\\other\\node.exe'};$script:identities[[uint32]103]=@{pid=[uint32]103;createdAt='999';ownerSid=$owner;imagePath='D:\\old\\node.exe'};$snapshot=Get-SharedStopSnapshot $root $directory $owner;@{children=@($snapshot.children).Count}|ConvertTo-Json -Compress`);
 assert.equal(r.children,0);
});
test('a snapshot for another root or owner cannot stop any process',async()=>{
 const r=await run(`$snapshot=Get-SharedStopSnapshot $root $directory $owner;$snapshot.root=@{pid=[uint32]200;createdAt='1000';ownerSid=$owner;imagePath=$root.imagePath};try{Invoke-SharedStopSnapshot $snapshot $root $directory $owner|Out-Null;throw 'test-failed'}catch{if($_.Exception.Message -ne 'SHARED_PROCESS_SNAPSHOT_INVALID'){throw}};@{stopped=@($script:stopped)}|ConvertTo-Json -Compress`);
 assert.deepEqual(r.stopped,[]);
});

test('CIM microsecond precision retains the exact native child identity',async()=>{
 const r=await run(`$stamp=[datetime]::SpecifyKind([datetime]'2026-10-13T01:00:00',[DateTimeKind]::Utc);$ticks=$stamp.ToFileTimeUtc();$child.createdAt=([long]$ticks+5).ToString();$script:table[0].CreationDate=$stamp;$snapshot=Get-SharedStopSnapshot $root $directory $owner;$result=Invoke-SharedStopSnapshot $snapshot $root $directory $owner;@{children=@($snapshot.children).Count;captured=$snapshot.children[0].createdAt;expected=$child.createdAt;stopped=@($script:stopped)}|ConvertTo-Json -Compress`);
 assert.equal(r.children,1);assert.equal(r.captured,r.expected);assert.deepEqual(r.stopped,[100,101]);
});

test('a genuine table/native creation mismatch still excludes the child',async()=>{
 const r=await run(`$stamp=[datetime]::SpecifyKind([datetime]'2026-10-13T01:00:00',[DateTimeKind]::Utc);$child.createdAt=([long]$stamp.ToFileTimeUtc()+10).ToString();$script:table[0].CreationDate=$stamp;$snapshot=Get-SharedStopSnapshot $root $directory $owner;@{children=@($snapshot.children).Count}|ConvertTo-Json -Compress`);
 assert.equal(r.children,0);
});
