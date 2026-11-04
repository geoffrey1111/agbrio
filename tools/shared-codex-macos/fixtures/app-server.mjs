// Original protocol fixture, not an OpenAI binary, account or inference client.
import readline from 'node:readline';
const scenario=process.argv[2]??'normal',seen=[];let initialized=false;
const send=value=>process.stdout.write(JSON.stringify(value)+'\n');
const input=readline.createInterface({input:process.stdin});
input.on('line',line=>{
 const v=JSON.parse(line);seen.push(v.method);
 if(v.method==='initialize'){if(scenario==='badProtocol'){process.stdout.write('not-json\n');return;}send({id:v.id,result:scenario==='badInitialize'?{}:{userAgent:'fixture/1',platformFamily:'unix',platformOs:'macos'}});return;}
 if(v.method==='initialized'){initialized=true;return;}
 if(!initialized){send({id:v.id,error:{code:-32600,message:'Not initialized'}});return;}
 if(v.method==='thread/loaded/list'){
  send({id:v.id,result:{data:scenario==='normal'||scenario==='hang'?[]:['fixture-thread'],nextCursor:null}});return;
 }
 if(v.method==='thread/read'){
  send({id:v.id,result:{thread:{id:'fixture-thread',status:{type:scenario==='busy'?'active':'idle',activeFlags:scenario==='busy'?['waitingOnApproval']:[]}}}});
  if(scenario==='question')send({id:'native-permission',method:'item/commandExecution/requestApproval',params:{threadId:'fixture-thread'}});
  if(scenario==='race')send({method:'thread/status/changed',params:{threadId:'fixture-thread',status:{type:'active',activeFlags:[]}}});
  return;
 }
 send({id:v.id,result:{data:[]}});
});
input.on('close',()=>{
 send({method:'fixture/report',params:{seen,eofs:1}});
 if(scenario==='hang')setInterval(()=>{},1000);
 else process.stdout.write('',()=>process.exit(0));
});
