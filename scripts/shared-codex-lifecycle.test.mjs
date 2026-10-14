import {test,afterEach} from 'node:test';
import assert from 'node:assert/strict';
import {mkdir,readFile,readdir,unlink,rmdir,lstat} from 'node:fs/promises';
import {resolve,join} from 'node:path';
import {EventEmitter} from 'node:events';
import {randomUUID} from 'node:crypto';
import {lifecycle,atomic,restoreChanges,sharedUrl,desktopLaunchOptions,readDesktopState,mergeStopSnapshots} from '../src-tauri/resources/codex-shared/control.mjs';

const ownedFixtures=new Set();
afterEach(async()=>{for(const directory of ownedFixtures){for(const name of await readdir(directory)){const path=join(directory,name);assert.ok((await lstat(path)).isFile());await unlink(path);}await rmdir(directory);ownedFixtures.delete(directory);}});
async function fixture(){
 const directory=resolve('runtime/shared-gateway-repair/unit',randomUUID());await mkdir(directory,{recursive:true});
 ownedFixtures.add(directory);
 let running=false,desktopConnected=false,turns=0;const env={CODEX_APP_SERVER_WS_URL:null,NODE_EXTRA_CA_CERTS:'original.pem',NODE_USE_SYSTEM_CA:null};const calls=[];
 const options={directory,binary:'fixture-codex.exe',version:'0.160.0',desktop:'fixture-Desktop.exe'};
 const child=()=>{const c=new EventEmitter();c.pid=12345;c.exitCode=null;c.unref=()=>{};c.kill=()=>{c.exitCode=0;c.emit('exit',0);desktopConnected=false;};return c;};
 const dependencies={
  verifyRuntime:async()=>({cohortVerified:true,files:[]}),
  run:async args=>{const action=args[args.indexOf('-Action')+1];calls.push(action);
   if(action==='Snapshot')return {...env};if(action==='DesktopWindow')return {visible:true};
   if(action==='Apply'){env.CODEX_APP_SERVER_WS_URL=args[args.indexOf('-Url')+1];}
   if(action==='Restore'){const j=JSON.parse(await readFile(args[args.indexOf('-SnapshotPath')+1],'utf8'));for(const c of j.changes)if(env[c.name]===c.installed)env[c.name]=c.previous;}
   if(action==='Stop')running=false;return{ok:true};
  },
  exec:async(_file,args)=>({stdout:args[0]==='--version'?'codex-cli 0.160.0':args.includes('-DesktopPid')?JSON.stringify({applied:true}):args.some(arg=>arg.endsWith('desktop-state.ps1'))?JSON.stringify({desktop:'fixture-current-Desktop.exe',count:0}):'S-1-5-21-fixture'}),
  spawn:(_file,args)=>{calls.push(args.length?'spawn-supervisor':'spawn-Desktop');if(args.length)running=true;else desktopConnected=true;return child();},
  fetch:async(url,opts)=>{if(!running)throw Error('refused');if(url.endsWith('/stop')){assert.match(opts.headers.Authorization,/^Bearer [0-9a-f]{64}$/);if(turns)return {ok:false,status:409};running=false;return{ok:true};}const cfg=JSON.parse(await readFile(join(directory,'runtime-config.json'),'utf8'));return{json:async()=>({instance:cfg.instance,ready:true,desktopConnected,activeTurns:turns})};},
  probe:async()=>{calls.push('probe-RPC');assert.equal(env.CODEX_APP_SERVER_WS_URL,null);},
 };
 return{options,dependencies,env,calls,setTurns:n=>turns=n,setDesktop:v=>desktopConnected=v};
}
test('only fixed localhost shared URLs are legacy-owned; unrelated endpoints are preserved',()=>{
 assert.equal(sharedUrl('wss://127.0.0.1:47116/rpc'),true);
 for(const url of ['ws://example.com:47116/rpc','ws://127.0.0.1:47117/rpc','ws://secret@127.0.0.1:47116/rpc','ws://127.0.0.1:47116/rpc?token=secret'])assert.equal(sharedUrl(url),false);
 assert.deepEqual(restoreChanges({A:'original'},{A:'installed'}),[{name:'A',previous:'original',installed:'installed'}]);
});

