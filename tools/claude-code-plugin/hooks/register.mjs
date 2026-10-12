// Official Claude Code Mods API. This module imports no Node/process/model APIs.
// It is NOT installed or enabled by building Agbrio.
const nativeId = value => typeof value === 'string' && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(value);
export function hostOrigin(value) {
  const u = new URL(value);
  if (u.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(u.hostname) || !u.port || u.username || u.password || u.pathname !== '/' || u.search || u.hash) throw new Error('AGBRIO_LOCAL_HOST_REQUIRED');
  return u.origin;
}
async function flush($, state) {
  while (state.pending.length) {
    const response = await $.http.fetch(`${state.origin}/v1/claude-code/events`, {method: 'POST', headers: {'content-type': 'application/json', authorization: `Bearer ${state.token}`}, body: JSON.stringify(state.pending[0])});
    if (!response.ok) throw new Error('AGBRIO_ADAPTER_DELIVERY_UNCONFIRMED');
    state.pending.shift();
  }
}
function emit($, state, kind, extra = {}) {
  if (!state.identity || state.stopped) return Promise.resolve();
  const event = {...state.identity, eventId: crypto.randomUUID(), instanceId: state.instanceId, sequence: ++state.sequence, kind, turnId: state.turnId, text: null, completedAt: null, ...extra};
  if (state.pending.length >= 64 || state.pending.reduce((bytes,item)=>bytes+new TextEncoder().encode(JSON.stringify(item)).length,0)+new TextEncoder().encode(JSON.stringify(event)).length>4_000_000) { state.stopped = true; return Promise.reject(new Error('AGBRIO_ADAPTER_QUEUE_FULL')); }
  state.pending.push(event);
  state.queue = state.queue.catch(() => {}).then(() => flush($, state));
  return state.queue;
}
export function register(on, options) {
  const origin = hostOrigin(options.hostUrl);
  if (!/^[0-9a-f]{64}$/.test(options.token ?? '')) throw new Error('AGBRIO_ADAPTER_TOKEN_REQUIRED');
  const state = {origin, token: options.token, instanceId: crypto.randomUUID(), identity: null, sequence: 0, turnId: null, stopped: false, queue: Promise.resolve(), pending: []};
  on('session.start', async ($, e, next) => {
    const sessionId = await $.session.id(); const cwd = await $.session.cwd();
    const build = await $.session.version(); const version = build.version;
    if (!nativeId(sessionId) || !/^\d+\.\d+\.\d+$/.test(version)) throw new Error('AGBRIO_NATIVE_IDENTITY_UNAVAILABLE');
    state.identity = {sessionId, cwd, version};
    void emit($, state, 'SESSION_START').catch(() => {});
    $.clock.every(5_000, async () => emit($, state, 'HEARTBEAT'));
    return next(e);
  });
  on('turn.start', async ($, e, next) => {
    if (e.agentId) return next(e);
    if (!nativeId(e.turnId)) throw new Error('AGBRIO_TURN_ID_MISSING');
    state.turnId = e.turnId; void emit($, state, 'RUNNING').catch(() => {}); return next(e);
  });
  on('turn.complete', async ($, e, next) => {
    const result = await next(e);
    if (e.agentId) return result;
    if (e.reason === 'answer' && !e.isAborted) {
      if (!nativeId(e.turnId) || e.turnId!==state.turnId || typeof e.answer!=='string') {void emit($, state,'RESULT_PENDING').catch(()=>{});}
      else if (!e.answer.trim()) {void emit($, state,'EMPTY').catch(()=>{});}
      else if (new TextEncoder().encode(e.answer).length>1_000_000) {void emit($, state,'LIMITED').catch(()=>{});}
      else {void emit($, state, 'COMPLETE', {turnId: e.turnId, text: e.answer, completedAt: $.clock.now()}).catch(() => {});}
    } else {
      void emit($, state, e.reason === 'aborted' || e.isAborted ? 'INTERRUPTED' : 'FAILED').catch(() => {});
    }
    return result;
  });
  on('classic.PermissionRequest', async ($, e, next) => { if (!e.agent_id) void emit($, state, 'WAITING_DECISION').catch(() => {}); return next(e); }).catch(async ($, e, next) => next(e));
  on('session.end', async ($, e, next) => { const delivery=emit($, state, 'SESSION_END'); state.stopped = true; void delivery.catch(() => {}); return next(e); });
}
