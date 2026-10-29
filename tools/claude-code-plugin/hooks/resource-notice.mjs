// Fixed owner-bound resource notices; no MCP action, process access or model API.
const KEY='agbrio-resource-notice-v1';
const TTL=120000,DROP='AGBRIO_RESOURCE_NOTICE_EXPIRED';
const uuid=v=>typeof v==='string'&&/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(v);
function owner(state){if(state.stopped||!uuid(state.identity?.sessionId)||typeof state.identity?.cwd!=='string')throw Error('AGBRIO_NOTICE_SESSION_UNAVAILABLE');return{sessionId:state.identity.sessionId,instanceId:state.instanceId,cwd:state.identity.cwd};}
function key(state){return `${KEY}:${owner(state).sessionId}`;}
function serial(state,work){const p=(state.resourceQueue??Promise.resolve()).catch(()=>{}).then(work);state.resourceQueue=p;return p;}
async function post($,state,input){const response=await $.http.fetch(`${state.origin}/v1/claude-code/resource-notice`,{method:'POST',headers:{'content-type':'application/json',authorization:`Bearer ${state.token}`},body:JSON.stringify({...owner(state),...input,protocolVersion:2})});if(!response.ok)throw Error('AGBRIO_NOTICE_HOST_UNAVAILABLE');return JSON.parse(response.text||'{}');}
async function saved($,state){const v=await $.store.get(key(state));return Array.isArray(v)?v.filter(v=>v?.sessionId===state.identity.sessionId&&v.cwd===state.identity.cwd):[];}
async function receipt($,state,v){const ack=await post($,state,{action:'RECEIPT',id:v.id,messageId:v.messageId});if(ack.accepted!==true)throw Error('AGBRIO_NOTICE_RECEIPT_UNCONFIRMED');}
function resourceText(e){if(e?.agentId||e?.origin?.kind!=='plugin'||e.origin.name!=='agbrio-bridge'||typeof e.text!=='string')return null;let text=e.text;const prefix='The agbrio-bridge plugin sent a message:\n',suffix='\n\nThis is how Claude Code surfaces a prompt a plugin submits';if(text.startsWith(prefix)&&text.lastIndexOf(suffix)>=prefix.length)text=text.slice(prefix.length,text.lastIndexOf(suffix));return text.startsWith('Agbrio 本机资源告警（只读，')?text:null;}
function sampled(text){const stamp=text?.match(/(?:^|\n)采样时间：([^\r\n（]+)/)?.[1];const at=Date.parse(stamp??'');return Number.isFinite(at)?at:null;}
function deadline(n){const at=Number.isSafeInteger(n.observedAt)?n.observedAt:sampled(n.text);return at===null?null:at+TTL;}
function expired(n,at){const end=deadline(n);return end===null||at>end||at<end-TTL;}
async function expiredReceipt($,state,r){const v=await post($,state,{action:'EXPIRED',id:r.id});if(v.accepted!==true)throw Error('AGBRIO_NOTICE_EXPIRY_UNCONFIRMED');}
async function finishDropped($,state,record){return serial(state,async()=>{const list=await saved($,state),r=list.find(v=>v.id===record.id&&v.status==='UNKNOWN');if(!r)return;const done={...r,status:'EXPIRED'};await $.store.set(key(state),list.map(v=>v.id===r.id?done:v));try{await expiredReceipt($,state,done);await $.store.set(key(state),list.filter(v=>v.id!==r.id));}catch{/* only expiry acknowledgement may retry */}});}
async function submitResource($,state,record){try{const result=await $.prompt.submit({text:record.text});if(result?.drop===DROP)await finishDropped($,state,record);}catch{/* unknown native result is never resubmitted */}}
export function registerResourceExpiry(on,state){
 on('prompt.submit',async($,e,next)=>{const text=resourceText(e);if(text===null)return next(e);if(state.manualActive?.text===text)return next(e);const list=await saved($,state),record=list.find(v=>v.text===text);if(!record)return next(e);state.resourceGuardKnown??=new Set();state.resourceGuardKnown.add(text);if(expired(record,await $.clock.now()))return{drop:DROP};return next(e);}).catch(async($,e,next)=>{if(resourceText(e)!==null&&state.resourceGuardKnown?.has(resourceText(e))&&!next.called)return{drop:DROP};if(next.called)throw Error('AGBRIO_RESOURCE_GUARD_UNCONFIRMED');return next(e);});
}
export async function tickResourceNotice($,state){return serial(state,async()=>{
 const now=await $.clock.now();if(now<(state.resourceNextAt??0))return;state.resourceNextAt=now+30000;
 const old=await saved($,state),retained=[];
 for(const v of old){if(v.status==='EXPIRED'){try{await expiredReceipt($,state,v);}catch{retained.push(v);}}else if(v.status==='DELIVERED'){try{await receipt($,state,v);}catch{retained.push(v);}}else retained.push(v);}
 await $.store.set(key(state),retained);
 // Unknown older sends remain fenced individually; they never block a new
 // critical event merely because an unrelated old callback was lost.
 if(state.resourceBusy===true)return;
 if(retained.length>=128)throw Error('AGBRIO_NOTICE_RECEIPT_LIMIT');
 const response=await post($,state,{action:'CLAIM'}),n=response.notice;if(!n)return;
 if(typeof n.id!=='string'||!/^resource:[0-9a-f-]{36}(?::[0-9a-f-]{36})?$/.test(n.id)||typeof n.text!=='string'||n.text.length>8192||!n.text.startsWith('Agbrio 本机资源告警（只读，'))throw Error('AGBRIO_NOTICE_INVALID');
 const record={...owner(state),id:n.id,text:n.text,observedAt:Number.isSafeInteger(n.observedAt)?n.observedAt:sampled(n.text),status:'UNKNOWN'};
 if(expired(record,await $.clock.now())){try{await expiredReceipt($,state,record);}catch{await $.store.set(key(state),[...retained,{...record,status:'EXPIRED'}]);}return;}
 // Host reservation AND plugin persistence precede the only native attempt.
 if(retained.some(v=>v.id===record.id))return;
 await $.store.set(key(state),[...retained,record]);state.resourceGuardKnown??=new Set();state.resourceGuardKnown.add(record.text);
 // Do not await: session.append can run while submit is awaiting native completion.
 void submitResource($,state,record);
});}
export async function appendResourceNotice($,state,e,result){
 if(e.agentId||!['prompt','delivery'].includes(e.door)||e.origin?.kind!=='plugin'||e.origin.name!=='agbrio-bridge'||result?.message?.type!=='user'||!uuid(result.uuid))return result;
 const c=result.message.content;let text=typeof c==='string'?c:Array.isArray(c)&&c.every(b=>b.type==='text')?c.map(b=>b.text).join(''):null;
 const prefix='The agbrio-bridge plugin sent a message:\n',suffix='\n\nThis is how Claude Code surfaces a prompt a plugin submits';
 if(text?.startsWith(prefix)&&text.lastIndexOf(suffix)>=prefix.length)text=text.slice(prefix.length,text.lastIndexOf(suffix));
 await serial(state,async()=>{const list=await saved($,state),old=list.find(v=>v.status==='UNKNOWN'&&text===v.text);if(!old)return;const done={...old,status:'DELIVERED',messageId:result.uuid};let updated=list.map(v=>v.id===old.id?done:v);await $.store.set(key(state),updated);try{await receipt($,state,done);updated=updated.filter(v=>v.id!==done.id);await $.store.set(key(state),updated);}catch{/* retain exact native-row receipt for retry, never resubmit */}});
 return result;
}
