import {registerOwnerRequests,flushOwnerReceipts,releaseOwnerRequests} from './owner-requests.mjs';
import {registerCodexWait,initCodexWait,tickCodexWaits,appendCodexWait} from './codex-wait.mjs';
import {tickResourceNotice,appendResourceNotice,registerResourceExpiry} from './resource-notice.mjs';
import {tickTaskNotices,appendTaskNotice} from './codex-notices.mjs';
// Official Claude Code Mods API. This module imports no Node/process/model APIs.
// It is NOT installed or enabled by building Agbrio.
const nativeId = value => typeof value === 'string' && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(value);
export function hostOrigin(value) {
  const u = new URL(value);
  if (u.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(u.hostname) || !u.port || u.username || u.password || u.pathname !== '/' || u.search || u.hash) throw new Error('AGBRIO_LOCAL_HOST_REQUIRED');
  return u.origin;
}
function diagnostic(state) {
  return JSON.stringify({pluginVersion:'0.1.17',mode:'OWNER_MANUAL_INPUT',phase:state.phase,error:state.error,httpStatus:state.httpStatus,identityReady:!!state.identity,pendingEvents:state.pending.length,deliveredEvents:state.delivered,writesEnabled:false,manualInputProtocol:1,codexWaitProtocol:state.codexWaitReady?1:null,resourceNoticeProtocol:2,taskNoticeProtocol:1,taskSubscriptions:state.taskSubscriptions?.length??0,taskNoticeError:state.taskNoticeError??null});
}
function noteError(state, error) {
  const message=typeof error?.message==='string' ? error.message : '';
  state.error=['AGBRIO_HTTP_UNAVAILABLE','AGBRIO_ADAPTER_DELIVERY_UNCONFIRMED','AGBRIO_NATIVE_IDENTITY_UNAVAILABLE','AGBRIO_ADAPTER_QUEUE_FULL','AGBRIO_TURN_ID_MISSING'].includes(message) ? message : `AGBRIO_${state.phase}_FAILED`;
}
async function report($, state) {
  const key=`${state.phase}:${state.error}:${state.httpStatus}`;
  if(state.reported===key)return;
  state.reported=key;
  // Only allowlisted stage/code/counters. Never exception text, URL, ID or secret.
  try {await $.ui.log(`Agbrio connection: ${diagnostic(state)}`);} catch {}
}
async function flush($, state) {
  while (state.pending.length) {
    state.phase='HTTP';
    let response;
    try { response = await $.http.fetch(`${state.origin}/v1/claude-code/events`, {method: 'POST', headers: {'content-type': 'application/json', authorization: `Bearer ${state.token}`, 'x-agbrio-control-version':'1','x-agbrio-plugin-version':'0.1.17'}, body: JSON.stringify(state.pending[0])}); } catch {throw new Error('AGBRIO_HTTP_UNAVAILABLE');}
    state.httpStatus=Number.isInteger(response.status) ? response.status : null;
    if (!response.ok) throw new Error('AGBRIO_ADAPTER_DELIVERY_UNCONFIRMED');
    const recovered=state.error!==null;state.pending.shift();state.delivered++;state.phase='CONNECTED';state.error=null;state.reported=null;if(recovered)await report($,state);
    let envelope;try{envelope=JSON.parse(response.text||'{}');}catch{throw new Error('AGBRIO_COMMAND_RESPONSE_INVALID');}
    if(envelope.command) await acceptManual($,state,envelope.command);
  }
}
/** A desktop "spare" session starts in a temporary folder and later moves into
 * its real project; follow the session's current folder. */
