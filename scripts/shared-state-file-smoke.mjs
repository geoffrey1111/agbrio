// Real Windows reader-denied replacement; only two disposable local JSON files.
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {mkdtemp,mkdir,writeFile,readFile,rename,unlink,rmdir} from 'node:fs/promises';
import {join,resolve} from 'node:path';
import {replaceStateFile} from '../src-tauri/resources/codex-shared/state-file.mjs';
assert.equal(process.platform,'win32');
const scratch=resolve('runtime/shared-native-compatibility');await mkdir(scratch,{recursive:true});
const directory=await mkdtemp(join(scratch,'state-file-')),target=join(directory,'ready.json'),pending=target+'.pending';
let child,closed;
try{
 await writeFile(target,'{"ready":false}');await writeFile(pending,'{"ready":true}');
 const code=`$ErrorActionPreference='Stop';$stream=[IO.File]::Open('${target.replaceAll("'","''")}',[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read);try{[Console]::WriteLine('LOCKED');$null=[Console]::ReadLine()}finally{$stream.Dispose()}`;
 child=spawn(join(process.env.WINDIR,'System32/WindowsPowerShell/v1.0/powershell.exe'),['-NoProfile','-NonInteractive','-EncodedCommand',Buffer.from(code,'utf16le').toString('base64')],{windowsHide:true,stdio:['pipe','pipe','ignore']});
 closed=new Promise(yes=>child.once('close',yes));
 await new Promise((yes,no)=>{const timer=setTimeout(()=>no(Error('LOCK_FIXTURE_NOT_READY')),8000);child.stdout.once('data',data=>{clearTimeout(timer);assert.equal(data.toString().trim(),'LOCKED');yes();});});
 let failure;try{await rename(pending,target);}catch(error){failure=error.code;}
 assert.ok(['EPERM','EACCES','EBUSY'].includes(failure),'Existing atomic replacement must fail with a live Windows reader');
 assert.equal(JSON.parse(await readFile(target,'utf8')).ready,false);
 const updated=replaceStateFile(target,{ready:true});
 setTimeout(()=>child.stdin.end('\n'),500);
 await updated;await closed;child=null;assert.equal(JSON.parse(await readFile(target,'utf8')).ready,true);
 console.log(JSON.stringify({readerDeniedReplacementReproduced:true,errorCode:failure,replacementAfterReaderRelease:'PASS',actualBoundedRetry:'PASS',desktopStarts:0,modelRequests:0}));
}finally{
 if(child){child.stdin.end('\n');await closed;}
 for(const p of [pending,target])await unlink(p).catch(error=>{if(error.code!=='ENOENT')throw error;});await rmdir(directory);
}