test('Desktop discovery rejects absent identity, invalid counts and malformed output',async()=>{
 for(const value of [null,{}, {count:-1,desktop:'valid'}, {count:0,desktop:''},{count:'0',desktop:'valid'}])await assert.rejects(readDesktopState('fixture',async()=>({stdout:JSON.stringify(value)})),/SHARED_DESKTOP_BINARY_UNRESOLVED/);
 await assert.rejects(readDesktopState('fixture',async()=>({stdout:'not-json'})),/SHARED_DESKTOP_BINARY_UNRESOLVED/);
});

test('preparation rejects ordinary Desktop before native verification or any write',async()=>{
 const f=await fixture();
 f.dependencies.exec=async()=>({stdout:JSON.stringify({desktop:'fixture-desktop',count:1})});
 f.dependencies.verifyRuntime=()=>assert.fail('Do not inspect or retain native files');
 f.dependencies.atomic=()=>assert.fail('No preparation writes');
 f.dependencies.run=()=>assert.fail('Do not stop or restore anything');
 f.dependencies.spawn=()=>assert.fail('No second writer');
 for(const action of ['prepare-check','setup'])await assert.rejects(lifecycle({...f.options,action},f.dependencies),/SHARED_DESKTOP_CLOSE_REQUIRED/);
 assert.deepEqual(await readdir(f.options.directory),[]);
});

test('preparation rejects unreachable proven-live supervisor before checking Desktop or native files',async()=>{
 const f=await fixture();await lifecycle({...f.options,action:'setup'},f.dependencies);
 const settings=await readFile(join(f.options.directory,'settings.json'),'utf8'),config=await readFile(join(f.options.directory,'runtime-config.json'),'utf8');
 f.dependencies.fetch=async()=>{throw Error('unreachable');};
 f.dependencies.run=async args=>{assert.ok(args.includes('SupervisorState'));return {alive:true};};
 f.dependencies.exec=()=>assert.fail('Do not proceed to discovery');
 f.dependencies.verifyRuntime=()=>assert.fail('No native verification');
 f.dependencies.atomic=()=>assert.fail('No changes to uncertain owner');
 for(const action of ['prepare-check','setup'])await assert.rejects(lifecycle({...f.options,action},f.dependencies),/SHARED_CONNECTION_UNCERTAIN/);
 assert.equal(await readFile(join(f.options.directory,'settings.json'),'utf8'),settings);
 assert.equal(await readFile(join(f.options.directory,'runtime-config.json'),'utf8'),config);
});

test('preparation rejects active native work before copying files or stopping its writer',async()=>{
 const f=await fixture();await lifecycle({...f.options,action:'setup'},f.dependencies);f.setTurns(2);
 const before=f.calls.length;
 f.dependencies.exec=()=>assert.fail('No candidate discovery while busy');
 f.dependencies.verifyRuntime=()=>assert.fail('No native verification');
 f.dependencies.atomic=()=>assert.fail('No writes while busy');
 for(const action of ['prepare-check','setup'])await assert.rejects(lifecycle({...f.options,action},f.dependencies),/SHARED_DESKTOP_BUSY/);
 assert.deepEqual(f.calls.slice(before),[]);
});

test('read-only preparation check permits idle work without changing any persisted settings',async()=>{
 const f=await fixture();await lifecycle({...f.options,action:'setup'},f.dependencies);
 const before=await readFile(join(f.options.directory,'settings.json'),'utf8'),calls=f.calls.length;
 f.dependencies.atomic=()=>assert.fail('Check must not write');
 f.dependencies.verifyRuntime=()=>assert.fail('Check must not verify/pin');
 assert.equal((await lifecycle({...f.options,action:'prepare-check'},f.dependencies)).ready,true);
 assert.equal(await readFile(join(f.options.directory,'settings.json'),'utf8'),before);
 assert.deepEqual(f.calls.slice(calls),[]);
});

