// One desktop-owned outbound relay. Never opens an agent or replays a request.
import fs from 'node:fs';
import http from 'node:http';
const config=JSON.parse(fs.readFileSync(process.argv[2],'utf8'));
const origin=new URL(config.origin);
if(origin.protocol!=='https:'||origin.pathname!=='/'||origin.search||origin.hash||origin.username||origin.password||!/^ag-[a-f0-9]{32}\./.test(origin.hostname)||!Number.isInteger(config.port)||config.port<1||config.port>65535||!/^[a-f0-9]{64}$/.test(config.hostToken))throw Error('INVALID_RELAY_CONFIGURATION');
const MAX=16*1024*1024;
let stopping=false,active=null,timer=null,inactive=false;
function status(state){try{fs.writeFileSync(config.statusPath,JSON.stringify({state,tenantId:config.tenantId,at:Date.now()}));}catch{}}
function stop(){stopping=true;clearTimeout(timer);active?.close();process.exit(0);}
process.on('SIGTERM',stop);process.on('SIGINT',stop);
process.stdin.resume();process.stdin.on('end',stop); // Parent pipe EOF ends this owned child.
const headersAllowed=new Set(['content-type','accept','cookie','authorization','origin','if-none-match','if-modified-since','range']);
async function forward(f){
 if(!/^[a-f0-9]{32}$/.test(f.id)||!['GET','HEAD','POST','DELETE'].includes(f.method)||typeof f.path!=='string'||!f.path.startsWith('/')||f.path.startsWith('//')||f.path.includes('\\')||f.path.length>8192||!Array.isArray(f.headers)||typeof f.body!=='string'||f.body.length>23*1024*1024)throw Error('INVALID_REQUEST');
 const u=new URL(f.path,origin);if(u.origin!==origin.origin)throw Error('INVALID_PATH');
 const body=Buffer.from(f.body,'base64');if(body.length>MAX)throw Error('TOO_LARGE');
 const headers={host:origin.host};for(const [k,v] of f.headers){if(headersAllowed.has(k.toLowerCase())&&typeof v==='string')headers[k.toLowerCase()]=v;}
 return new Promise(resolve=>{
  const req=http.request({hostname:'127.0.0.1',port:config.port,path:u.pathname+u.search,method:f.method,headers,timeout:55000},res=>{
   const chunks=[];let size=0;res.on('data',c=>{size+=c.length;if(size>MAX){res.destroy();return;}chunks.push(c);});
   res.on('end',()=>{const pairs=[];for(let i=0;i<res.rawHeaders.length;i+=2)pairs.push([res.rawHeaders[i].toLowerCase(),res.rawHeaders[i+1]]);resolve({type:'response',id:f.id,status:res.statusCode,headers:pairs,body:Buffer.concat(chunks).toString('base64')});});
   res.on('error',()=>resolve({type:'response',id:f.id,status:502,headers:[['content-type','application/json'],['cache-control','no-store']],body:Buffer.from('{"error":"LOCAL_RESPONSE_UNCERTAIN"}').toString('base64')}));
  });req.on('timeout',()=>req.destroy());req.on('error',()=>resolve({type:'response',id:f.id,status:502,headers:[['content-type','application/json'],['cache-control','no-store']],body:Buffer.from('{"error":"LOCAL_REQUEST_UNCERTAIN"}').toString('base64')}));req.end(body);
 });
}
function connect(){if(stopping)return;status('CONNECTING');const u=new URL('/__agbrio/connector',origin);u.protocol='wss:';const ws=new WebSocket(u,['agbrio','cap-'+config.hostToken]);active=ws;let inflight=0,authorized=false;const seen=new Set();
 ws.addEventListener('message',async event=>{try{if(typeof event.data!=='string'||event.data.length>23*1024*1024){ws.close();return;}const f=JSON.parse(event.data);if(f.type==='inactive'){inactive=true;status(f.state==='REVOKED'?'REVOKED':'EXPIRED');ws.close();return;}if(f.type==='ready'){authorized=true;status('READY');return;}if(!authorized||f.type!=='request')return;if(seen.has(f.id)||inflight>=12){ws.close();return;}seen.add(f.id);if(seen.size>10000){ws.close();return;}inflight++;const r=await forward(f);inflight--;if(ws.readyState===WebSocket.OPEN)ws.send(JSON.stringify(r));}catch{ws.close();}});
 ws.addEventListener('error',()=>{});ws.addEventListener('close',()=>{if(!inactive)status('OFFLINE');if(!stopping&&!inactive)timer=setTimeout(connect,5000);});
}
connect();
