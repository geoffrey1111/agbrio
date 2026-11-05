// Disposable protocol peer: no model, credentials or provider network.
import fs from 'node:fs';import path from 'node:path';import readline from 'node:readline';
const root=process.argv[2];let sent=false;
readline.createInterface({input:process.stdin}).on('line',line=>{const q=JSON.parse(line);if(q.id===undefined)return;let result={};
const turn=sent?'fixture-reply':'fixture-seed';const text=sent?'Fixture reply received':'Fixture ready';
if(q.method==='thread/read'||q.method==='thread/resume')result={thread:{id:'fixture-chat',cwd:root,status:{type:'idle'},model:'fixture',approvalPolicy:'never',sandbox:{type:'readOnly'}}};
else if(q.method==='thread/turns/list')result={data:[{id:turn,status:'completed',itemsView:'notLoaded'}],nextCursor:null};
else if(q.method==='thread/items/list')result={data:[{turnId:turn,item:{id:'fixture-answer',type:'agentMessage',phase:'final_answer',text}}],nextCursor:null};
else if(q.method==='model/list')result={data:[{id:'fixture',model:'fixture',displayName:'Synthetic fixture',defaultReasoningEffort:'low',supportedReasoningEfforts:[{reasoningEffort:'low',description:'fixture'}]}],nextCursor:null};
else if(q.method==='account/read')result={account:{type:'apiKey'},requiresOpenaiAuth:false};
else if(q.method==='thread/goal/get')result={goal:null};
else if(q.method==='turn/start'){fs.appendFileSync(path.join(root,'native-sends.jsonl'),JSON.stringify(q.params)+'\n');sent=true;result={turn:{id:'fixture-reply'}};}
else if(q.method!=='initialize'){fs.appendFileSync(path.join(root,'native-protocol-errors.jsonl'),JSON.stringify({method:q.method})+'\n');throw Error('unexpected fixture method '+q.method);}
process.stdout.write(JSON.stringify({id:q.id,result})+'\n');if(q.method==='turn/start')setTimeout(()=>process.stdout.write(JSON.stringify({method:'turn/completed',params:{threadId:'fixture-chat',turn:{id:'fixture-reply',status:'completed'}}})+'\n'),20);});
