import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import net from 'node:net';
import test from 'node:test';
import {
  FrameDecoder, OFFICIAL_PIPE, assertAllowedOutgoing, classifyDiscovery, compareVersions,
  encodeFrame, evaluatePeer, readPipePeer, runProbe, summarizeSnapshot,
} from './codex-ipc-probe.mjs';

const SECRET = 'PRIVATE_CONVERSATION_TEXT';
const OWNER = 'owner-client';

// A minimal router and owner, so nothing here touches a real Desktop.
async function fakeRouter(options = {}) {
  const received = [];
  const pipe = String.raw`\\.\pipe\agbrio-ipc-probe-test-` + randomUUID();
  const decoderFor = () => new FrameDecoder(64 * 1024 * 1024);
  const server = net.createServer((socket) => {
    const decoder = decoderFor();
    socket.on('error', () => {});
    socket.on('data', (chunk) => {
      for (const { message } of decoder.push(chunk)) {
        received.push(message);
        if (message.method === 'initialize') {
          socket.write(encodeFrame({ type: 'response', requestId: message.requestId, resultType: 'success', method: 'initialize', handledByClientId: 'router', result: { clientId: 'probe-1' } }));
          // The router asks every client whether it can handle another client's request.
          socket.write(encodeFrame({ type: 'client-discovery-request', requestId: 'foreign-1', request: { method: 'ide-context' } }));
          socket.write(encodeFrame({ type: 'broadcast', method: 'client-status-changed', sourceClientId: OWNER, version: 0, params: {} }));
        } else if (message.method === 'thread-owner-discovery') {
          const known = message.params.conversationId === 'owned';
          socket.write(encodeFrame(known
            ? { type: 'response', requestId: message.requestId, resultType: 'success', method: message.method, handledByClientId: OWNER, result: { supportsUntrustedAppInput: true } }
            : { type: 'response', requestId: message.requestId, resultType: 'error', error: options.discoveryError ?? 'no-client-found' }));
        } else if (message.method === 'thread-stream-following-changed' && message.params.following) {
          if (options.oversizedSnapshot) {
            const header = Buffer.alloc(4);
            header.writeUInt32LE(200 * 1024 * 1024, 0);
            socket.write(header);
            continue;
          }
          socket.write(encodeFrame({
            type: 'broadcast', method: 'thread-stream-state-changed', sourceClientId: OWNER, version: 11,
            params: { conversationId: 'owned', hostId: 'local', change: { type: 'snapshot', revision: 7, conversationState: {
              threadRuntimeStatus: { type: 'idle' }, cwd: SECRET,
              requests: [{ method: 'item/tool/requestUserInput', params: { text: SECRET } }],
              turns: [{ items: [{ text: SECRET }] }], turnHistory: { history: { entitiesByKey: { a: { text: SECRET } } } },
            } } },
          }));
        }
      }
    });
  });
  await new Promise((resolve) => server.listen(pipe, resolve));
  return { pipe, received, close: () => new Promise((resolve) => server.close(resolve)) };
}

const offline = (router, extra = {}) => runProbe({ pipe: router.pipe, verifyPeer: false, exists: () => true, requestTimeoutMs: 3000, ...extra });

test('only read-only frames can leave the probe', () => {
  for (const method of ['thread-follower-start-turn', 'thread-follower-steer-turn', 'thread-follower-interrupt-turn', 'thread-follower-command-approval-decision', 'thread-follower-submit-user-input', 'thread-follower-set-queued-follow-ups-state']) {
    assert.throws(() => assertAllowedOutgoing({ type: 'request', method }), /PROBE_METHOD_FORBIDDEN/);
    assert.throws(() => assertAllowedOutgoing({ type: 'broadcast', method }), /PROBE_METHOD_FORBIDDEN/);
  }
  assert.throws(() => assertAllowedOutgoing({ type: 'client-discovery-response', response: { canHandle: true } }), /MUST_NOT_CLAIM/);
  assert.doesNotThrow(() => assertAllowedOutgoing({ type: 'request', method: 'thread-owner-discovery' }));
});

test('discovery, one follow and unfollow, with no conversation content in the report', async () => {
  const router = await fakeRouter();
  try {
    const report = await offline(router, { threads: ['owned', 'cold'], followThread: 'owned', disposableConfirmed: true });
    assert.equal(report.conclusion, 'READ_ONLY_PROBE_COMPLETED');
    const steps = Object.fromEntries(report.steps.filter((s) => s.step === 'DISCOVERY').map((s) => [s.thread, s]));
    assert.equal(steps.owned.outcome, 'OWNER_FOUND');
    assert.equal(steps.cold.outcome, 'NO_OWNER_ANSWERED');
    const follow = report.steps.find((s) => s.step === 'FOLLOW');
    assert.equal(follow.ok, true);
    assert.equal(follow.runtimeStatus, 'idle');
    assert.equal(follow.pendingRequests, 1);
    assert.equal(follow.historyEntities, 1);
    assert.equal(report.stats.discoveryRequestsAnswered, 1);
    assert.ok(!JSON.stringify(report).includes(SECRET));

    await new Promise((resolve) => setTimeout(resolve, 50));
    const follows = router.received.filter((m) => m.method === 'thread-stream-following-changed');
    assert.deepEqual(follows.map((m) => m.params.following), [true, false]);
    assert.ok(follows.every((m) => m.targetClientIds.length === 1 && m.targetClientIds[0] === OWNER));
    const answer = router.received.find((m) => m.type === 'client-discovery-response');
    assert.deepEqual(answer.response, { canHandle: false });
    assert.deepEqual([...new Set(router.received.map((m) => m.method).filter(Boolean))].sort(), ['initialize', 'thread-owner-discovery', 'thread-stream-following-changed']);
  } finally {
    await router.close();
  }
});

