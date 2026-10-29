// Same authenticated native session as the Desktop Code tab. Metadata only.
export function quotaWindows(usage){
 const rows=Array.isArray(usage?.rateLimits)?usage.rateLimits:[];
 const out=[];for(const kind of ['five_hour','seven_day']){const row=rows.find(v=>v?.kind===kind);if(!row||!Number.isFinite(row.percentUsed)||row.percentUsed<0||row.percentUsed>100)continue;const reset=typeof row.resetsAt==='string'?Date.parse(row.resetsAt):NaN;out.push({kind,percentUsed:row.percentUsed,...(Number.isFinite(reset)?{resetsAt:Math.trunc(reset/1000)}:{})});}return out;
}
export async function publishQuota($,state,usage){
 if(!state.identity||state.stopped)return;
 const now=await $.clock.now();if(state.quotaFailedUntil&&now<state.quotaFailedUntil)return;
 if(!usage&&state.quotaNextAt&&now<state.quotaNextAt)return;
 const windows=quotaWindows(usage??await $.session.usage());if(!windows.length)return;
 const fingerprint=JSON.stringify(windows);if(fingerprint===state.quotaFingerprint&&state.quotaNextAt&&now<state.quotaNextAt)return;
 state.quotaNextAt=now+60_000;state.quotaFingerprint=fingerprint;
 const result=await $.http.fetch(`${state.origin}/v1/claude-code/usage`,{method:'POST',headers:{'content-type':'application/json',authorization:`Bearer ${state.token}`},body:JSON.stringify({sessionId:state.identity.sessionId,instanceId:state.instanceId,cwd:state.identity.cwd,observedAt:now,windows})});
 if(result.ok){state.quotaFingerprint=fingerprint;state.quotaFailedUntil=0;}else {state.quotaFailedUntil=now+15_000;state.quotaNextAt=now+15_000;}
}
export function registerQuota(on,state){on('session.measure',async($,e,next)=>{const result=await next(e);try{await publishQuota($,state,e);}catch{/* quota never changes the user's native turn */}return result;});}
