import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { BrowserExecutor, sha256, snapshotDom, nativeUploadEvidence } from './executor.mjs';

const target = '00000000-0000-4000-8000-000000000001';
const other = '00000000-0000-4000-8000-000000000002';
test('canonical upload names require native file-ID, exact message text/target and size evidence', () => {
  const file = { filename: 'selected.txt', size: 53, sha256: sha256('selected') };
  const evidence = nativeUploadEvidence(target, 'approved', [file]);
  evidence.accept('https://chatgpt.com/backend-api/files/process_upload_stream', { file_name: 'selected.txt', file_id: 'file_123' });
  const message = { id: other, author: { role: 'user' }, content: { parts: ['approved'] }, metadata: { attachments: [{ id: 'file_123', name: 'selected(1).txt', size: 53 }] } };
  const body = { conversation_id: target, messages: [message] };
  evidence.accept('https://chatgpt.com/backend-api/f/conversation', { ...body, conversation_id: other });
  assert.equal(evidence.evidence, null);
  evidence.accept('https://chatgpt.com/backend-api/f/conversation', { ...body, messages: [{ ...message, content: { parts: ['modified'] } }] });
  assert.equal(evidence.evidence, null);
  evidence.accept('https://chatgpt.com/backend-api/f/conversation', body);
  assert.deepEqual(evidence.evidence, { userMessageId: other, files: [{ fileId: 'file_123', originalFilename: 'selected.txt', canonicalFilename: 'selected(1).txt', size: 53, sha256: file.sha256 }] });
});
test('name resemblance, unbound IDs and size mismatches cannot manufacture canonical upload acceptance', () => {
  for (const kind of ['missing-id', 'wrong-size', 'ambiguous-id']) {
    const evidence = nativeUploadEvidence(target, 'approved', [{ filename: 'selected.txt', size: 53, sha256: sha256('selected') }]);
    if (kind !== 'missing-id') evidence.accept('https://chatgpt.com/backend-api/files/process_upload_stream', { file_name: 'selected.txt', file_id: 'file_123' });
    if (kind === 'ambiguous-id') evidence.accept('https://chatgpt.com/backend-api/files/process_upload_stream', { file_name: 'selected.txt', file_id: 'file_456' });
    evidence.accept('https://chatgpt.com/backend-api/f/conversation', { conversation_id: target, messages: [{ id: other, author: { role: 'user' }, content: { parts: ['approved'] }, metadata: { attachments: [{ id: 'file_123', name: 'selected(1).txt', size: kind === 'wrong-size' ? 52 : 53 }] } }] });
    assert.equal(evidence.evidence, null);
  }
});
async function fixture(testContext) {
  const temporaryRoot = fileURLToPath(new URL('../../runtime/isolated-browser-executor/', import.meta.url)).replace(/[\\/]$/, '');
  const receiptDirectory = await mkdtemp(path.join(temporaryRoot, 'test-'));
  testContext.after(() => {
    assert.equal(path.dirname(path.resolve(receiptDirectory)), temporaryRoot);
    return rm(receiptDirectory, { recursive: true });
  });
  const executor = new BrowserExecutor({ receiptDirectory });
  let browserCalls = 0;
  executor._use = async () => { browserCalls++; throw new Error('No browser access is permitted for this assertion'); };
  return { executor, receiptDirectory, browserCalls: () => browserCalls };
}
test('hash mismatch and invalid exact identity reach no browser', async t => {
  const f = await fixture(t);
  assert.equal((await f.executor.send(target, 'd1', 'original\ntext', sha256('modified text'))).status, 'HASH_MISMATCH');
  assert.equal((await f.executor.observe('sidebar title')).status, 'INVALID_CONVERSATION_ID');
  assert.equal(f.browserCalls(), 0);
});
test('unsupported outer whitespace is rejected without altering bytes or touching browser', async t => {
  const f = await fixture(t);
  for (const payload of ['approved\n', '\napproved', ' approved ', '\u00a0approved']) {
    assert.deepEqual(await f.executor.send(target, sha256(payload), payload, sha256(payload)),
      { status: 'UNAVAILABLE', code: 'IMMUTABLE_OUTER_WHITESPACE_UNSUPPORTED' });
  }
  assert.equal(f.browserCalls(), 0);
});

