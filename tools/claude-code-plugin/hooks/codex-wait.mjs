// Native Mods APIs only. A notice is metadata, never a provider reply or an owner instruction.
import {registerTaskWait,cancelTaskWait,taskWaitStatus} from './codex-notices.mjs';
const TOOL='mcp__agbrio-bridge__codex_wait';
const UUID=/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const TERMINAL=['RESULT_READY','ACTION_REQUIRED','FAILED','INTERRUPTED'];
const KEY='agbrio-codex-waits-v1';
const MAX_WAITING=128,RECENT_SETTLED=32;
const archiveKey=(owner,w)=>`${KEY}:receipt:${owner.sessionId}:${w.threadId}:${w.generation}:${w.turnId}`;
function identity(state){
  if(!state.identity||state.stopped||!UUID.test(state.identity.sessionId??'')||typeof state.identity.cwd!=='string')throw Error('AGBRIO_WAIT_SESSION_UNAVAILABLE');
  return {sessionId:state.identity.sessionId,cwd:state.identity.cwd};
}
function serial(state,work){
  const task=(state.codexWaitQueue??Promise.resolve()).catch(()=>{}).then(work);
  state.codexWaitQueue=task;return task;
}
async function load($,state){
  const owner=identity(state),saved=await $.store.get(`${KEY}:${owner.sessionId}`);
  if(!saved)return [];
  if(saved.sessionId!==owner.sessionId||saved.cwd!==owner.cwd)return [];
  if(!Array.isArray(saved.waits)||saved.waits.length>10000)throw Error('AGBRIO_WAIT_STORAGE_INVALID');
  return saved.waits;
}
async function save($,state,waits){
 const owner=identity(state),settled=waits.filter(w=>['DELIVERED','EXPIRED','CANCELLED'].includes(w.status));
 const retire=new Set(settled.slice(0,Math.max(0,settled.length-RECENT_SETTLED)));
 // Durable tombstones precede compaction: old exact turns never become new waits.
 for(const w of retire)await $.store.set(archiveKey(owner,w),{...owner,wait:w});
 await $.store.set(`${KEY}:${owner.sessionId}`,{...owner,waits:waits.filter(w=>!retire.has(w))});
}
function target(input){
  if(!UUID.test(input.threadId??'')||!Number.isSafeInteger(input.generation)||input.generation<1||!UUID.test(input.turnId??''))throw Error('AGBRIO_WAIT_TARGET_INVALID');
  return {threadId:input.threadId,generation:input.generation,turnId:input.turnId};
}
function same(a,b){return a.threadId===b.threadId&&a.generation===b.generation&&a.turnId===b.turnId;}
export async function probeCodexWait($,state,wait){
  const response=await $.http.fetch(`${state.origin}/v1/claude-code/codex-wait`,{method:'POST',headers:{'content-type':'application/json',authorization:`Bearer ${state.token}`,'x-agbrio-wait-version':'2'},body:JSON.stringify({...identity(state),instanceId:state.instanceId,...target(wait)})});
  if(!response.ok)throw Error(response.status===404?'AGBRIO_WAIT_HOST_UPDATE_REQUIRED':'AGBRIO_WAIT_SOURCE_UNAVAILABLE');
  const value=JSON.parse(response.text||'{}');
  if(!same(value,wait)||!['IDLE','RUNNING','RESULT_PENDING','INCOMPLETE','UNKNOWN',...TERMINAL].includes(value.state))throw Error('AGBRIO_WAIT_RESPONSE_INVALID');
  return value;
}
export function waitNotice(wait,status){
  return `Agbrio Codex notice (metadata only)\nthreadId: ${wait.threadId}\nturnId: ${wait.turnId}\ngeneration: ${wait.generation}\nstate: ${status}\nRead this exact conversation through the authorized Agbrio MCP before reporting its result. This notice contains no reply body, approval, or permission to send another message. Follow only the owner's previously delegated task; ask the owner for any missing decision.`;
}
export async function handleCodexWait($,state,input){return serial(state,async()=>{
  if(input.action==='wait'&&input.followGoal===true)return registerTaskWait($,state,input);
  if(input.action==='cancel'&&input.subscriptionId)return cancelTaskWait($,state,input);
  const waits=await load($,state),now=await $.clock.now();
  for(const w of waits)if(w.status==='WAITING'&&w.expiresAt<=now)w.status='EXPIRED';
  if(input.action==='status'){await save($,state,waits);return{persistentSubscriptions:state.taskSubscriptions??[],pluginVersion:'0.1.18',waits,capacity:{waiting:waits.filter(w=>w.status==='WAITING').length,maxWaiting:MAX_WAITING,unknown:waits.filter(w=>w.status==='UNKNOWN').length}};}
  const t=target(input),saved=await $.store.get(archiveKey(identity(state),t)),old=waits.find(w=>same(w,t))??(saved?.sessionId===state.identity.sessionId&&saved.cwd===state.identity.cwd&&same(saved.wait??{},t)?saved.wait:null);
  if(input.action==='cancel'){
    if(old?.status==='WAITING')old.status='CANCELLED';
    await save($,state,waits);return{wait:old??null};
  }
  if(input.action!=='wait')throw Error('AGBRIO_WAIT_ACTION_INVALID');
  if(old){await save($,state,waits);return{wait:old};}
  const ttl=input.ttlSeconds??3600;
  if(!Number.isInteger(ttl)||ttl<30||ttl>86400)throw Error('AGBRIO_WAIT_TTL_INVALID');
  if(waits.filter(w=>w.status==='WAITING').length>=MAX_WAITING)throw Error('AGBRIO_WAIT_ACTIVE_LIMIT');
  // Existing registered source and exact turn must be confirmed before persisting intent.
  await probeCodexWait($,state,t);
  const wait={...t,status:'WAITING',expiresAt:now+ttl*1000,lastError:null};
  waits.push(wait);await save($,state,waits);return{wait};
});}
export async function tickCodexWaits($,state){return serial(state,async()=>{
  if(state.codexWaitReady===false)return;
  const now=await $.clock.now();
  // Piggyback the observation heartbeat; never query or restart a provider.
  if(now<(state.codexWaitNextAt??0))return;
  state.codexWaitNextAt=now+5000;
  const waits=await load($,state);
  for(const w of waits)if(w.status==='WAITING'&&state.taskSubscriptions?.some(s=>s.mode==='GOAL'&&['ACTIVE','DONE'].includes(s.status)&&s.threadId===w.threadId&&s.generation===w.generation))w.status='CANCELLED';
  if(!waits.length)return;
  for(const wait of waits){
    if(wait.status!=='WAITING')continue;
    if(wait.expiresAt<=now){wait.status='EXPIRED';await save($,state,waits);continue;}
    let value;
    try{value=await probeCodexWait($,state,wait);wait.lastError=null;}
    catch{wait.lastError='SOURCE_UNAVAILABLE';continue;}
    if(!TERMINAL.includes(value.state))continue;
    // Persist BEFORE the native call. Crash, reload, lost response and cancellation
    // after this point never cause an automatic second submission.
    wait.status='UNKNOWN';wait.sourceState=value.state;
    await save($,state,waits);
    void submitWaitNotice($,state,{...wait});
  }
  await save($,state,waits);
});}
async function submitWaitNotice($,state,wait){
  try{await $.prompt.submit({text:waitNotice(wait,wait.sourceState)});}catch{}
  // Acceptance is deliberately not delivery. Only session.append below proves it.
}
export async function initCodexWait($,state){
    state.codexWaitReady=false;
    try{await $.tool.register({name:'codex_wait',description:'Only for an owner-delegated Codex task: use followGoal:true to follow the same native Goal across intermediate turns. Receive new-question metadata and final/blocked Goal metadata separately, without progress spam. Host-persisted subscriptions survive reloads. Set followGoal:false or omit it for legacy exact-turn one-shot waiting. Cancel a durable subscription by subscriptionId, or inspect status. First read the exact target with authorized Agbrio MCP. Does not read bodies, send to Codex, or approve further work. Requires the Agbrio Host with codex-wait support; open session and live plugin required. Settled history is compacted without losing exact-turn deduplication. Up to128 concurrent waits; status reports capacity. UNKNOWN is never retried.',inputSchema:{type:'object',properties:{action:{type:'string',enum:['wait','status','cancel']},followGoal:{type:'boolean'},subscriptionId:{type:'string'},threadId:{type:'string'},generation:{type:'integer'},turnId:{type:'string'},ttlSeconds:{type:'integer',minimum:30,maximum:86400}},required:['action'],additionalProperties:false}});state.codexWaitReady=true;}catch{}
}
export function registerCodexWait(on,state){
  on('tool.call',{tool:TOOL},async($,e)=>{
    try{return{result:JSON.stringify(await handleCodexWait($,state,e))};}
    catch(error){const code=['AGBRIO_WAIT_HOST_UPDATE_REQUIRED','AGBRIO_WAIT_SOURCE_UNAVAILABLE','AGBRIO_WAIT_SESSION_UNAVAILABLE','AGBRIO_WAIT_TARGET_INVALID','AGBRIO_WAIT_TTL_INVALID','AGBRIO_WAIT_LIMIT','AGBRIO_WAIT_ACTIVE_LIMIT'].includes(error?.message)?error.message:'AGBRIO_WAIT_NOT_REGISTERED';return{result:JSON.stringify({error:code,instruction:'Check the exact target, live plugin/Host compatibility and authorized MCP. No notification is promised; do not replace this with a polling task or resend.'})};}
  });
}
export async function appendCodexWait($,state,e,result){
    if(e.agentId||!['prompt','delivery'].includes(e.door)||e.origin?.kind!=='plugin'||e.origin.name!=='agbrio-bridge'||result?.message?.type!=='user'||!UUID.test(result.uuid??''))return result;
    const c=result.message.content;
    const text=typeof c==='string'?c:Array.isArray(c)&&c.every(b=>b.type==='text')?c.map(b=>b.text).join(''):null;
    try{await serial(state,async()=>{
      const waits=await load($,state);
      for(const w of waits){
        if(w.status!=='UNKNOWN')continue;
        const notice=waitNotice(w,w.sourceState);
        const prefix='The agbrio-bridge plugin sent a message:\n',suffix='\n\nThis is how Claude Code surfaces a prompt a plugin submits';
        const unwrapped=typeof text==='string'&&text.startsWith(prefix)&&text.lastIndexOf(suffix)>=prefix.length?text.slice(prefix.length,text.lastIndexOf(suffix)):text;
        if(unwrapped===notice){w.status='DELIVERED';w.messageId=result.uuid;}
      }
      await save($,state,waits);
    });}catch{}
    return result;
}
