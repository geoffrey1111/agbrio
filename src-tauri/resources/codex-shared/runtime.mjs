// One detached, version-pinned native backend for this Windows owner.
// The public local gateway admits native connections owned by the same SID;
// browser Origin headers are rejected. The inner backend additionally uses a
// random bearer capability that never appears in the gateway URL or logs.
import{createRequire}from'node:module';import{createServer}from'node:http';import{createServer as tcpServer,connect as tcpConnect}from'node:net';import{spawn,execFile}from'node:child_process';import{readFile}from'node:fs/promises';import{randomBytes,createHash}from'node:crypto';import{dirname,join}from'node:path';
import {assertNativeCohort} from './native-cohort.mjs';
import {backendRecovery,verifiedBackendStart} from './backend-recovery.mjs';
import {verifyProcessPrivilege,preferredPowerShell} from './privilege.mjs';
import {replaceStateFile} from './state-file.mjs';
const require=createRequire(import.meta.url),{WebSocket,WebSocketServer}=require('./ws/index.js'),configPath=process.argv[2],cfg=JSON.parse(await readFile(configPath,'utf8'));
if(!cfg.ownerSid?.startsWith('S-1-')||!Number.isInteger(cfg.port)||cfg.port<1024||cfg.port>65535)throw Error('SHARED_CONFIGURATION_INVALID');
const folder=dirname(configPath),nonce=cfg.instance,readyPath=join(folder,'ready.json'),powershell=preferredPowerShell();
const activeTurns=new Set(),gatewaySockets=new Set();
const gateway=createServer(async(req,res)=>{
 if(req.headers.origin||req.socket.remoteAddress!=='127.0.0.1'){res.writeHead(403);return res.end();}
 if(req.url==='/stop'&&req.method==='POST'){
  if(!cfg.controlToken||req.headers.authorization!=='Bearer '+cfg.controlToken||!(await sameOwner(req.socket.remotePort)).owner){res.writeHead(403);return res.end();}
  if(activeTurns.size){res.writeHead(409);return res.end();}
  record('OWNER_STOP_REQUEST');
  res.end('{"stopping":true}');setImmediate(()=>void stop());return;
 }
 if(req.url!=='/health'||req.method!=='GET'){res.writeHead(403);return res.end();}
 res.setHeader('content-type','application/json');res.setHeader('cache-control','no-store');
 res.end(JSON.stringify({instance:nonce,ready:backendReady&&peerAvailable&&readyDurable,verifierReady:peerAvailable,supervisorPid:process.pid,backendPid:backend?.pid??null,elevated:supervisorIdentity?.elevated??null,backendElevated:backendElevation,version:cfg.version,activeTurns:activeTurns.size,desktopConnected:[...clients].some(ws=>ws.desktopActor&&['codex_desktop','Codex Desktop'].includes(ws.clientName)&&ws.initialized===true)}));
});
gateway.on('connection',s=>{gatewaySockets.add(s);s.on('close',()=>gatewaySockets.delete(s));s.on('error',()=>record('SOCKET_ERROR'));});
let supervisorIdentity=null,backendElevation=null;let backend=null,backendReady=false,readyDurable=false,backoff=1000,closing=false,innerPort,token;const clients=new Set();const wss=new WebSocketServer({noServer:true,maxPayload:16*1024*1024});
const events=[];let eventWrite=Promise.resolve();
function record(event,detail={}){events.push({at:Date.now(),event,pid:process.pid,...detail});if(events.length>64)events.shift();const value={format:1,events:[...events]};eventWrite=eventWrite.catch(()=>{}).then(()=>replaceStateFile(join(folder,'runtime-events.json'),value));eventWrite.catch(()=>{});}
let readyWrite=Promise.resolve();function saveReady(){const current=backend,value={instance:nonce,port:cfg.port,supervisorPid:process.pid,backendPid:backend?.pid??null,supervisorIdentity,ready:backendReady&&peerAvailable,verifierReady:peerAvailable};readyWrite=readyWrite.catch(()=>{}).then(async()=>{await replaceStateFile(readyPath,value);if(current===backend)readyDurable=value.ready&&backendReady&&peerAvailable;}).catch(error=>{readyDurable=false;record('STATE_WRITE_FAILED',{code:/^[A-Z_]{1,40}$/.test(error.code??'')?error.code:'IO_ERROR'});console.error('SHARED_STATE_WRITE_FAILED');throw error;});readyWrite.catch(()=>{});return readyWrite;}
record('RUNTIME_START',{parentPid:process.ppid});
// Keep one small native ownership service; WMI on every handshake was too slow.
let peerService=null,peerAvailable=false,peerNext=0,peerBuffer='',peerRetry=null;const peerPending=new Map();
let peerReadyResolve;const peerReady=new Promise(resolve=>peerReadyResolve=resolve);
function failPeers(){peerAvailable=false;readyDurable=false;record('OWNER_SERVICE_UNAVAILABLE');void saveReady();for(const pending of peerPending.values()){clearTimeout(pending.timer);pending.resolve({owner:false,desktop:false});}peerPending.clear();}
function startPeerService(){if(closing)return;peerBuffer='';peerAvailable=false;const service=spawn(powershell,['-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',join(folder,'runtime/peer-service.ps1'),'-OwnerSid',cfg.ownerSid],{windowsHide:true,stdio:['pipe','pipe','ignore']});peerService=service;let failed=false;
 const recover=()=>{if(failed||peerService!==service)return;failed=true;failPeers();service.kill();if(!closing)peerRetry=setTimeout(startPeerService,1000);};
 service.stdout.on('data',chunk=>{if(peerService!==service)return;peerBuffer+=chunk;let at;while((at=peerBuffer.indexOf('\n'))>=0){const line=peerBuffer.slice(0,at);peerBuffer=peerBuffer.slice(at+1);let value;try{value=JSON.parse(line)}catch{continue;}if(value.ready){const identity=value.supervisorIdentity;if(identity?.pid!==process.pid||identity.ownerSid!==cfg.ownerSid||identity.imagePath.toLowerCase()!==process.execPath.toLowerCase()){record('OWNER_IDENTITY_REJECTED');service.kill();continue;}supervisorIdentity=identity;peerAvailable=true;record('OWNER_VERIFIED');saveReady().then(()=>peerReadyResolve(true),()=>peerReadyResolve(false));}else{const pending=peerPending.get(value.id);if(pending){clearTimeout(pending.timer);peerPending.delete(value.id);pending.resolve(value);}}}});
 service.stdin.on('error',recover);service.once('error',recover);service.once('exit',recover);service.once('close',recover);
}
function sameOwner(peerPort,serverPort=cfg.port,expectedPid=0){return new Promise(resolve=>{if(!peerAvailable||!peerService?.stdin.writable)return resolve({owner:false,desktop:false});const id=++peerNext,timer=setTimeout(()=>{peerPending.delete(id);resolve({owner:false,desktop:false});},2000);peerPending.set(id,{resolve,timer});peerService.stdin.write(JSON.stringify({id,peerPort,serverPort,expectedPid})+'\n',error=>{if(error){const pending=peerPending.get(id);if(pending){clearTimeout(pending.timer);peerPending.delete(id);pending.resolve({owner:false,desktop:false});}}});});}
gateway.on('upgrade',async(req,socket,head)=>{if(cfg.testOnly)console.error('TEST_GATEWAY_UPGRADE '+JSON.stringify({origin:!!req.headers.origin,path:req.url,ready:backendReady,loopback:socket.remoteAddress==='127.0.0.1'}));socket.pause();if(req.headers.origin||req.url!=='/rpc'||socket.remoteAddress!=='127.0.0.1'||!backendReady||!peerAvailable||!readyDurable){socket.end('HTTP/1.1 403 Forbidden\r\nConnection: close\r\n\r\n');return;}const actor=await sameOwner(socket.remotePort);if(cfg.testOnly)console.error('TEST_ACTOR_'+actor.owner+'_DESKTOP_'+actor.desktop);if(!actor.owner){socket.end('HTTP/1.1 403 Forbidden\r\nConnection: close\r\n\r\n');return;}
 // Native history responses contain tool media. Match the ws client's bounded
 // 100 MiB response envelope; owner requests above remain limited to 16 MiB.
 const upstream=new WebSocket(`ws://127.0.0.1:${innerPort}`,{headers:{Authorization:'Bearer '+token},maxPayload:100*1024*1024,handshakeTimeout:7000,createConnection:(options,done)=>{const raw=tcpConnect({host:'127.0.0.1',port:innerPort});raw.once('error',error=>done(error));raw.once('connect',async()=>{const peer=await sameOwner(raw.localPort,innerPort,backend.pid);if(cfg.testOnly)console.error('TEST_INNER_'+peer.owner);if(peer.owner)done(null,raw);else{raw.destroy();done(Error('INNER_OWNER_REJECTED'));}});}});let client;
 upstream.once('open',()=>{if(cfg.testOnly)console.error('TEST_UPSTREAM_OPEN');if(socket.destroyed){upstream.close();return;}wss.handleUpgrade(req,socket,head,ws=>{client=ws;ws.desktopActor=actor.desktop;clients.add(ws);socket.resume();ws.on('message',(data,binary)=>{try{const v=JSON.parse(data.toString());if(v.method==='initialize'){ws.clientName=v.params?.clientInfo?.name;if(cfg.testOnly)console.error('TEST_INITIALIZE_NAME_'+ws.clientName);ws.initializeId=JSON.stringify(v.id);}}catch{}if(binary||upstream.readyState!==WebSocket.OPEN)return ws.close();upstream.send(data,{binary:false});});ws.on('close',()=>{clients.delete(ws);upstream.close();});ws.on('error',()=>upstream.close());});});
 upstream.on('message',(data,binary)=>{if(client&&!binary){try{const v=JSON.parse(data.toString());if(v.method==='turn/started'&&v.params?.turn?.id)activeTurns.add(v.params.turn.id);if(v.method==='turn/completed'&&v.params?.turn?.id)activeTurns.delete(v.params.turn.id);if(!v.method&&v.id!==undefined&&JSON.stringify(v.id)===client.initializeId&&Object.hasOwn(v,'result'))client.initialized=true;}catch{}}if(client?.readyState===WebSocket.OPEN)client.send(data,{binary});});upstream.on('close',()=>{if(client)client.close();else socket.destroy();});upstream.on('error',error=>{if(cfg.testOnly)console.error('TEST_UPSTREAM_ERROR');const oversized=error.code==='WS_ERR_UNSUPPORTED_MESSAGE_LENGTH';console.error(oversized?'SHARED_BACKEND_RESPONSE_TOO_LARGE':'SHARED_BACKEND_CONNECTION_FAILED');if(client)client.close(oversized?1009:1011,oversized?'SHARED_BACKEND_RESPONSE_TOO_LARGE':'SHARED_BACKEND_CONNECTION_FAILED');else socket.destroy();});
});
process.on('SIGTERM',()=>{record('SIGNAL_STOP',{signal:'SIGTERM'});void stop();});process.on('SIGINT',()=>{record('SIGNAL_STOP',{signal:'SIGINT'});void stop();});process.on('uncaughtException',error=>{record('UNCAUGHT',{code:/^[A-Z_]{1,40}$/.test(error.code??'')?error.code:'UNCLASSIFIED'});console.error('SHARED_RUNTIME_FAILED');void stop();});
const recovery=backendRecovery(startBackend,async()=>{console.error('SHARED_BACKEND_RECOVERY_FAILED');await stop();});
startPeerService();
try{
 if(!await Promise.race([peerReady,new Promise(resolve=>setTimeout(()=>resolve(false),6000))]))throw Error("SHARED_OWNER_SERVICE_FAILED");
 await new Promise((resolve,reject)=>{gateway.once('error',reject);gateway.listen(cfg.port,'127.0.0.1',resolve);});
  record('GATEWAY_LISTENING');
}catch{console.error('SHARED_START_FAILED');await stop();}
async function freePort(){return new Promise((r,j)=>{const s=tcpServer();s.once('error',j);s.listen(0,'127.0.0.1',()=>{const port=s.address().port;s.close(()=>r(port));});});}
async function startBackend(){if(closing)return;backendReady=false;backendElevation=null;
 const current=await verifiedBackendStart(()=>closing,async()=>{
  const port=await freePort(),capability=randomBytes(32).toString('hex');return {port,capability,digest:createHash('sha256').update(capability).digest('hex')};
 },async()=>{if(!cfg.testOnly)await assertNativeCohort(cfg.binary,cfg.nativeFiles);},candidate=>{
  innerPort=candidate.port;token=candidate.capability;
  backend=spawn(cfg.binary,['-c','features.code_mode_host=true','app-server','--listen',`ws://127.0.0.1:${innerPort}`,'--ws-auth','capability-token','--ws-token-sha256',candidate.digest],{env:{...process.env,...cfg.codexHome?{CODEX_HOME:cfg.codexHome}:{}},windowsHide:true,stdio:['ignore','ignore','pipe']});backend.stderr.resume();
 const owned=backend;record('BACKEND_START',{backendPid:owned.pid??null});let recovered=false;const recover=(code,signal)=>{if(recovered||backend!==owned)return;recovered=true;backendReady=false;readyDurable=false;record('BACKEND_EXIT',{backendPid:owned.pid??null,code:Number.isInteger(code)?code:null,signal:typeof signal==='string'?signal:null});activeTurns.clear();for(const client of clients)client.close();void saveReady();if(!closing){recovery.schedule(backoff);backoff=Math.min(backoff*2,30000);}};owned.once('error',recover);owned.once('exit',recover);owned.once('close',recover);return owned;
 });
 if(!current)return;
 const actual=await verifyProcessPrivilege(current.pid,cfg.ownerSid,cfg.binary,supervisorIdentity.elevated);backendElevation=actual.elevated;
 if(typeof cfg.launchElevated==='boolean'&&cfg.launchElevated!==backendElevation)throw Error('SHARED_PRIVILEGE_TOKEN_MISMATCH');
 for(let n=0;n<40&&!closing&&backend===current&&current.exitCode===null;n++){try{const r=await fetch(`http://127.0.0.1:${innerPort}/readyz`,{headers:{Authorization:'Bearer '+token},signal:AbortSignal.timeout(500)});if(r.ok&&!closing&&backend===current&&current.exitCode===null){backendReady=true;backoff=1000;break;}}catch{}await new Promise(r=>setTimeout(r,100));}await saveReady();if(readyDurable)record('BACKEND_READY',{backendPid:current.pid});
}
async function stop(){
 if(closing)return;closing=true;backendReady=false;readyDurable=false;record('RUNTIME_STOP');clearTimeout(peerRetry);recovery.cancel();
 for(const ws of clients)ws.terminate();for(const raw of gatewaySockets)raw.destroy();gateway.close();failPeers();
 // Wait for proven children to exit before ending the supervisor. Windows SIGTERM
 // alone does not run JS cleanup when an unrelated controller terminates Node.
 const wait=child=>new Promise(resolve=>{if(!child||child.exitCode!==null)return resolve();child.once('exit',resolve);child.kill();setTimeout(resolve,3000).unref();});
 await Promise.all([wait(peerService),wait(backend)]);try{await saveReady();await eventWrite;}finally{process.exit(0);}
}

try{await startBackend();}catch{console.error('SHARED_START_FAILED');await stop();}

if(cfg.testOnly&&process.send)process.on("message",message=>{if(message==="STOP_TEST_RUNTIME")stop();else if(message==="STOP_TEST_PEER")peerService?.kill();});