export async function refreshCwd($, state) {
  // Only a spare session moves into its project. A normal session keeps its root even
  // when its working directory changes during a turn (for example into a subfolder).
  if (!state.identity || !/[\\/]\.claude[\\/]spares[\\/]spare-/i.test(state.identity.cwd)) return;
  try { const cwd = await $.session.cwd(); if (typeof cwd === 'string' && cwd && cwd !== state.identity.cwd) state.identity = {...state.identity, cwd}; } catch {}
}
export function emit($, state, kind, extra = {}) {
  if (!state.identity || state.stopped) return Promise.resolve();
  // While Agbrio is unreachable, at most one heartbeat waits: a liveness signal
  // must never fill the queue and switch this session's connection off.
  if (kind === 'HEARTBEAT' && state.pending.some(item => item.kind === 'HEARTBEAT')) { state.queue = state.queue.catch(() => {}).then(() => flush($, state)); return state.queue; }
  const event = {...state.identity, eventId: crypto.randomUUID(), instanceId: state.instanceId, sequence: ++state.sequence, kind, turnId: state.turnId, text: null, completedAt: null, ...extra};
  const content = state.pending.filter(item => item.kind !== 'HEARTBEAT');
  if (content.length >= 64 || content.reduce((bytes,item)=>bytes+new TextEncoder().encode(JSON.stringify(item)).length,0)+new TextEncoder().encode(JSON.stringify(event)).length>4_000_000) { state.stopped = true; return Promise.reject(new Error('AGBRIO_ADAPTER_QUEUE_FULL')); }
  state.pending.push(event);
  state.queue = state.queue.catch(() => {}).then(() => flush($, state));
  return state.queue;
}
export function register(on, options) {
  const origin = hostOrigin(options.hostUrl);
  if (!/^[0-9a-f]{64}$/.test(options.token ?? '')) throw new Error('AGBRIO_ADAPTER_TOKEN_REQUIRED');
  const state = {origin, token: options.token, instanceId: crypto.randomUUID(), identity: null, sequence: 0, turnId: null, stopped: false, queue: Promise.resolve(), pending: [],phase:'REGISTERED',error:null,httpStatus:null,delivered:0,reported:null};
  registerResourceExpiry(on,state);
  registerManual(on,state);
  registerOwnerRequests(on,state);
  registerCodexWait(on,state);
  on('command.run', {command:'agbrio-status'}, async () => ({text:diagnostic(state)}));
  on('session.start', async ($, e, next) => {
    // This is a native immediate command, not a prompt/model/tool registration.
    try {await $.command.register({name:'agbrio-status',description:'Read local Agbrio observation connection state; no model calls',immediate:true});} catch {}
    try {
      state.phase='SESSION_ID';const sessionId=await $.session.id();
      state.phase='CWD';const cwd=await $.session.cwd();
      state.phase='VERSION';const build=await $.session.version();const version=build.version;
      if (!nativeId(sessionId) || !/^\d+\.\d+\.\d+$/.test(version)) throw new Error('AGBRIO_NATIVE_IDENTITY_UNAVAILABLE');
      state.identity={sessionId,cwd,version};
      await initCodexWait($,state);
      void emit($,state,'SESSION_START').catch(async error=>{noteError(state,error);await report($,state);});
      $.clock.every(5_000,async()=>{try {await refreshCwd($,state);await flushOwnerReceipts($,state);await flushReceipt($,state);await emit($,state,'HEARTBEAT');} catch(error){noteError(state,error);await report($,state);}try{await tickTaskNotices($,state);}catch{state.taskNoticeError='AGBRIO_NOTICE_SOURCE_UNAVAILABLE';}try{await tickCodexWaits($,state);}catch{/* wait failure cannot disable observation or a manual receipt */}try{await tickResourceNotice($,state);}catch{/* optional resource notice must not disable observation */}});
    } catch(error) {noteError(state,error);void report($,state);}
    return next(e);
  });
  on('turn.start', async ($, e, next) => {
    if (e.agentId) return next(e);
    if (!nativeId(e.turnId)) throw new Error('AGBRIO_TURN_ID_MISSING');
    state.resourceBusy=true;state.turnId = e.turnId; void emit($, state, 'RUNNING').catch(async error => {noteError(state,error);await report($,state);}); return next(e);
  });
  on('turn.complete', async ($, e, next) => {
    const result = await next(e);
    if (e.agentId) return result;
    state.resourceBusy=false;await releaseOwnerRequests($,state);
    if (e.reason === 'answer' && !e.isAborted) {
      if (!nativeId(e.turnId) || e.turnId!==state.turnId || typeof e.answer!=='string') {void emit($, state,'RESULT_PENDING').catch(async error=>{noteError(state,error);await report($,state);});}
      else if (!e.answer.trim()) {void emit($, state,'EMPTY').catch(async error=>{noteError(state,error);await report($,state);});}
      else if (new TextEncoder().encode(e.answer).length>1_000_000) {void emit($, state,'LIMITED').catch(async error=>{noteError(state,error);await report($,state);});}
      else {void emit($, state, 'COMPLETE', {turnId: e.turnId, text: e.answer, completedAt: await $.clock.now()}).catch(async error => {noteError(state,error);await report($,state);});}
    } else {
      void emit($, state, e.reason === 'aborted' || e.isAborted ? 'INTERRUPTED' : 'FAILED').catch(async error => {noteError(state,error);await report($,state);});
    }
    return result;
  });
  on('classic.PermissionRequest', async ($, e, next) => { if (!e.agent_id) void emit($, state, 'WAITING_DECISION').catch(async error => {noteError(state,error);await report($,state);}); return next(e); }).catch(async ($, e, next) => { if (next.called) throw new Error('AGBRIO_NATIVE_PERMISSION_UNCONFIRMED'); return next(e); });
  on('session.end', async ($, e, next) => { const delivery=emit($, state, 'SESSION_END'); state.stopped = true; void delivery.catch(async error => {noteError(state,error);await report($,state);}); return next(e); });
}