test('an oversized snapshot stops the probe without buffering it', async () => {
  const router = await fakeRouter({ oversizedSnapshot: true });
  try {
    const report = await offline(router, { threads: ['owned'], followThread: 'owned', disposableConfirmed: true, maxFrame: 1024 * 1024 });
    assert.equal(report.conclusion, 'STOP_PROTOCOL_ERROR');
    assert.match(report.steps.at(-1).code, /IPC_FRAME_OVER_PROBE_LIMIT/);
  } finally {
    await router.close();
  }
});

test('following needs an explicit disposable confirmation and an actual owner', async () => {
  await assert.rejects(runProbe({ pipe: 'x', verifyPeer: false, exists: () => true, followThread: 'owned' }), /DISPOSABLE_CONFIRMATION/);
  const router = await fakeRouter();
  try {
    const report = await offline(router, { threads: ['cold'], followThread: 'cold', disposableConfirmed: true });
    assert.equal(report.steps.find((s) => s.step === 'FOLLOW').reason, 'NO_OWNER_FOR_FOLLOW_THREAD');
    assert.equal(router.received.filter((m) => m.method === 'thread-stream-following-changed').length, 0);
  } finally {
    await router.close();
  }
});

test('the official pipe cannot skip the server identity check', async () => {
  await assert.rejects(runProbe({ pipe: OFFICIAL_PIPE, verifyPeer: false, exists: () => true }), /IDENTITY_CHECK_IS_MANDATORY/);
});

test('an untrusted server is refused before any frame is sent', async () => {
  const router = await fakeRouter();
  try {
    const report = await runProbe({
      pipe: router.pipe, exists: () => true, threads: ['owned'],
      peerReader: () => ({ pid: 1, image: String.raw`C:\Temp\ChatGPT.exe`, signatureStatus: 'NotSigned', signer: null }),
    });
    assert.equal(report.conclusion, 'STOP_PEER_UNVERIFIED');
    assert.equal(router.received.length, 0);
  } finally {
    await router.close();
  }
});

test('peer rules accept only the signed official package image', () => {
  const official = { image: String.raw`C:\Program Files\WindowsApps\OpenAI.Codex_26.1103.3517.0_x64__2p2nqsd0c76g0\app\ChatGPT.exe`, signatureStatus: 'Valid', signer: 'CN=OpenAI, O=OpenAI, L=San Francisco' };
  assert.equal(evaluatePeer(official).trusted, true);
  assert.deepEqual(evaluatePeer({ ...official, image: null }).reasons, ['IMAGE_UNREADABLE']);
  assert.ok(evaluatePeer({ ...official, image: String.raw`C:\Users\x\OpenAI.Codex_1\ChatGPT.exe` }).reasons.includes('IMAGE_NOT_IN_OFFICIAL_PACKAGE'));
  assert.ok(evaluatePeer({ ...official, signatureStatus: 'NotSigned' }).reasons.includes('SIGNATURE_NOT_VALID'));
  assert.ok(evaluatePeer({ ...official, signer: 'CN=Someone Else' }).reasons.includes('SIGNER_NOT_OPENAI'));
});

test('a real pipe peer lookup identifies this test process, and the rules refuse it', { skip: process.platform !== 'win32' }, async () => {
  const router = await fakeRouter();
  try {
    const peer = readPipePeer(router.pipe);
    assert.equal(peer.pid, process.pid);
    assert.equal(evaluatePeer(peer).trusted, false);
  } finally {
    await router.close();
  }
});

test('method table comparison fails closed', () => {
  assert.equal(compareVersions({ 'thread-owner-discovery': 1, 'thread-stream-following-changed': 1 }).compatible, true);
  assert.equal(compareVersions({ 'thread-owner-discovery': 2, 'thread-stream-following-changed': 1 }).compatible, false);
  assert.equal(compareVersions(null).compatible, false);
});

test('decoder handles split frames and rejects malformed lengths', () => {
  const frame = encodeFrame({ type: 'response', requestId: 'a' });
  const decoder = new FrameDecoder(1024);
  assert.deepEqual(decoder.push(frame.subarray(0, 3)), []);
  assert.deepEqual(decoder.push(frame.subarray(3, 6)), []);
  assert.equal(decoder.push(Buffer.concat([frame.subarray(6), frame])).length, 2);
  assert.throws(() => new FrameDecoder(1024).push(Buffer.alloc(4)), /IPC_FRAME_EMPTY/);
});

test('summaries and discovery classification expose no free text', () => {
  const summary = summarizeSnapshot({ cwd: SECRET, requests: [{ method: SECRET.length ? 'item/x' : '', params: SECRET }] });
  assert.ok(!JSON.stringify(summary).includes(SECRET));
  assert.equal(classifyDiscovery({ resultType: 'error', error: 'request-version-mismatch' }).outcome, 'VERSION_MISMATCH');
  assert.equal(classifyDiscovery({ resultType: 'success' }).outcome, 'ERROR');
});
