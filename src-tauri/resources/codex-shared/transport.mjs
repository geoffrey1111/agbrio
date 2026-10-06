import{createRequire}from'node:module';import{createInterface}from'node:readline';import{dirname,resolve}from'node:path';
const require=createRequire(import.meta.url),{WebSocket}=require('./ws/index.js'),url=new URL(process.argv[2]);
if(url.protocol!=='ws:'||url.hostname!=='127.0.0.1'||url.pathname!=='/rpc'||url.username||url.password||url.search)throw Error('SHARED_ENDPOINT_INVALID');
const ws=new WebSocket(url,{maxPayload:16*1024*1024,handshakeTimeout:6000});const lines=createInterface({input:process.stdin,crlfDelay:Infinity});lines.pause();
let ended=false;const finish=()=>{if(ended)return;ended=true;lines.close();ws.close();setTimeout(()=>process.exit(1),50);};
ws.on('open',()=>lines.resume());lines.on('line',line=>{if(Buffer.byteLength(line)>16*1024*1024)return finish();if(ws.readyState!==WebSocket.OPEN)return finish();ws.send(line);});
ws.on('message',(data,binary)=>{if(binary)return finish();if(!process.stdout.write(data.toString()+'\n'))ws.pause();});process.stdout.on('drain',()=>ws.resume());
lines.on('close',()=>{ws.close();setTimeout(()=>process.exit(0),100);});ws.on('close',finish);ws.on('error',()=>{console.error('SHARED_CONNECTION_UNAVAILABLE');finish();});