// Official Mods calls only. No model helper, child session or UI automation.
export async function textHash(text){return Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',new TextEncoder().encode(text)))).map(v=>v.toString(16).padStart(2,'0')).join('');}
function receiptKey(id){return `agbrio-manual-v1:${id}`;}
async function postReceipt($,state,receipt){
  const result=await $.http.fetch(`${state.origin}/v1/claude-code/receipts`,{method:'POST',headers:{'content-type':'application/json',authorization:`Bearer ${state.token}`},body:JSON.stringify(receipt)});
  if(!result.ok)throw new Error('AGBRIO_RECEIPT_UNCONFIRMED');
}
async function saveReceipt($,state,status,messageId=null){
  const command=state.manualActive;
  if(!command)return;
  const receipt={requestId:command.requestId,sessionId:command.sessionId,instanceId:command.instanceId,cwd:command.cwd,status,messageId,textSha256:command.textSha256};
  state.manualReceipt=receipt;
  try{await $.store.set(receiptKey(command.requestId),{fingerprint:command.fingerprint,receipt});}catch{if(status!=='SENT')throw new Error('AGBRIO_RECEIPT_STORAGE_UNAVAILABLE');}
  try{await postReceipt($,state,receipt);state.manualReceipt=null;}catch{}
}
export async function flushReceipt($,state){if(state.manualReceipt){const current=state.manualReceipt;await postReceipt($,state,current);if(state.manualReceipt===current)state.manualReceipt=null;}}
export async function acceptManual($,state,command){
  if(!command)return;
  if(!state.identity||state.stopped||!nativeId(command.requestId)||command.sessionId!==state.identity.sessionId||command.instanceId!==state.instanceId||command.cwd!==state.identity.cwd||typeof command.text!=='string'||!command.text.trim()||new TextEncoder().encode(command.text).length>100_000||await textHash(command.text)!==command.textSha256)throw new Error('AGBRIO_COMMAND_TARGET_INVALID');
  const fingerprint=JSON.stringify([command.sessionId,command.instanceId,command.cwd,command.textSha256]);
  const key=receiptKey(command.requestId);
  const old=await $.store.get(key);
  if(old){
    if(old.fingerprint!==fingerprint)throw new Error('AGBRIO_COMMAND_REPLAY_CONFLICT');
    // Even a previously interrupted/unknown attempt is NEVER submitted again.
    state.manualReceipt=old.receipt;await flushReceipt($,state);return;
  }
  if(state.manualActive)throw new Error('AGBRIO_COMMAND_WRITER_BUSY');
  const active={...command,fingerprint,provenSent:false,notSent:false};
  state.manualActive=active;
  try{
    // A durable uncertainty marker is required before entering the physical call.
    // If persistence fails, no prompt is submitted.
    await $.store.set(key,{fingerprint,receipt:{requestId:command.requestId,sessionId:command.sessionId,instanceId:command.instanceId,cwd:command.cwd,status:'UNKNOWN',messageId:null,textSha256:command.textSha256}});
    const result=await $.prompt.submit({text:command.text});
    // Other mods can rewrite the result after core. A refusal-looking answer
    // alone cannot prove that no prompt was queued, so remain fail closed.
    if(result?.drop&&!active.provenSent)await saveReceipt($,state,'UNKNOWN');
    // submit resolving only proves acceptance/queueing, not a transcript row.
    // session.append below is the independent stored-message receipt.
  }catch(error){if(!active.provenSent)try{await saveReceipt($,state,'UNKNOWN');}catch{}throw error;}
  finally{if(state.manualActive===active&&(active.provenSent||active.notSent))state.manualActive=null;}
}
/** Newer Claude Code stores a plugin-submitted prompt inside its own framing:
 * "The agbrio-bridge plugin sent a message:\n<text>\n\nThis is how Claude Code
 * surfaces …". Returns the exact submitted text, or the input unchanged. */
export const PLUGIN_PROMPT_PREFIX='The agbrio-bridge plugin sent a message:\n';
export const PLUGIN_PROMPT_SUFFIX='\n\nThis is how Claude Code surfaces a prompt a plugin submits';
export function unwrapPluginPrompt(text){
  if(typeof text!=='string'||!text.startsWith(PLUGIN_PROMPT_PREFIX))return text;
  const body=text.slice(PLUGIN_PROMPT_PREFIX.length);const end=body.lastIndexOf(PLUGIN_PROMPT_SUFFIX);
  return end<0?body:body.slice(0,end);
}
export function registerManual(on,state){
  on('session.append',async($,e,next)=>{
    const result=await next(e);
    await appendCodexWait($,state,e,result);
    try{await appendResourceNotice($,state,e,result);}catch{/* resource receipt cannot break native append */}
    try{await appendTaskNotice($,state,e,result);}catch{/* native append remains authoritative */}
    const active=state.manualActive;
    if(!active||e.agentId||!['prompt','delivery'].includes(e.door)||e.origin?.kind!=='plugin'||e.origin.name!=='agbrio-bridge'||result?.message?.type!=='user'||!nativeId(result.uuid))return result;
    const content=result.message.content;
    let text;
    if(typeof content==='string')text=content;
    else if(Array.isArray(content)&&content.every(b=>b.type==='text'&&typeof b.text==='string'))text=content.map(b=>b.text).join('');
    else return result;
    if(unwrapPluginPrompt(text)!==active.text||await textHash(active.text)!==active.textSha256)return result;
    // A failed receipt transport must never abort/change the user's native row.
    active.provenSent=true;
    try{await saveReceipt($,state,'SENT',result.uuid);}catch{}
    if(state.manualActive===active)state.manualActive=null;
    return result;
  }).catch(async($,e,next)=>{if(next.called)throw new Error('AGBRIO_NATIVE_RECEIPT_UNCONFIRMED');return next(e);});
}
