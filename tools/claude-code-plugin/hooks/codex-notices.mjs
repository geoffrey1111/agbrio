// Host-owned durable task subscriptions. Only fixed metadata, never a model helper.
const KEY='agbrio-codex-task-receipts-v1';
const uuid=v=>typeof v==='string'&&/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(v);
function owner(state){if(state.stopped||!uuid(state.identity?.sessionId)||typeof state.identity?.cwd!=='string')throw Error('AGBRIO_NOTICE_SESSION_UNAVAILABLE');return{sessionId:state.identity.sessionId,instanceId:state.instanceId,cwd:state.identity.cwd};}
function key(state){return `${KEY}:${owner(state).sessionId}`;}
function serial(state,work){const p=(state.taskNoticeQueue??Promise.resolve()).catch(()=>{}).then(work);state.taskNoticeQueue=p;return p;}
async function post($,state,input){const r=await $.http.fetch(`${state.origin}/v1/claude-code/codex-notices`,{method:'POST',headers:{'content-type':'application/json',authorization:`Bearer ${state.token}`},body:JSON.stringify({...owner(state),...input})});if(!r.ok)throw Error('AGBRIO_NOTICE_HOST_UNAVAILABLE');return JSON.parse(r.text||'{}');}
async function load($,state){const v=await $.store.get(key(state));return Array.isArray(v)?v.filter(v=>v.sessionId===state.identity.sessionId&&v.cwd===state.identity.cwd):[];}
async function ack($,state,r){const v=await post($,state,{action:'ACK',noticeId:r.id,messageId:r.messageId});if(v.accepted!==true)throw Error('AGBRIO_NOTICE_RECEIPT_UNCONFIRMED');}
export async function registerTaskWait($,state,input){return post($,state,{action:'REGISTER',threadId:input.threadId,generation:input.generation,turnId:input.turnId,followGoal:true,ttlSeconds:input.ttlSeconds??3600});}
export async function cancelTaskWait($,state,input){return post($,state,{action:'CANCEL',subscriptionId:input.subscriptionId??null,threadId:input.threadId??null,turnId:input.turnId??null});}
export async function taskWaitStatus($,state){return post($,state,{action:'STATUS'});}
export async function tickTaskNotices($,state){return serial(state,async()=>{
 const now=await $.clock.now();if(now<(state.taskNoticeNextAt??0))return;state.taskNoticeNextAt=now+5000;
 const old=await load($,state),keep=[];for(const r of old){if(r.status==='DELIVERED'){try{await ack($,state,r);}catch{keep.push(r);}}else keep.push(r);}await $.store.set(key(state),keep);
 const response=await post($,state,{action:keep.length>=128?'STATUS':'SYNC'});
 if(Array.isArray(response.settled)){const ids=new Set(response.settled.map(r=>r.id));for(let i=keep.length-1;i>=0;i--)if(ids.has(keep[i].id))keep.splice(i,1);await $.store.set(key(state),keep);}if(keep.length>=128)throw Error('AGBRIO_NOTICE_RECEIPT_LIMIT');if(!Array.isArray(response.subscriptions))throw Error('AGBRIO_NOTICE_RESPONSE_INVALID');
 state.taskSubscriptions=response.subscriptions;state.taskNoticeError=null;
 const n=response.notice;if(!n)return;
 if(typeof n.id!=='string'||!/^[a-f0-9]{64}$/.test(n.id)||typeof n.text!=='string'||n.text.length>8192||!n.text.startsWith('Agbrio Codex task notice (metadata only)\n')||!n.text.includes(`noticeId: ${n.id}\n`))throw Error('AGBRIO_NOTICE_INVALID');
 if(keep.some(r=>r.id===n.id))return;
 const record={...owner(state),id:n.id,text:n.text,status:'UNKNOWN'};await $.store.set(key(state),[...keep,record]);
 // The Host already reserved UNKNOWN. Reload, crash and an ambiguous submit
 // never create another attempt. An appended native row/Host reconciliation settles it.
 void $.prompt.submit({text:n.text}).catch(()=>{});
});}
export async function appendTaskNotice($,state,e,result){
 if(e.agentId||!['prompt','delivery'].includes(e.door)||e.origin?.kind!=='plugin'||e.origin.name!=='agbrio-bridge'||result?.message?.type!=='user'||!uuid(result.uuid))return result;
 const c=result.message.content;let text=typeof c==='string'?c:Array.isArray(c)&&c.every(b=>b.type==='text')?c.map(b=>b.text).join(''):null;
 const prefix='The agbrio-bridge plugin sent a message:\n',suffix='\n\nThis is how Claude Code surfaces a prompt a plugin submits';if(text?.startsWith(prefix)&&text.lastIndexOf(suffix)>=prefix.length)text=text.slice(prefix.length,text.lastIndexOf(suffix));
 await serial(state,async()=>{const list=await load($,state),old=list.find(r=>r.status==='UNKNOWN'&&r.text===text);if(!old)return;const done={...old,status:'DELIVERED',messageId:result.uuid};let next=list.map(r=>r.id===old.id?done:r);await $.store.set(key(state),next);try{await ack($,state,done);next=next.filter(r=>r.id!==old.id);await $.store.set(key(state),next);}catch{/* acknowledgement only is retried */}});return result;
}