test('recapture retains earlier orphan identities and deduplicates the same child',()=>{
 const root={pid:100,createdAt:'1000',ownerSid:'owner',imagePath:'node'},old={pid:101,createdAt:'1001',ownerSid:'owner',imagePath:'tool'},next={pid:102,createdAt:'1002',ownerSid:'owner',imagePath:'tool'};
 assert.deepEqual(mergeStopSnapshots({root,children:[old]},{root,children:[old,next]}).children,[old,next]);
});

test('a different supervisor identity never borrows an old shutdown capture',()=>{
 const root={pid:100,createdAt:'1000',ownerSid:'owner',imagePath:'node'},newRoot={...root,createdAt:'2000'},current={root:newRoot,children:[]};
 assert.equal(mergeStopSnapshots({root,children:[{pid:101,createdAt:'1001',ownerSid:'owner',imagePath:'tool'}]},current),current);
});
test('launch progress follows real readiness and the window gate',async()=>{
 const f=await fixture(),steps=[];await lifecycle({...f.options,action:'setup'},f.dependencies);
 f.dependencies.atomic=async(path,value)=>{if(path.endsWith('launch-progress.json'))steps.push(value.step);await atomic(path,value);};
 await lifecycle({...f.options,action:'launch'},f.dependencies);
 assert.deepEqual(steps,['CHECKING','LAUNCHING','CONNECTING','SHOWING','CONNECTED']);
 assert.equal(JSON.parse(await readFile(join(f.options.directory,'launch-progress.json'),'utf8')).state,'DONE');
});
test('reopening failure preserves existing shared backend and active turns',async()=>{
 const f=await fixture();await lifecycle({...f.options,action:'setup'},f.dependencies);await lifecycle({...f.options,action:'launch'},f.dependencies);f.setDesktop(false);f.setTurns(3);
 const before=f.calls.length,spawn=f.dependencies.spawn;f.dependencies.spawn=(...args)=>{const c=spawn(...args);c.exitCode=1;f.setDesktop(false);return c;};
 await assert.rejects(lifecycle({...f.options,action:'launch'},f.dependencies),/SHARED_DESKTOP_CONNECTION_FAILED/);
 assert.ok(!f.calls.slice(before).includes('Stop'));assert.ok(!f.calls.slice(before).includes('Restore'));
 assert.equal(JSON.parse(await readFile(join(f.options.directory,'settings.json'),'utf8')).phase,'ACTIVE');
 assert.equal((await lifecycle({...f.options,action:'status'},f.dependencies)).ready,true);
});
test('an already-connected shared Desktop is shown without spawning another process',async()=>{
 const f=await fixture();await lifecycle({...f.options,action:'setup'},f.dependencies);await lifecycle({...f.options,action:'launch'},f.dependencies);const before=f.calls.length;
 f.dependencies.exec=async()=>({stdout:JSON.stringify({desktop:'fixture-current-Desktop.exe',count:5})});const run=f.dependencies.run;f.dependencies.run=args=>args.includes('ShowDesktop')?Promise.resolve({visible:true}):run(args);
 assert.equal((await lifecycle({...f.options,action:'launch'},f.dependencies)).desktopConnected,true);
 assert.ok(!f.calls.slice(before).includes('spawn-Desktop'));
});
test('prepare changes no persistent endpoint and launches no Desktop; active commits only after Desktop initialization; disable restores',async()=>{
 const f=await fixture();let value=await lifecycle({...f.options,action:'setup'},f.dependencies);assert.equal(value.state,'SHARED_PREPARED');assert.equal(value.enabled,false);assert.equal(f.env.CODEX_APP_SERVER_WS_URL,null);assert.ok(!f.calls.includes('spawn-Desktop'));
 value=await lifecycle({...f.options,action:'launch'},f.dependencies);assert.equal(value.state,'SHARED_CONNECTED');assert.equal(value.enabled,true);assert.equal(f.env.CODEX_APP_SERVER_WS_URL,null);
 value=await lifecycle({...f.options,action:'disable'},f.dependencies);assert.equal(value.enabled,false);assert.equal(value.ready,false);assert.equal(f.env.CODEX_APP_SERVER_WS_URL,null);assert.equal(f.env.NODE_EXTRA_CA_CERTS,'original.pem');
});
test('failed RPC preparation rolls back runtime/settings and never modifies Desktop environment',async()=>{
 const f=await fixture();f.dependencies.probe=async()=>{throw Error('SHARED_GATEWAY_PREFLIGHT_FAILED');};
 await assert.rejects(lifecycle({...f.options,action:'setup'},f.dependencies),/SHARED_GATEWAY_PREFLIGHT_FAILED/);
 assert.equal(f.env.CODEX_APP_SERVER_WS_URL,null);assert.ok(f.calls.includes('Stop'));const s=JSON.parse(await readFile(join(f.options.directory,'settings.json'),'utf8'));assert.equal(s.enabled,false);assert.equal(s.phase,'DISABLED');
});
test('successful activation never writes environment, even after actual Desktop handshake',async()=>{
 const f=await fixture();await lifecycle({...f.options,action:'setup'},f.dependencies);await lifecycle({...f.options,action:'launch'},f.dependencies);assert.ok(!f.calls.includes('Apply'));assert.equal(f.env.CODEX_APP_SERVER_WS_URL,null);
});
test('failed final settings commit restores endpoint even when config has already been committed',async()=>{
 const f=await fixture();await lifecycle({...f.options,action:'setup'},f.dependencies);let failed=false;
 f.dependencies.atomic=async(path,value)=>{if(!failed&&path.endsWith('settings.json')&&value.phase==='ACTIVE'){failed=true;throw Error('SHARED_SETTINGS_UNAVAILABLE');}await atomic(path,value);};
 await assert.rejects(lifecycle({...f.options,action:'launch'},f.dependencies),/SHARED_SETTINGS_UNAVAILABLE/);assert.equal(f.env.CODEX_APP_SERVER_WS_URL,null);assert.equal(JSON.parse(await readFile(join(f.options.directory,'runtime-config.json'),'utf8')).enabled,false);
});
test('disable keeps later owner configuration and is idempotent',async()=>{
 const f=await fixture();await lifecycle({...f.options,action:'setup'},f.dependencies);await lifecycle({...f.options,action:'launch'},f.dependencies);f.env.CODEX_APP_SERVER_WS_URL='wss://owner.example/rpc';
 await lifecycle({...f.options,action:'disable'},f.dependencies);await lifecycle({...f.options,action:'disable'},f.dependencies);assert.equal(f.env.CODEX_APP_SERVER_WS_URL,'wss://owner.example/rpc');
});
test('active Desktop turns reject disable before any endpoint mutation or process stop',async()=>{
 const f=await fixture();await lifecycle({...f.options,action:'setup'},f.dependencies);await lifecycle({...f.options,action:'launch'},f.dependencies);f.setTurns(1);const before=f.calls.length;
 await assert.rejects(lifecycle({...f.options,action:'disable'},f.dependencies),/SHARED_DESKTOP_BUSY/);assert.equal(f.env.CODEX_APP_SERVER_WS_URL,null);assert.ok(!f.calls.slice(before).includes('Restore'));assert.ok(!f.calls.slice(before).includes('Stop'));
});

