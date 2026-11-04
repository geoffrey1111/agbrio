// Fixed task metadata only. No model helper, user impersonation or task control.
const KEY='agbrio-codex-task-receipts-v1',MAX=128,BUFFER=8;
const uuid=v=>typeof v==='string'&&/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(v);
function owner(state){if(state.stopped||!uuid(state.identity?.sessionId)||typeof state.identity?.cwd!=='string')throw Error('AGBRIO_NOTICE_SESSION_UNAVAILABLE');return{sessionId:state.identity.sessionId,instanceId:state.instanceId,cwd:state.identity.cwd};}
function key(state){return KEY+':'+owner(state).sessionId;}
function serial(state,work){const p=(state.taskNoticeQueue??Promise.resolve()).catch(()=>{}).then(work);state.taskNoticeQueue=p;return p;}
export function hostFailure(r){
 let body;try{body=JSON.parse(r.text||'{}');}catch{}
 const candidate=typeof body?.error==='string'?body.error:body?.error?.code;
 const code=/^(?:CLAUDE|WATCH|AGBRIO|CODEX|MANAGEMENT)_[A-Z0-9_]{1,112}$/.test(candidate??'')?candidate:r.status===404?'AGBRIO_WAIT_HOST_UPDATE_REQUIRED':r.status===422?'AGBRIO_NOTICE_REQUEST_INVALID':'AGBRIO_NOTICE_HOST_REJECTED';
 const e=Error(code);e.httpStatus=Number.isInteger(r.status)?r.status:null;return e;
}
async function post($,state,input){let r;try{r=await $.http.fetch(state.origin+'/v1/claude-code/codex-notices',{method:'POST',headers:{'content-type':'application/json',authorization:'Bearer '+state.token},body:JSON.stringify({...owner(state),...input})});}catch{throw Error('AGBRIO_NOTICE_NETWORK_UNAVAILABLE');}if(!r.ok)throw hostFailure(r);try{return JSON.parse(r.text||'{}');}catch{throw Error('AGBRIO_NOTICE_RESPONSE_INVALID');}}
// A working-directory change cannot delete a same-session native attempt.
// The Host, not this presentation filter, validates the canonical native root.
async function load($,state){const v=await $.store.get(key(state));return Array.isArray(v)?v.filter(v=>v.sessionId===state.identity.sessionId):[];}
async function edit($,state,change){return serial(state,async()=>{const list=await load($,state);const result=change(list);await $.store.set(key(state),list);return result;});}
function claimBody(r){return{action:'CLAIM',noticeId:r.id,claimId:r.claimId,...(r.proofNonce?{deliveryChannel:'INLINE_OR_PROMPT',proofNonce:r.proofNonce}:{})};}
function ackBody(r){return{action:'ACK',noticeId:r.id,messageId:r.messageId,...(r.deliveryMode==='INLINE_TOOL_RESULT'?{deliveryChannel:r.deliveryMode,proofNonce:r.proofNonce,claimId:r.claimId}:{})};}
async function ack($,state,r){const v=await post($,state,ackBody(r));if(v.accepted!==true)throw Error('AGBRIO_NOTICE_RECEIPT_UNCONFIRMED');}
function queueAck($,state,r){
 const active=state.taskNoticeAcking??=new Set();if(active.has(r.id)||active.size>=2)return;active.add(r.id);
 void ack($,state,r).then(()=>edit($,state,list=>{const i=list.findIndex(v=>v.id===r.id&&v.status==='DELIVERED'&&v.messageId===r.messageId);if(i>=0)list.splice(i,1);})).catch(()=>{state.taskNoticeAckError='AGBRIO_NOTICE_ACK_PENDING';}).finally(()=>active.delete(r.id));
}
function validNotice(n){return n&&typeof n.id==='string'&&/^[a-f0-9]{64}$/.test(n.id)&&typeof n.text==='string'&&n.text.length<=8192&&/^Agbrio (?:Codex|Pi|Claude Code) task notice \(metadata only\)\n/.test(n.text)&&n.text.includes('noticeId: '+n.id+'\n');}
async function claim($,state,r){
 const active=state.taskNoticeClaiming??=new Map();if(active.has(r.id))return active.get(r.id);
 const pending=post($,state,claimBody(r)).finally(()=>active.delete(r.id));active.set(r.id,pending);return pending;
}
async function claimSoon($,state,r){
 // Never hold an original tool result for the HTTP API's 30-second deadline.
 return Promise.race([claim($,state,r),$.clock.sleep(200).then(()=>{throw Error('AGBRIO_NOTICE_INLINE_CLAIM_DEFERRED');})]);
}
function nativeDrop(reason){
 if(typeof reason!=='string')return null;
 if(reason.includes('the session was stopped and nobody has sent input since')||reason.includes('was removed from the queue before it ran'))return 'USER_INPUT_REQUIRED';
 return 'NATIVE_DROP_UNCONFIRMED';
}
function submitPrompt($,state,r){
 state.taskNoticeDelivery='WAITING_NATIVE_IDLE';
 void $.prompt.submit({text:r.text}).then(async result=>{
  const drop=nativeDrop(result?.drop);if(!drop)return;
  state.taskNoticeDelivery=drop;state.taskNoticeSubmitError=drop;
  // Only these documented no-queue/drop outcomes prove no native append.
  if(drop==='USER_INPUT_REQUIRED')await edit($,state,list=>{const row=list.find(v=>v.id===r.id&&v.status==='UNKNOWN'&&v.deliveryMode==='PROMPT');if(row){row.status='DEFERRED';row.dropReason=drop;}});
 }).catch(()=>{state.taskNoticeSubmitError='AGBRIO_NOTICE_NATIVE_SUBMIT_UNCONFIRMED';});
}
async function submitPrepared($,state,r){
 if(r.proofNonce&&state.resourceBusy)return;
 const response=await claim($,state,r);if(response.accepted!==true)throw Error('AGBRIO_NOTICE_CLAIM_UNCONFIRMED');
 const selected=await edit($,state,list=>{const row=list.find(v=>v.id===r.id&&v.status==='PREPARED');if(!row)return null;if(row.proofNonce&&state.resourceBusy)return null;row.status='UNKNOWN';row.deliveryMode='PROMPT';return{...row};});
 if(selected)submitPrompt($,state,selected);
}
export async function registerTaskWait($,state,input){return post($,state,{action:'REGISTER',threadId:input.threadId,generation:input.generation,turnId:input.turnId,followGoal:input.followGoal===true,followTask:input.followTask===true,ttlSeconds:input.ttlSeconds??86400});}
export async function cancelTaskWait($,state,input){return post($,state,{action:'CANCEL',subscriptionId:input.subscriptionId??null,threadId:input.threadId??null,turnId:input.turnId??null});}
export async function taskWaitStatus($,state){return post($,state,{action:'STATUS'});}
export async function tickTaskNotices($,state){
 if(state.taskNoticeTickRunning)return;state.taskNoticeTickRunning=true;
 try{
  const now=await $.clock.now();if(now<(state.taskNoticeNextAt??0))return;state.taskNoticeNextAt=now+1000;
  const old=await serial(state,()=>load($,state));
  for(const r of old.filter(r=>r.status==='DELIVERED'))queueAck($,state,r);
  const buffered=old.filter(r=>['PREPARED','DEFERRED'].includes(r.status));
  const prepared=buffered.find(r=>r.status==='PREPARED');
  if(prepared&&(!prepared.proofNonce||!state.resourceBusy))try{await submitPrepared($,state,prepared);}catch{state.taskNoticeError='AGBRIO_NOTICE_PRE_SUBMIT_RETRY';}
  const input={action:old.length>=MAX?'STATUS':'OFFER',...(state.taskInlineSupported?{excludeNoticeIds:buffered.slice(0,BUFFER).map(r=>r.id)}:{})};
  const response=await post($,state,input);if(!Array.isArray(response.subscriptions))throw Error('AGBRIO_NOTICE_RESPONSE_INVALID');
  state.taskSubscriptions=response.subscriptions;state.taskInlineSupported=response.inlineNoticeProtocol===1;state.taskNoticeError=null;
  if(Array.isArray(response.settled)){const ids=new Set(response.settled.map(r=>r.id));await edit($,state,list=>{for(let i=list.length-1;i>=0;i--)if(ids.has(list[i].id))list.splice(i,1);});}
  for(const id of response.invalidatedNoticeIds??[]){
   const r=old.find(r=>r.id===id&&['PREPARED','DEFERRED'].includes(r.status));if(!r)continue;
   try{const released=await post($,state,{...claimBody(r),action:'RELEASE_PREPARED'});if(released.accepted===true)await edit($,state,list=>{const i=list.findIndex(v=>v.id===id&&['PREPARED','DEFERRED'].includes(v.status));if(i>=0)list.splice(i,1);});}catch{/* Attempted/ambiguous notices stay fenced. */}
  }
  const n=response.notice;if(!n)return;if(!validNotice(n))throw Error('AGBRIO_NOTICE_INVALID');
  const record=await edit($,state,list=>{
   if(list.some(r=>r.id===n.id))return null;if(list.length>=MAX)throw Error('AGBRIO_NOTICE_RECEIPT_LIMIT');
   if(state.taskInlineSupported&&list.filter(r=>['PREPARED','DEFERRED'].includes(r.status)).length>=BUFFER)return null;
   const r={...owner(state),id:n.id,text:n.text,claimId:state.instanceId,status:'PREPARED',...(state.taskInlineSupported?{proofNonce:crypto.randomUUID()}:{})};list.push(r);return{...r};
  });
  if(record&&(!record.proofNonce||!state.resourceBusy))await submitPrepared($,state,record);
 }finally{state.taskNoticeTickRunning=false;}
}
function toolBlock(e){
 if(e.agentId||e.isSidechain||['plugin','person','user'].includes(e.origin?.kind)||!(e.message?.type==='user'||e.message?.role==='user')||!Array.isArray(e.message?.content))return null;
 for(let i=0;i<e.message.content.length;i++){const b=e.message.content[i];if(b?.type!=='tool_result'||typeof b.tool_use_id!=='string'||!b.tool_use_id||b.tool_use_id.length>256)continue;
  if(typeof b.content==='string'&&b.content.length<=1000000)return{i,block:b};
  if(Array.isArray(b.content)&&b.content.every(v=>v.type==='text'&&typeof v.text==='string')&&b.content.reduce((n,v)=>n+v.text.length,0)<=1000000)return{i,block:b};
 }return null;
}
export function inlineFrame(r,toolUseId){return '\n\n[Agbrio task notice: '+JSON.stringify({noticeId:r.id,proofNonce:r.proofNonce,toolUseId})+']\n'+r.text+'\n[/Agbrio task notice]';}
export async function prepareInlineTaskNotices($,state,e){
  const target=state.identity&&!state.stopped&&state.taskInlineSupported&&toolBlock(e);if(!target)return null;
  const candidates=await serial(state,()=>load($,state));const chosen=[];let bytes=0;
  for(const r of candidates.filter(r=>r.status==='PREPARED'&&uuid(r.proofNonce)).slice(0,1)){
   const frame=inlineFrame(r,target.block.tool_use_id),size=new TextEncoder().encode(frame).length;if(bytes+size>8192)continue;
   try{const claim=await claimSoon($,state,r);if(claim.accepted!==true)continue;}catch{state.taskNoticeDelivery='INLINE_CLAIM_DEFERRED';break;}
   const reserved=await edit($,state,list=>{const row=list.find(v=>v.id===r.id&&v.status==='PREPARED');if(!row)return false;row.status='UNKNOWN';row.deliveryMode='INLINE_TOOL_RESULT';row.toolUseId=target.block.tool_use_id;return true;});
   if(reserved){chosen.push({...r,deliveryMode:'INLINE_TOOL_RESULT',toolUseId:target.block.tool_use_id,frame});bytes+=size;}
  }
  if(!chosen.length)return null;
  const suffix=chosen.map(r=>r.frame).join(''),original=target.block.content;
  const block={...target.block,content:typeof original==='string'?original+suffix:[...original,{type:'text',text:suffix}]};
  const event={...e,message:{...e.message,content:e.message.content.map((b,i)=>i===target.i?block:b)}};
  return{event,chosen,suffix,toolUseId:target.block.tool_use_id};
}
export async function finishInlineTaskNotices($,state,prepared,result){
  if(!prepared)return result;const {chosen,suffix,toolUseId}=prepared;if(!uuid(result?.uuid))return result;
  const stored=result.message?.content?.find?.(b=>b.type==='tool_result'&&b.tool_use_id===toolUseId);
  const content=typeof stored?.content==='string'?stored.content:Array.isArray(stored?.content)?stored.content.filter(b=>b.type==='text').map(b=>b.text).join(''):null;
  if(typeof content!=='string'||!content.endsWith(suffix))return result;
  const done=await edit($,state,list=>{const records=[];for(const r of chosen){const row=list.find(v=>v.id===r.id&&v.status==='UNKNOWN'&&v.proofNonce===r.proofNonce&&v.deliveryMode==='INLINE_TOOL_RESULT');if(row){row.status='DELIVERED';row.messageId=result.uuid;records.push({...row});}}return records;});
  state.taskNoticeDelivery='NATIVE_TOOL_RESULT_APPENDED';for(const r of done)queueAck($,state,r);return result;
}
export async function resumeDeferredTaskNotices($,state){if(state.identity&&!state.stopped)await edit($,state,list=>{for(const r of list)if(r.status==='DEFERRED')r.status='PREPARED';});}

