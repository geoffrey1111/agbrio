import type {BridgeRole,RoleState} from "./RoleBridgePanel";
type Reply=RoleState["replies"][number];
const valid=(time?:number|null):time is number=>typeof time==="number"&&Number.isFinite(time)&&time>0&&time<=253402300799000;
export function replyTime(reply:Reply){return valid(reply.completedAt)?{at:reply.completedAt,completed:true}:{at:valid(reply.observedAt)?reply.observedAt:null,completed:false};}
export function latestSideReply(state:RoleState,role:BridgeRole){const endpoint=(role==="DECISION"?state.bindings.decision:state.bindings.execution)?.endpoint.id;return state.replies.filter(r=>endpoint&&r.endpointId===endpoint).sort((a,b)=>(b.observedAt??0)-(a.observedAt??0))[0];}
export function latestBridgeReply(state:RoleState){
 const candidates=(["DECISION","EXECUTION"] as BridgeRole[]).flatMap(role=>{const reply=latestSideReply(state,role);return reply?[{role,reply}]:[];});
 const byCompletion=candidates.length>0&&candidates.every(c=>valid(c.reply.completedAt));
 const rows=candidates.filter(c=>valid(byCompletion?c.reply.completedAt:c.reply.observedAt));
 rows.sort((a,b)=>(byCompletion?b.reply.completedAt!-a.reply.completedAt!:(b.reply.observedAt??0)-(a.reply.observedAt??0)));
 return rows[0]?{...rows[0],byCompletion}:null;
}

/** Reading arrival still uses chronology; attention is about the current cycle. */
export function bridgeReplyAttention(state:RoleState){
 const busy=state.activities?.some(a=>[state.bindings.decision,state.bindings.execution].some(side=>side?.endpoint.id===a.endpointId)&&Date.now()-a.checkedAt<15000&&["RUNNING","THINKING","RESULT_PENDING"].includes(a.state));
 if(busy)return null;
 const latest=latestBridgeReply(state);if(!latest||latest.reply.handledAt)return null;
 if(state.handoffs.some(h=>h.status==="SENT"&&h.sourceEndpoint?.id===latest.reply.endpointId&&h.sourceResponseIdentity&&h.sourceResponseIdentity===latest.reply.assistantIdentity))return null;
 return latest;
}