test('shutdown captures children before the graceful request removes their root',async()=>{
 const f=await fixture();await lifecycle({...f.options,action:'setup'},f.dependencies);
 const events=[],run=f.dependencies.run,fetch=f.dependencies.fetch;
 f.dependencies.run=async args=>{const action=args[args.indexOf('-Action')+1];events.push(action);if(action==='CaptureStop')return {root:{pid:123},rootAbsent:false,children:[{pid:456}]};if(action==='Stop'){const path=args[args.indexOf('-SnapshotPath')+1];assert.equal(JSON.parse(await readFile(path,'utf8')).children[0].pid,456);}return run(args);};
 f.dependencies.fetch=async(url,opts)=>{if(url.endsWith('/stop'))events.push('graceful-stop');return fetch(url,opts);};
 await lifecycle({...f.options,action:'disable'},f.dependencies);
 assert.ok(events.indexOf('CaptureStop')<events.indexOf('graceful-stop'));assert.ok(events.indexOf('graceful-stop')<events.indexOf('Stop'));
 await assert.rejects(readFile(join(f.options.directory,'stop-processes.json')),e=>e.code==='ENOENT');
});

test('failed child cleanup keeps its capture for recovery after the root is absent',async()=>{
 const f=await fixture();await lifecycle({...f.options,action:'setup'},f.dependencies);const run=f.dependencies.run;let captured=false,fail=true;
 f.dependencies.run=async args=>{if(args.includes('CaptureStop')){if(captured)return {rootAbsent:true};captured=true;return {root:{pid:123},rootAbsent:false,children:[{pid:456}]};}if(args.includes('Stop')){assert.equal(JSON.parse(await readFile(join(f.options.directory,'stop-processes.json'),'utf8')).children[0].pid,456);if(fail)throw Error('SHARED_STOP_FAILED');}return run(args);};
 await assert.rejects(lifecycle({...f.options,action:'disable'},f.dependencies),/SHARED_ROLLBACK_INCOMPLETE/);
 fail=false;await lifecycle({...f.options,action:'disable'},f.dependencies);await assert.rejects(readFile(join(f.options.directory,'stop-processes.json')),e=>e.code==='ENOENT');
});