for (const phase of ['ready', 'final']) test(`a challenge at ${phase} stops the reserved send before any click and reports authRequired`, async t => {
  const f = await fixture(t);
  let snapshots = 0, clicks = 0;
  const payload = 'approved public test text';
  const page = {
    url: () => `https://chatgpt.com/c/${target}`,
    emulateFocusedPage: async () => {},
    keyboard: { sendCharacter: async () => {} },
    mouse: { click: async () => { clicks++; } },
    evaluate: async () => { snapshots++; return { url: `https://chatgpt.com/c/${target}`, auth: snapshots === (phase === 'ready' ? 2 : 3), streaming: false, composerPresent: true, composerText: snapshots === 1 ? '' : payload, composerFiles: 0, sendReady: true, messages: [] }; },
    $: async () => ({ focus: async () => {}, evaluate: async (_fn, point) => point ? true : undefined, boundingBox: async () => ({ x: 0, y: 0, width: 20, height: 20 }) }),
  };
  f.executor._use = async operation => operation({ pages: async () => [page] });
  const result = await f.executor.send(target, `auth-${phase}`, payload, sha256(payload));
  assert.equal(result.status, 'UNKNOWN');
  assert.equal(result.authRequired, true);
  assert.equal(clicks, 0);
  assert.equal(result.submissionIntentCount, phase === 'ready' ? 0 : 1);
});
test('UNKNOWN reservation survives a fresh Executor and cannot be blindly resent', async t => {
  const f = await fixture(t);
  const row = { status: 'UNKNOWN', dispatchId: 'd2', conversationId: target, payloadSha256: sha256('immutable'), submissionIntentCount: 0, automaticResend: false };
  await f.executor._store(row, true);
  const restarted = new BrowserExecutor({ receiptDirectory: f.receiptDirectory });
  restarted._use = async () => { throw new Error('Restart must not touch a browser for resend'); };
  assert.deepEqual(await restarted.send(target, 'd2', 'immutable', row.payloadSha256), row);
  assert.deepEqual(await restarted.receipt('d2'), row);
});
test('same dispatch cannot be reused for a changed immutable payload or destination', async t => {
  const f = await fixture(t);
  await f.executor._store({ status: 'UNKNOWN', dispatchId: 'd3', conversationId: target, payloadSha256: sha256('approved') }, true);
  assert.equal((await f.executor.send(other, 'd3', 'approved', sha256('approved'))).status, 'DISPATCH_CONFLICT');
  assert.equal((await f.executor.send(target, 'd3', 'rewritten', sha256('rewritten'))).status, 'DISPATCH_CONFLICT');
  assert.equal(f.browserCalls(), 0);
});
test('SENT identity is durable and repeat reconcile performs no provider operation', async t => {
  const f = await fixture(t);
  const row = { status: 'SENT', dispatchId: 'd4', conversationId: target, payloadSha256: sha256('approved'), acceptedMessageId: 'provider-native-user-id' };
  await f.executor._store(row, true);
  const restarted = new BrowserExecutor({ receiptDirectory: f.receiptDirectory });
  restarted._use = async () => { throw new Error('A SENT receipt is already exact retained evidence'); };
  assert.deepEqual(await restarted.receipt('d4'), row);
  assert.deepEqual(await restarted.send(target, 'd4', 'approved', row.payloadSha256), row);
});
test('exclusive reservation prevents two independent writers from claiming the same dispatch', async t => {
  const f = await fixture(t);
  const second = new BrowserExecutor({ receiptDirectory: f.receiptDirectory });
  const row = { status: 'UNKNOWN', dispatchId: 'd5', conversationId: target, payloadSha256: sha256('approved') };
  const outcomes = await Promise.allSettled([f.executor._store(row, true), second._store(row, true)]);
  assert.equal(outcomes.filter(result => result.status === 'fulfilled').length, 1);
  assert.equal(outcomes.find(result => result.status === 'rejected').reason.code, 'EEXIST');
});
test('damaged durable receipt fails closed instead of treating the dispatch as new', async t => {
  const f = await fixture(t);
  await writeFile(f.executor._receiptPath('d6'), '{partial receipt');
  await assert.rejects(f.executor.send(target, 'd6', 'approved', sha256('approved')));
  assert.equal(f.browserCalls(), 0);
});

test('a changed selected file is rejected before any browser upload or write', async t => {
  const f = await fixture(t);
  const selected = path.join(f.receiptDirectory, 'selected.txt');
  await writeFile(selected, 'reviewed bytes');
  const reviewed = sha256('reviewed bytes');
  await writeFile(selected, 'changed bytes');
  assert.equal((await f.executor.send(target, 'file-change', 'approved', sha256('approved'),
    [{ path: selected, sha256: reviewed }])).status, 'ATTACHMENT_CHANGED_BEFORE_UPLOAD');
  assert.equal(f.browserCalls(), 0);
  assert.equal(await f.executor._read('file-change'), null);
});

test('retained attachment intent cannot be reused for another selected file set', async t => {
  const f = await fixture(t);
  const selected = path.join(f.receiptDirectory, 'selected.txt');
  const files = [{ path: selected, sha256: sha256('reviewed') }];
  const row = { status: 'UNKNOWN', dispatchId: 'file-intent', conversationId: target,
    payloadSha256: sha256('approved'), attachmentsSha256: sha256(JSON.stringify(files)), submissionIntentCount: 0 };
  await f.executor._store(row, true);
  assert.equal((await f.executor.send(target, 'file-intent', 'approved', row.payloadSha256, [])).status, 'DISPATCH_CONFLICT');
  assert.deepEqual(await f.executor.send(target, 'file-intent', 'approved', row.payloadSha256, files), row);
  assert.equal(f.browserCalls(), 0);
});


test('provider cloudflare_challenge alert activates auth stop even without a verification iframe', t => {
  const priorDocument = globalThis.document;
  const priorLocation = globalThis.location;
  t.after(() => { globalThis.document = priorDocument; globalThis.location = priorLocation; });
  const alert = { innerText: 'cloudflare_challenge\nRetry', getClientRects: () => [1], closest: () => null };
  globalThis.location = { origin: 'https://chatgpt.com', pathname: '/c/' + target, href: 'https://chatgpt.com/c/' + target };
  globalThis.document = {
    querySelector: () => null,
    querySelectorAll: selector => selector === '[role="dialog"], [role="alert"]' ? [alert] : [],
  };
  assert.equal(snapshotDom().auth, true);
  alert.closest = () => ({});
  assert.equal(snapshotDom().auth, false, 'quoted message content is not an account-security control');
});
