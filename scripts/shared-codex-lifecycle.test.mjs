import {test} from 'node:test';
import assert from 'node:assert/strict';
import {mkdir,readFile} from 'node:fs/promises';
import {resolve,join} from 'node:path';
import {EventEmitter} from 'node:events';
import {randomUUID} from 'node:crypto';
import {lifecycle,atomic,restoreChanges,sharedUrl,desktopLaunchOptions} from '../src-tauri/resources/codex-shared/control.mjs';

async function fixture(){
 const directory=resolve('runtime/shared-gateway-repair/unit',randomUUID());await mkdir(directory,{recursive:true});
 let running=false,desktopConnected=false,turns=0;const env={CODEX_APP_SERVER_WS_URL:null,NODE_EXTRA_CA_CERTS:'original.pem',NODE_USE_SYSTEM_CA:null};const calls=[];
 const options={directory,binary:'fixture-codex.exe',version:'0.160.0',desktop:'fixture-Desktop.exe'};
 const child=()=>{const c=new EventEmitter();c.pid=12345;c.exitCode=null;c.unref=()=>{};c.kill=()=>{c.exitCode=0;c.emit('exit',0);desktopConnected=false;};return c;};
 const dependencies={
  run:async args=>{const action=args[args.indexOf('-Action')+1];calls.push(action);
   if(action==='Snapshot')return {...env};if(action==='DesktopWindow')return {visible:true};
   if(action==='Apply'){env.CODEX_APP_SERVER_WS_URL=args[args.indexOf('-Url')+1];}
   if(action==='Restore'){const j=JSON.parse(await readFile(args[args.indexOf('-SnapshotPath')+1],'utf8'));for(const c of j.changes)if(env[c.name]===c.installed)env[c.name]=c.previous;}
   if(action==='Stop')running=false;return{ok:true};
  },
  exec:async(_file,args)=>({stdout:args[0]==='--version'?'codex-cli 0.160.0':args.at(-1).includes('.Count')?'0':'S-1-5-21-fixture'}),
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
test('already-running Desktop blocks launch without creating a second instance or abandoning prepared mode',async()=>{
 const f=await fixture();await lifecycle({...f.options,action:'setup'},f.dependencies);f.dependencies.exec=async()=>({stdout:'1'});
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