test('snapshot persistence failure leaves the live root running and reports recovery',async()=>{
 const f=await fixture();await lifecycle({...f.options,action:'setup'},f.dependencies);const run=f.dependencies.run,fetch=f.dependencies.fetch;let stopped=false;
 f.dependencies.run=async args=>args.includes('CaptureStop')?{root:{pid:123},rootAbsent:false}:run(args);
 f.dependencies.atomic=async(path,value)=>{if(path.endsWith('stop-processes.json'))throw Error('disk-full');return atomic(path,value);};
 f.dependencies.fetch=async(url,opts)=>{if(url.endsWith('/stop'))stopped=true;return fetch(url,opts);};
 await assert.rejects(lifecycle({...f.options,action:'disable'},f.dependencies),/SHARED_ROLLBACK_INCOMPLETE/);assert.equal(stopped,false);assert.equal((await lifecycle({...f.options,action:'status'},f.dependencies)).ready,true);
});
test('already-running Desktop blocks launch without creating a second instance or abandoning prepared mode',async()=>{
 const f=await fixture();await lifecycle({...f.options,action:'setup'},f.dependencies);f.dependencies.exec=async()=>({stdout:JSON.stringify({desktop:'fixture-current-Desktop.exe',count:1})});
 await assert.rejects(lifecycle({...f.options,action:'launch'},f.dependencies),/SHARED_DESKTOP_CLOSE_REQUIRED/);assert.ok(!f.calls.includes('spawn-Desktop'));assert.equal(f.env.CODEX_APP_SERVER_WS_URL,null);
});
test('legacy enabled/settings and disabled/runtime mismatch restores only installed values',async()=>{
 const f=await fixture();await atomic(join(f.options.directory,'settings.json'),{enabled:true,previousDesktopUrl:null,previousSystemCa:null,previousCa:'original.pem'});await atomic(join(f.options.directory,'runtime-config.json'),{enabled:false});
 f.env.CODEX_APP_SERVER_WS_URL='wss://127.0.0.1:47116/rpc';f.env.NODE_USE_SYSTEM_CA='1';await lifecycle({...f.options,action:'disable'},f.dependencies);assert.equal(f.env.CODEX_APP_SERVER_WS_URL,null);assert.equal(f.env.NODE_USE_SYSTEM_CA,null);assert.equal(f.env.NODE_EXTRA_CA_CERTS,'original.pem');
});
test('legacy restore failure still attempts process cleanup and reports recovery required, not success',async()=>{
 const f=await fixture();await atomic(join(f.options.directory,'settings.json'),{enabled:true,previousDesktopUrl:null});await atomic(join(f.options.directory,'runtime-config.json'),{enabled:false});f.env.CODEX_APP_SERVER_WS_URL='wss://127.0.0.1:47116/rpc';
 const run=f.dependencies.run;f.dependencies.run=async args=>{if(args.includes('Restore'))throw Error('denied');return run(args);};await assert.rejects(lifecycle({...f.options,action:'disable'},f.dependencies),/SHARED_ROLLBACK_INCOMPLETE/);assert.ok(f.calls.includes('Stop'));assert.equal(JSON.parse(await readFile(join(f.options.directory,'settings.json'),'utf8')).phase,'RECOVERY_REQUIRED');
});