export async function appendTaskNotice($,state,e,result){
 if(e.agentId||!['prompt','delivery'].includes(e.door)||e.origin?.kind!=='plugin'||e.origin.name!=='agbrio-bridge'||!(result?.message?.type==='user'||result?.type==='user'&&result?.message?.role==='user')||!uuid(result.uuid))return result;
 const c=result.message.content;let text=typeof c==='string'?c:Array.isArray(c)&&c.every(b=>b.type==='text')?c.map(b=>b.text).join(''):null;
 const prefix='The agbrio-bridge plugin sent a message:\n',suffix='\n\nThis is how Claude Code surfaces a prompt a plugin submits';if(text?.startsWith(prefix)&&text.lastIndexOf(suffix)>=prefix.length)text=text.slice(prefix.length,text.lastIndexOf(suffix));
 const done=await edit($,state,list=>{const row=list.find(r=>r.status==='UNKNOWN'&&r.text===text&&r.deliveryMode!=='INLINE_TOOL_RESULT');if(!row)return null;row.status='DELIVERED';row.deliveryMode='PROMPT';row.messageId=result.uuid;return{...row};});
 if(done){state.taskNoticeDelivery='NATIVE_PROMPT_APPENDED';queueAck($,state,done);}return result;
}
export async function reconcileLegacyWaits($,state,waits){return post($,state,{action:'RECONCILE_LEGACY',legacyWaits:waits.map(w=>({threadId:w.threadId,generation:w.generation,turnId:w.turnId,sourceState:w.sourceState}))});}
