import {test} from 'node:test';
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {mkdtemp,realpath,rm,mkdir,writeFile,readFile,symlink} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {Journal,MacSharedLifecycle,StdioSession,ownedAppServer,realPackageVerified} from './lifecycle.mjs';
const fixtureFile=fileURLToPath(new URL('./fixtures/app-server.mjs',import.meta.url));
async function fixture(t,scenario='normal'){
 const scratch=path.resolve('runtime/shared-native-compatibility');await mkdir(scratch,{recursive:true});
 const base=await realpath(await mkdtemp(path.join(scratch,'agbrio-mac-shared-'))),root=path.join(base,'cohort');
 const children=[],frames=[];let starts=0;
 const createSession=async()=>{starts++;const child=spawn(process.execPath,[fixtureFile,scenario],{windowsHide:true,stdio:['pipe','pipe','pipe']});children.push(child);let buffer='';child.stdout.on('data',part=>{buffer+=part;for(;;){const at=buffer.indexOf('\n');if(at<0)break;try{frames.push(JSON.parse(buffer.slice(0,at)));}catch{frames.push({fixtureCorruptFrame:true});}buffer=buffer.slice(at+1);}});return new StdioSession(child,{timeout:3000});};
 const journal=new Journal(root),controller=new MacSharedLifecycle({journal,createSession});
 t.after(async()=>{for(const child of children){if(child.exitCode===null&&child.signalCode===null){const ended=new Promise(resolve=>child.once('close',resolve));child.kill();await ended;}}await rm(base,{recursive:true,force:true});});
 return {controller,journal,root,base,frames,children,createSession,starts:()=>starts};
}
test('three steps use one initialized child, no turn or auth mutation, and an exact epoch',async t=>{
 const f=await fixture(t),p=await f.controller.prepare();assert.equal(p.phase,'PREPARED');assert.equal(p.realPackageVerified,false);assert.equal(realPackageVerified,false);
 assert.throws(()=>f.controller.connection(p.epoch),/CONNECTION_UNAVAILABLE/);
 assert.equal((await f.controller.prepare()).epoch,p.epoch);assert.equal(f.starts(),1);
 await assert.rejects(f.controller.activate('wrong'),/EPOCH_CHANGED/);
 assert.equal((await f.controller.activate(p.epoch)).phase,'ACTIVE');
 assert.equal((await f.controller.activate(p.epoch)).desktopConnected,false);
 await f.controller.connection(p.epoch).request('model/list');
 await assert.rejects(f.controller.connection(p.epoch).request('turn/start'),/READ_ONLY/);
 assert.equal((await f.controller.close(p.epoch)).phase,'STOPPED');assert.equal(f.children[0].exitCode,0);
 assert.equal((await f.controller.close(p.epoch)).phase,'STOPPED');
 const report=f.frames.find(v=>v.method==='fixture/report').params;assert.equal(report.eofs,1);assert.deepEqual(report.seen.slice(0,2),['initialize','initialized']);
 assert.ok(report.seen.every(v=>['initialize','initialized','thread/loaded/list','thread/read','model/list'].includes(v)));
 const successor=await f.controller.prepare();assert.notEqual(successor.epoch,p.epoch);
 await assert.rejects(f.controller.close(p.epoch),/EPOCH_CHANGED/);assert.equal(f.children[1].exitCode,null);await f.controller.close(successor.epoch);
});
for(const scenario of ['busy','question','race'])test(`${scenario}: activation and close never cross uncertain activity or answer a native question`,async t=>{
 const f=await fixture(t,scenario),p=await f.controller.prepare();
 await assert.rejects(f.controller.activate(p.epoch),/ACTIVITY_UNCONFIRMED/);await assert.rejects(f.controller.close(p.epoch),/ACTIVITY_UNCONFIRMED/);
 assert.equal((await f.journal.read()).phase,'PREPARED');assert.equal(f.controller.session.eofSent,false);assert.equal(f.children[0].exitCode,null);
});
test('unknown EOF stays fenced, cannot replay, and reconciles only its actual child exit',async t=>{
 const f=await fixture(t,'hang'),p=await f.controller.prepare();await f.controller.activate(p.epoch);f.controller.session.timeout=60;
 await assert.rejects(f.controller.close(p.epoch),/CLOSE_UNCONFIRMED/);assert.equal((await f.journal.read()).phase,'UNKNOWN');
 await assert.rejects(f.controller.close(p.epoch),/RECONCILE_REQUIRED/);await assert.rejects(f.controller.prepare(),/RECONCILE_REQUIRED/);assert.equal(f.starts(),1);
 const other=new MacSharedLifecycle({journal:f.journal,createSession:f.createSession});await assert.rejects(other.reconcile(p.epoch),/RECONCILE_REQUIRED/);
 await assert.rejects(f.controller.reconcile(p.epoch),/RECONCILE_REQUIRED/);
 const ended=new Promise(resolve=>f.children[0].once('close',resolve));f.children[0].kill();await ended;
 assert.equal((await f.controller.reconcile(p.epoch)).phase,'STOPPED');
});
test('restart never adopts a saved PID/session, or replaces an active backend',async t=>{
 const f=await fixture(t),p=await f.controller.prepare();await f.controller.activate(p.epoch);
 const other=new MacSharedLifecycle({journal:f.journal,createSession:f.createSession});await assert.rejects(other.prepare(),/RECONCILE_REQUIRED/);assert.equal(f.starts(),1);assert.equal(f.children[0].exitCode,null);
 assert.equal((await other.inspect()).ready,false);assert.equal((await other.inspect()).active,false);
 await f.controller.close(p.epoch);
});
test('read-only inspection creates no storage, while a concurrent action cannot spawn twice',async t=>{
 const f=await fixture(t);assert.equal((await f.controller.inspect()).phase,'STOPPED');
 await assert.rejects(readFile(path.join(f.root,'state.json')),error=>error.code==='ENOENT');
 const preparation=f.controller.prepare();await assert.rejects(f.controller.prepare(),/BUSY/);const p=await preparation;assert.equal(f.starts(),1);await f.controller.close(p.epoch);
});
test('lost activation checkpoint leaves a durable fence rather than a usable connection',async t=>{
 const f=await fixture(t),p=await f.controller.prepare();const original=f.journal.write.bind(f.journal);
 f.journal.write=async value=>{await original(value);if(value.phase==='ACTIVATING')throw Error('fixture-write-unconfirmed');};
 await assert.rejects(f.controller.activate(p.epoch),/write-unconfirmed/);assert.equal((await f.journal.read()).phase,'ACTIVATING');assert.throws(()=>f.controller.connection(p.epoch),/CONNECTION_UNAVAILABLE/);await assert.rejects(f.controller.activate(p.epoch),/RECONCILE_REQUIRED/);
});
for(const scenario of ['badInitialize','badProtocol'])test(`${scenario}: malformed initialization is durable UNKNOWN and never automatically spawns again`,async t=>{
 const f=await fixture(t,scenario);await assert.rejects(f.controller.prepare(),/INITIALIZE_INVALID|TRANSPORT_UNCONFIRMED/);assert.equal((await f.journal.read()).phase,'UNKNOWN');await assert.rejects(f.controller.prepare(),/RECONCILE_REQUIRED/);assert.equal(f.starts(),1);
});
test('foreign files, linked journals and held operation intent are preserved',async t=>{
 const f=await fixture(t);await mkdir(f.root,{mode:0o700});await writeFile(path.join(f.root,'keep.txt'),'foreign');await assert.rejects(f.controller.prepare(),/FOREIGN_PRESERVED/);assert.equal(await readFile(path.join(f.root,'keep.txt'),'utf8'),'foreign');assert.equal(f.starts(),0);
 const other=new Journal(path.join(f.base,'second'));await mkdir(other.root,{mode:0o700});await writeFile(path.join(other.root,'operation.lock'),'unconfirmed',{mode:0o600});await assert.rejects(other.locked(()=>{}),/BUSY_OR_UNCONFIRMED/);
 if(process.platform!=='win32'){await rm(path.join(f.root,'keep.txt'));const original=path.join(f.base,'original');await writeFile(original,'keep',{mode:0o600});await symlink(original,path.join(f.root,'state.json'));await assert.rejects(f.controller.prepare(),/STORAGE_UNTRUSTED/);assert.equal(await readFile(original,'utf8'),'keep');}
});
test('real adapter rejects an unverifiable executable before process or account work',async()=>{
 await assert.rejects(ownedAppServer('not-an-absolute-binary')(),/BINARY_UNVERIFIED/);
});