test('Desktop uses an independent visible launch with process-local URL only',()=>{
 const original={UNRELATED:'keep'};const options=desktopLaunchOptions('ws://127.0.0.1:47116/rpc',original);
 assert.equal(options.detached,true);assert.equal(options.windowsHide,false);assert.equal(options.stdio,'ignore');assert.equal(options.env.UNRELATED,'keep');assert.equal(original.CODEX_APP_SERVER_WS_URL,undefined);
});

test('a handshake with no visible Desktop window never commits ACTIVE',async()=>{
 const f=await fixture();await lifecycle({...f.options,action:'setup'},f.dependencies);let clock=0;f.dependencies.now=()=>clock;f.dependencies.sleep=async ms=>{clock+=ms;};const run=f.dependencies.run;
 f.dependencies.run=async args=>args.includes('DesktopWindow')?{visible:false}:run(args);
 await assert.rejects(lifecycle({...f.options,action:'launch'},f.dependencies),/SHARED_DESKTOP_CONNECTION_FAILED/);
 const saved=JSON.parse(await readFile(join(f.options.directory,'settings.json'),'utf8'));assert.equal(saved.enabled,false);assert.equal(saved.phase,'DISABLED');
 const result=JSON.parse(await readFile(join(f.options.directory,'desktop-launch.json'),'utf8'));assert.equal(result.state,'FAILED');assert.ok(f.calls.includes('Stop'));
});

test('diagnostic persistence failure cannot block rollback or fail successful activation',async()=>{
 const f=await fixture();await lifecycle({...f.options,action:'setup'},f.dependencies);f.dependencies.atomic=async(path,value)=>{if(path.endsWith('desktop-launch.json'))throw Error('diagnostic storage unavailable');await atomic(path,value);};
 const active=await lifecycle({...f.options,action:'launch'},f.dependencies);assert.equal(active.enabled,true);await lifecycle({...f.options,action:'disable'},f.dependencies);
 await lifecycle({...f.options,action:'setup'},f.dependencies);let clock=0;f.dependencies.now=()=>clock;f.dependencies.sleep=async ms=>{clock+=ms;};const run=f.dependencies.run;f.dependencies.run=async args=>args.includes('DesktopWindow')?{visible:false}:run(args);
 await assert.rejects(lifecycle({...f.options,action:'launch'},f.dependencies),/SHARED_DESKTOP_CONNECTION_FAILED/);assert.equal(JSON.parse(await readFile(join(f.options.directory,'settings.json'),'utf8')).phase,'DISABLED');
});


test('Store upgrade resolves the registered Desktop rather than a removed saved executable',async()=>{
 const f=await fixture();await lifecycle({...f.options,action:'setup'},f.dependencies);
 const spawn=f.dependencies.spawn;let launched;
 f.dependencies.spawn=(file,args,opts)=>{if(!args.length)launched=file;return spawn(file,args,opts);};
 await lifecycle({...f.options,action:'launch'},f.dependencies);
 assert.equal(launched,'fixture-current-Desktop.exe');
 assert.equal(JSON.parse(await readFile(join(f.options.directory,'desktop-launch.json'),'utf8')).iconApplied,true);
});
test('icon repair failure preserves the connected Desktop and reports the cosmetic failure',async()=>{
 const f=await fixture();await lifecycle({...f.options,action:'setup'},f.dependencies);const exec=f.dependencies.exec;
 f.dependencies.exec=async(file,args)=>{if(args.includes('-DesktopPid'))throw Error('icon unavailable');return exec(file,args);};
 assert.equal((await lifecycle({...f.options,action:'launch'},f.dependencies)).state,'SHARED_CONNECTED');
 assert.equal(JSON.parse(await readFile(join(f.options.directory,'desktop-launch.json'),'utf8')).iconApplied,false);
 assert.ok(!f.calls.includes('Stop'));
});

