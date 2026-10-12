// Offline control prototype. Not bundled in the observation plugin or loaded by Host.
const nativeId = value => typeof value === 'string' && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(value);
const inFlight = new Map();
export async function executeReviewedCommand($, command, context, policy = {writesEnabled: false}) {
  if (!policy.writesEnabled) return {status: 'NOT_SENT', error: 'CLAUDE_WRITES_DISABLED_BY_OWNER'};
  if (context.pendingApproval || context.unknownSend) throw new Error('AGBRIO_WRITER_OR_APPROVAL_UNRESOLVED');
  if (!nativeId(command.sessionId) || !nativeId(command.instanceId) || command.sessionId !== context.sessionId || command.instanceId !== context.instanceId || command.cwd !== context.cwd || command.bindingRevision !== context.bindingRevision || !nativeId(command.requestId) || !command.ownerApproved) throw new Error('AGBRIO_COMMAND_FENCE_FAILED');
  if (command.kind === 'SEND') {
    if (context.state !== 'IDLE' || !command.text?.trim() || new TextEncoder().encode(command.text).length > 100_000) throw new Error('AGBRIO_TARGET_NOT_IDLE');
    const actual = [...new Uint8Array(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(command.text)))].map(v => v.toString(16).padStart(2, '0')).join('');
    if (actual !== command.textSha256) throw new Error('AGBRIO_PAYLOAD_CHANGED');
  } else if (command.kind !== 'STOP' || !nativeId(command.expectedTurnId) || command.expectedTurnId !== context.turnId) throw new Error('AGBRIO_EXPECTED_TURN_MISMATCH');
  const fingerprint = JSON.stringify([command.sessionId,command.instanceId,command.cwd,command.bindingRevision,command.kind,command.text??null,command.textSha256??null,command.expectedTurnId??null]);
  const key = 'agbrio-command:' + context.sessionId + ':' + command.requestId;
  const current = inFlight.get(key);
  if (current) {
    if (current.fingerprint !== fingerprint) throw new Error('AGBRIO_REQUEST_REPLAY_CONFLICT');
    return current.promise;
  }
  const promise = (async () => {
    const prior = await $.store.get(key);
    if (prior) {
      if (prior.fingerprint !== fingerprint) throw new Error('AGBRIO_REQUEST_REPLAY_CONFLICT');
      return prior.receipt;
    }
    const uncertain = {status: 'UNKNOWN', requestId: command.requestId};
    await $.store.set(key, {fingerprint,receipt: uncertain});
    try {
      if (command.kind === 'SEND') {
        const result = await $.prompt.submit({text: command.text});
        if (result?.drop || result?.text !== command.text) throw new Error('AGBRIO_NATIVE_PROMPT_NOT_ACCEPTED_EXACTLY');
      } else { await $.turn.abort({turnId: command.expectedTurnId}); }
      const receipt = {status: 'ACCEPTED', requestId: command.requestId};
      await $.store.set(key, {fingerprint,receipt});
      return receipt;
    } catch { return uncertain; }
  })();
  inFlight.set(key,{fingerprint,promise});
  try { return await promise; } finally { if(inFlight.get(key)?.promise===promise)inFlight.delete(key); }
}
