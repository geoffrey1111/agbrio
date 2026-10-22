// Owner questions and one-call approvals. Never submits a prompt or changes rules.
const nativePoll=(state,id)=>({id,sessionId:state.identity.sessionId,instanceId:state.instanceId,cwd:state.identity.cwd});
async function post($,state,suffix,body){
 const r=await $.http.fetch(`${state.origin}/v1/claude-code/owner-requests${suffix}`,{method:'POST',headers:{'content-type':'application/json',authorization:`Bearer ${state.token}`},body:JSON.stringify(body)});
 if(!r.ok)throw Error('AGBRIO_OWNER_REQUEST_UNAVAILABLE');return JSON.parse(r.text||'{}');
}
export async function flushOwnerReceipts($,state){
 for(const id of state.ownerReceipts??[]){try{const r=await post($,state,'/receipt',nativePoll(state,id));if(r.confirmed){state.ownerReceipts.delete(id);for(const [tool,held] of state.ownerPending){if(held===id)state.ownerPending.delete(tool);}await $.store.set('agbrio-owner-receipts-v1',[...state.ownerReceipts]);}}catch{/* receipts retry; decisions never do */}}
}
async function hold($,state,event,kind,payload){
 if(!state.identity||state.stopped||typeof event.tool_use_id!=='string'||!event.tool_use_id)return null;
 const id=crypto.randomUUID(),body={...nativePoll(state,id),toolUseId:event.tool_use_id,kind,payload};let claimed=false,captured=false;
 try{
  if(!(await post($,state,'',body)).captured)return null;captured=true;
  state.ownerPending.set(event.tool_use_id,id);
  // HTTP time is outside a Mods hook's own execution budget. The server holds
  // PENDING polls; no clock.sleep, busy polling, model request or new session.
  for(let attempt=0;attempt<60;attempt++){
   const response=await post($,state,'/poll',nativePoll(state,id));
   if(response.status==='PENDING')continue;
   if(response.status!=='ANSWER')return null;
   // Persist uncertainty before returning a decision to the native engine.
   const answer=response.answer;
   if(kind==='QUESTION'&&(answer?.action!=='ANSWER'||!answer.answers))throw Error('AGBRIO_OWNER_ANSWER_INVALID');
   if(kind==='PERMISSION'&&!['ALLOW_ONCE','DENY'].includes(answer?.action))throw Error('AGBRIO_OWNER_ANSWER_INVALID');
   state.ownerReceipts.add(id);await $.store.set('agbrio-owner-receipts-v1',[...state.ownerReceipts]);claimed=true;return answer;
  }
 }catch{/* fail back to the original native flow; never invent an answer */}
 finally{if(captured&&!claimed){state.ownerPending.delete(event.tool_use_id);try{await post($,state,'/native',nativePoll(state,id));}catch{}}}
 return null;
}
export function registerOwnerRequests(on,state){
 state.ownerPending=new Map();state.ownerReceipts=new Set();
 on('classic.PreToolUse',{tool:'AskUserQuestion'},async($,e,next)=>{
  const original=(await next(e))??{};
  if(original.deny||e.agentId||e.agent_id||e.answers&&Object.keys(e.answers).length)return original;
  const input=original.updatedInput??e;
  if(!Array.isArray(input.questions))return original;
  const answer=await hold($,state,e,'QUESTION',{questions:input.questions});if(!answer)return original;
  const {tool,tool_use_id,...args}=input;
  return {...original,ask:undefined,deny:undefined,allow:true,updatedInput:{...args,answers:answer.answers}};
 }).catch(async($,e,next)=>{if(next.called)throw Error('AGBRIO_OWNER_DECISION_UNCONFIRMED');return next(e);});
 on('tool.check',async($,e,next)=>{
  const original=await next(e);
  // Managed/organization/MCP and subagent boundaries remain native. Only a
  // genuine pending built-in call can be approved once by the owner.
  if(original.decision!=='ask'||e.ceiling==='ask'||e.agentId||e.tool==='AskUserQuestion'||e.tool?.startsWith('mcp__')||!e.tool_use_id)return original;
  const {tool,tool_use_id,agentId,ceiling,isReadOnly,...input}=e;
  const answer=await hold($,state,e,'PERMISSION',{tool,input});if(!answer)return original;
  return {decision:answer.action==='ALLOW_ONCE'?'allow':'deny',reason:answer.action==='ALLOW_ONCE'?'Owner allowed this call in Agbrio.':'Owner declined this call in Agbrio.'};
 }).catch(async($,e,next)=>{if(next.called)throw Error('AGBRIO_OWNER_DECISION_UNCONFIRMED');return next(e);});
 on('classic.PostToolUse',async($,e,next)=>{const result=await next(e);await flushOwnerReceipts($,state);return result;});
 on('classic.PostToolUseFailure',async($,e,next)=>{const result=await next(e);await flushOwnerReceipts($,state);return result;});


}

export async function releaseOwnerRequests($,state){
 for(const id of state.ownerPending.values()){try{await post($,state,'/native',nativePoll(state,id));}catch{}}
 state.ownerPending.clear();
}