test('missing native companions are rejected before stopping an existing resident or changing settings',async()=>{
 const f=await fixture();await lifecycle({...f.options,action:'setup'},f.dependencies);await lifecycle({...f.options,action:'launch'},f.dependencies);
 const before=await readFile(join(f.options.directory,'settings.json'),'utf8'),count=f.calls.length;
 f.dependencies.verifyRuntime=async()=>{throw Error('SHARED_NATIVE_RUNTIME_INCOMPLETE');};
 await assert.rejects(lifecycle({...f.options,action:'setup'},f.dependencies),/SHARED_NATIVE_RUNTIME_INCOMPLETE/);
 assert.equal(await readFile(join(f.options.directory,'settings.json'),'utf8'),before);assert.equal(f.calls.length,count);
});
test('unknown live ownership cannot be force-stopped when health is unavailable',async()=>{
 const f=await fixture();await lifecycle({...f.options,action:'setup'},f.dependencies);
 const before=await readFile(join(f.options.directory,'settings.json'),'utf8'),run=f.dependencies.run;
 f.dependencies.fetch=async()=>{throw Error('unreachable');};
 f.dependencies.run=async args=>args.includes('SupervisorState')?{alive:true}:run(args);
 const count=f.calls.length;await assert.rejects(lifecycle({...f.options,action:'disable'},f.dependencies),/SHARED_CONNECTION_UNCERTAIN/);
 assert.equal(await readFile(join(f.options.directory,'settings.json'),'utf8'),before);assert.ok(!f.calls.slice(count).includes('Stop'));
});
test('a native upgrade leaves a healthy current writer intact until a safe explicit switch',async()=>{
 const f=await fixture();await lifecycle({...f.options,action:'setup'},f.dependencies);f.setTurns(2);
 const count=f.calls.length;await assert.rejects(lifecycle({...f.options,version:'0.162.0',action:'setup'},f.dependencies),/SHARED_RUNTIME_UPDATE_PENDING/);
 assert.equal((await lifecycle({...f.options,action:'status'},f.dependencies)).ready,true);assert.ok(!f.calls.slice(count).includes('Stop'));
});
test('launch verifies retained tools again before it can open Desktop',async()=>{
 const f=await fixture();await lifecycle({...f.options,action:'setup'},f.dependencies);const count=f.calls.length;
 f.dependencies.verifyRuntime=async()=>{throw Error('SHARED_NATIVE_COHORT_MISMATCH');};
 await assert.rejects(lifecycle({...f.options,action:'launch'},f.dependencies),/SHARED_NATIVE_COHORT_MISMATCH/);
 assert.ok(!f.calls.slice(count).includes('spawn-Desktop'));assert.equal((await lifecycle({...f.options,action:'status'},f.dependencies)).ready,true);
});

test('a backend lost during verification blocks Desktop startup and preserves diagnostics',async()=>{
 const f=await fixture();await lifecycle({...f.options,action:'setup'},f.dependencies);
 let lost=false;const fetch=f.dependencies.fetch;
 f.dependencies.fetch=async(...args)=>{if(lost)throw Error('producer absent');return fetch(...args);};
 f.dependencies.verifyRuntime=async()=>{lost=true;return{cohortVerified:true};};
 f.dependencies.spawn=()=>assert.fail('Never open a Desktop against a missing producer');
 await assert.rejects(lifecycle({...f.options,action:'launch'},f.dependencies),/SHARED_BACKEND_LOST_BEFORE_DESKTOP/);
 assert.equal(JSON.parse(await readFile(join(f.options.directory,'launch-progress.json'),'utf8')).errorCode,'SHARED_BACKEND_LOST_BEFORE_DESKTOP');
});
