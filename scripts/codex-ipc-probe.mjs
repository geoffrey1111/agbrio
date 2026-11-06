#!/usr/bin/env node
// Read-only probe for the Codex Desktop private IPC pipe.
//
// It can only: list the pipe, verify the server process, initialize, ask who owns a
// thread, and (for one explicitly named disposable thread) follow once to measure the
// snapshot and then unfollow. It has no way to start, steer, interrupt or answer
// anything: every outgoing frame passes assertAllowedOutgoing. Reports hold structure,
// sizes and timings only, never conversation content.
//
// The protocol is private to OpenAI and unstable. Facts come from an installed package
// and the public sources named in docs/CODEX_LIGHT_CONNECTION.md.
import { execFileSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import fs from 'node:fs';
import net from 'node:net';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

export const OFFICIAL_PIPE = String.raw`\\.\pipe\codex-ipc`;
export const MAX_PROTOCOL_FRAME = 256 * 1024 * 1024;
const DEFAULT_MAX_FRAME = 64 * 1024 * 1024;
const INITIALIZING_CLIENT = 'initializing-client';
const LOCAL_HOST = 'local';
export const REQUIRED_VERSIONS = Object.freeze({
  'thread-owner-discovery': 1,
  'thread-stream-following-changed': 1,
});

// The complete list of what this tool may ever send.
const ALLOWED = Object.freeze({
  request: new Set(['initialize', 'thread-owner-discovery']),
  broadcast: new Set(['thread-stream-following-changed']),
});
export function assertAllowedOutgoing(message) {
  if (message.type === 'client-discovery-response') {
    if (message.response?.canHandle !== false) throw new Error('PROBE_MUST_NOT_CLAIM_OWNERSHIP');
    return;
  }
  if (!ALLOWED[message.type]?.has(message.method)) {
    throw new Error(`PROBE_METHOD_FORBIDDEN:${message.type}:${message.method}`);
  }
}

export function encodeFrame(message) {
  const body = Buffer.from(JSON.stringify(message), 'utf8');
  const header = Buffer.alloc(4);
  header.writeUInt32LE(body.length, 0);
  return Buffer.concat([header, body]);
}

export class FrameDecoder {
  constructor(maxFrame) {
    this.maxFrame = maxFrame;
    this.chunks = [];
    this.buffered = 0;
    this.largest = 0;
  }
  push(chunk) {
    this.chunks.push(chunk);
    this.buffered += chunk.length;
    const frames = [];
    for (;;) {
      if (this.buffered < 4) return frames;
      const head = this.#peek(4);
      const length = head.readUInt32LE(0);
      if (length === 0) throw new Error('IPC_FRAME_EMPTY');
      if (length > MAX_PROTOCOL_FRAME) throw new Error('IPC_FRAME_OVER_PROTOCOL_LIMIT');
      if (length > this.maxFrame) throw new Error(`IPC_FRAME_OVER_PROBE_LIMIT:${length}`);
      if (this.buffered < 4 + length) return frames;
      const all = Buffer.concat(this.chunks, this.buffered);
      const body = all.subarray(4, 4 + length);
      const rest = all.subarray(4 + length);
      this.chunks = rest.length ? [rest] : [];
      this.buffered = rest.length;
      this.largest = Math.max(this.largest, length);
      frames.push({ length, message: JSON.parse(body.toString('utf8')) });
    }
  }
  #peek(count) {
    return Buffer.concat(this.chunks, this.buffered).subarray(0, count);
  }
}

export class ProbeClient {
  constructor(socket, { maxFrame = DEFAULT_MAX_FRAME } = {}) {
    this.socket = socket;
    this.clientId = INITIALIZING_CLIENT;
    this.decoder = new FrameDecoder(maxFrame);
    this.pending = new Map();
    this.broadcastWaiters = [];
    this.stats = { discoveryRequestsAnswered: 0, broadcastsIgnored: 0 };
    this.failure = null;
    socket.on('data', (chunk) => {
      try {
        for (const frame of this.decoder.push(chunk)) this.#dispatch(frame);
      } catch (error) {
        this.#fail(error);
      }
    });
    socket.on('error', (error) => this.#fail(error));
    socket.on('close', () => this.#fail(new Error('IPC_CONNECTION_CLOSED')));
  }
  #fail(error) {
    this.failure ??= error;
    for (const { reject } of this.pending.values()) reject(this.failure);
    this.pending.clear();
    for (const waiter of this.broadcastWaiters.splice(0)) waiter.reject(this.failure);
    if (!this.socket.destroyed) this.socket.destroy();
  }
  #dispatch({ length, message }) {
    if (message.type === 'response') {
      const pending = this.pending.get(message.requestId);
      if (pending) {
        this.pending.delete(message.requestId);
        pending.resolve(message);
      }
      return;
    }
    if (message.type === 'client-discovery-request') {
      // Never claim another client's thread. Silence would stall the router for 10 s.
      this.#send({ type: 'client-discovery-response', requestId: message.requestId, response: { canHandle: false } });
      this.stats.discoveryRequestsAnswered++;
      return;
    }
    if (message.type === 'broadcast') {
      const index = this.broadcastWaiters.findIndex((waiter) => waiter.match(message));
      if (index >= 0) this.broadcastWaiters.splice(index, 1)[0].resolve({ length, message });
      else this.stats.broadcastsIgnored++;
    }
  }
  #send(message) {
    assertAllowedOutgoing(message);
    this.socket.write(encodeFrame(message));
  }
  request(method, version, params, timeoutMs) {
    if (this.failure) return Promise.reject(this.failure);
    const requestId = randomUUID();
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(requestId);
        reject(new Error('IPC_REQUEST_TIMEOUT'));
      }, timeoutMs);
      this.pending.set(requestId, {
        resolve: (value) => { clearTimeout(timer); resolve(value); },
        reject: (error) => { clearTimeout(timer); reject(error); },
      });
      try {
        this.#send({ type: 'request', requestId, sourceClientId: this.clientId, version, method, params });
      } catch (error) {
        clearTimeout(timer);
        this.pending.delete(requestId);
        reject(error);
      }
    });
  }
  async initialize(clientType, timeoutMs) {
    const reply = await this.request('initialize', 0, { clientType }, timeoutMs);
    if (reply.resultType !== 'success' || !reply.result?.clientId) throw new Error(`IPC_INITIALIZE_FAILED:${reply.error ?? 'NO_CLIENT_ID'}`);
    this.clientId = reply.result.clientId;
    return this.clientId;
  }
  setFollowing(owner, conversationId, following) {
    this.#send({
      type: 'broadcast',
      method: 'thread-stream-following-changed',
      sourceClientId: this.clientId,
      targetClientIds: [owner],
      version: REQUIRED_VERSIONS['thread-stream-following-changed'],
      params: { conversationId, hostId: LOCAL_HOST, following },
    });
  }
  waitForBroadcast(match, timeoutMs) {
    if (this.failure) return Promise.reject(this.failure);
    return new Promise((resolve, reject) => {
      const waiter = {
        match,
        resolve: (value) => { clearTimeout(timer); resolve(value); },
        reject: (error) => { clearTimeout(timer); reject(error); },
      };
      const timer = setTimeout(() => {
        this.broadcastWaiters.splice(this.broadcastWaiters.indexOf(waiter), 1);
        reject(new Error('IPC_SNAPSHOT_TIMEOUT'));
      }, timeoutMs);
      this.broadcastWaiters.push(waiter);
    });
  }
  close() {
    this.failure ??= new Error('IPC_CLOSED_BY_PROBE');
    this.socket.end();
  }
}

// Field names and counts only. Text, paths and commands stay out of the report.
export function summarizeSnapshot(state) {
  const history = state?.turnHistory?.history?.entitiesByKey;
  return {
    keys: Object.keys(state ?? {}).sort(),
    runtimeStatus: typeof state?.threadRuntimeStatus?.type === 'string' ? state.threadRuntimeStatus.type : null,
    pendingRequests: Array.isArray(state?.requests) ? state.requests.length : null,
    pendingRequestMethods: Array.isArray(state?.requests)
      ? [...new Set(state.requests.map((r) => r?.method).filter((m) => typeof m === 'string'))].sort()
      : [],
    turns: Array.isArray(state?.turns) ? state.turns.length : null,
    historyEntities: history && typeof history === 'object' ? Object.keys(history).length : null,
  };
}

export function classifyDiscovery(reply) {
  if (reply.resultType === 'success' && reply.handledByClientId) return { outcome: 'OWNER_FOUND', owner: reply.handledByClientId, result: { supportsUntrustedAppInput: reply.result?.supportsUntrustedAppInput === true } };
  // `no-client-found` cannot separate "nobody holds it" from "the owner is hung"; the writer lock decides that.
  if (reply.error === 'no-client-found') return { outcome: 'NO_OWNER_ANSWERED' };
  if (reply.error === 'request-version-mismatch') return { outcome: 'VERSION_MISMATCH' };
  return { outcome: 'ERROR', error: typeof reply.error === 'string' ? reply.error : 'UNKNOWN' };
}

export function pipeExists(pipe) {
  if (process.platform !== 'win32') throw new Error('PROBE_WINDOWS_ONLY');
  return fs.readdirSync(String.raw`\\.\pipe\\`).some((name) => path.win32.join(String.raw`\\.\pipe\\`, name).toLowerCase() === pipe.toLowerCase());
}

const IDENTITY_SCRIPT = String.raw`
$ErrorActionPreference='Stop'
Add-Type -TypeDefinition @'
using System;using System.Runtime.InteropServices;using Microsoft.Win32.SafeHandles;
public static class AgbrioPipePeer{[DllImport("kernel32.dll",SetLastError=true)]public static extern bool GetNamedPipeServerProcessId(SafePipeHandle h,out uint pid);}
'@
$c=[System.IO.Pipes.NamedPipeClientStream]::new('.',$args[0],[System.IO.Pipes.PipeDirection]::InOut)
$c.Connect(3000)
$serverPid=[uint32]0
if(-not [AgbrioPipePeer]::GetNamedPipeServerProcessId($c.SafePipeHandle,[ref]$serverPid)){throw 'PEER_PID_FAILED'}
$c.Dispose()
$image=$null;$status=$null;$signer=$null
try{$image=(Get-Process -Id $serverPid).Path}catch{}
if($image){$sig=Get-AuthenticodeSignature -LiteralPath $image;$status=[string]$sig.Status;if($sig.SignerCertificate){$signer=$sig.SignerCertificate.Subject}}
[pscustomobject]@{pid=$serverPid;image=$image;signatureStatus=$status;signer=$signer}|ConvertTo-Json -Compress
`;

export function readPipePeer(pipe) {
  const name = pipe.replace(/^\\\\\.\\pipe\\/i, '');
  const psCommand = `& {${IDENTITY_SCRIPT}} ${JSON.stringify(name).replace(/"/g, "'")}`;
  const stdout = execFileSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', psCommand], { encoding: 'utf8', timeout: 20000, windowsHide: true });
  return JSON.parse(stdout.trim().split(/\r?\n/).pop());
}

export function evaluatePeer(peer) {
  const reasons = [];
  if (!peer?.image) reasons.push('IMAGE_UNREADABLE');
  else {
    if (path.win32.basename(peer.image).toLowerCase() !== 'chatgpt.exe') reasons.push('IMAGE_NAME_NOT_CHATGPT');
    if (!/\\WindowsApps\\OpenAI\.Codex_[^\\]+\\/i.test(peer.image)) reasons.push('IMAGE_NOT_IN_OFFICIAL_PACKAGE');
    if (peer.signatureStatus !== 'Valid') reasons.push('SIGNATURE_NOT_VALID');
    if (!/\bOpenAI\b/i.test(peer.signer ?? '')) reasons.push('SIGNER_NOT_OPENAI');
  }
  return { trusted: reasons.length === 0, reasons };
}

// Reads the method table from an installed app.asar; returns null if it cannot be found.
export function readInstalledVersions(asarPath) {
  const fd = fs.openSync(asarPath, 'r');
  try {
    const size = Buffer.alloc(16);
    fs.readSync(fd, size, 0, 16, 0);
    const headerSize = size.readUInt32LE(4);
    const jsonLength = size.readUInt32LE(12);
    const headerBytes = Buffer.alloc(jsonLength);
    fs.readSync(fd, headerBytes, 0, jsonLength, 16);
    const entry = JSON.parse(headerBytes.toString('utf8')).files?.['.vite']?.files?.build?.files?.['early-bootstrap.js'];
    if (!entry?.size) return null;
    const body = Buffer.alloc(Number(entry.size));
    fs.readSync(fd, body, 0, body.length, 8 + headerSize + Number(entry.offset));
    const text = body.toString('utf8');
    const anchor = text.indexOf('"thread-owner-discovery":');
    if (anchor < 0) return null;
    const start = text.lastIndexOf('{', anchor);
    const end = text.indexOf('}', anchor);
    return JSON.parse(text.slice(start, end + 1));
  } finally {
    fs.closeSync(fd);
  }
}

export function compareVersions(installed) {
  if (!installed) return { compatible: false, mismatches: ['TABLE_NOT_FOUND'] };
  const mismatches = Object.entries(REQUIRED_VERSIONS)
    .filter(([method, version]) => installed[method] !== version)
    .map(([method, version]) => `${method}: expected ${version}, installed ${installed[method] ?? 'absent'}`);
  return { compatible: mismatches.length === 0, mismatches };
}

function connect(pipe, timeoutMs) {
  return new Promise((resolve, reject) => {
    const socket = net.connect(pipe);
    const timer = setTimeout(() => { socket.destroy(); reject(new Error('IPC_CONNECT_TIMEOUT')); }, timeoutMs);
    socket.once('connect', () => { clearTimeout(timer); socket.removeAllListeners('error'); resolve(socket); });
    socket.once('error', (error) => { clearTimeout(timer); reject(error); });
  });
}

export async function runProbe({
  pipe = OFFICIAL_PIPE,
  threads = [],
  followThread = null,
  disposableConfirmed = false,
  asarPath = null,
  maxFrame = DEFAULT_MAX_FRAME,
  clientType = 'agbrio-ipc-probe',
  requestTimeoutMs = 15000,
  verifyPeer = true,
  peerReader = readPipePeer,
  exists = pipeExists,
} = {}) {
  const report = { pipe, steps: [], writeMethodsAvailable: false };
  const finish = (conclusion) => ({ ...report, conclusion });
  if (verifyPeer === false && pipe.toLowerCase() === OFFICIAL_PIPE.toLowerCase()) throw new Error('PROBE_IDENTITY_CHECK_IS_MANDATORY_ON_OFFICIAL_PIPE');
  if (followThread && !disposableConfirmed) throw new Error('PROBE_FOLLOW_REQUIRES_DISPOSABLE_CONFIRMATION');

  if (asarPath) {
    const check = compareVersions(readInstalledVersions(asarPath));
    report.steps.push({ step: 'METHOD_TABLE', ...check });
    if (!check.compatible) return finish('STOP_VERSION_TABLE_MISMATCH');
  }
  if (!exists(pipe)) {
    report.steps.push({ step: 'PIPE', present: false });
    return finish('PIPE_ABSENT');
  }
  report.steps.push({ step: 'PIPE', present: true });

  if (verifyPeer) {
    let peer;
    try {
      peer = peerReader(pipe);
    } catch (error) {
      report.steps.push({ step: 'PEER', trusted: false, reasons: ['PEER_READ_FAILED'], detail: String(error.message).slice(0, 160) });
      return finish('STOP_PEER_UNVERIFIED');
    }
    const verdict = evaluatePeer(peer);
    report.steps.push({ step: 'PEER', pid: peer.pid, image: peer.image, signatureStatus: peer.signatureStatus, ...verdict });
    if (!verdict.trusted) return finish('STOP_PEER_UNVERIFIED');
  }

  let socket;
  const connectedAt = performance.now();
  try {
    socket = await connect(pipe, 5000);
  } catch (error) {
    report.steps.push({ step: 'CONNECT', ok: false, code: error.code ?? error.message });
    return finish('STOP_CONNECT_FAILED');
  }
  const client = new ProbeClient(socket, { maxFrame });
  try {
    const clientId = await client.initialize(clientType, requestTimeoutMs);
    report.steps.push({ step: 'INITIALIZE', ok: true, ms: Math.round(performance.now() - connectedAt), clientIdIssued: Boolean(clientId) });

    const owners = new Map();
    for (const thread of threads) {
      const started = performance.now();
      const reply = await client.request('thread-owner-discovery', REQUIRED_VERSIONS['thread-owner-discovery'], { hostId: LOCAL_HOST, conversationId: thread }, requestTimeoutMs);
      const outcome = classifyDiscovery(reply);
      if (outcome.owner) owners.set(thread, outcome.owner);
      const { owner: _owner, ...publicOutcome } = outcome;
      report.steps.push({ step: 'DISCOVERY', thread, ms: Math.round(performance.now() - started), ...publicOutcome });
    }

    if (followThread) {
      const owner = owners.get(followThread);
      if (!owner) {
        report.steps.push({ step: 'FOLLOW', ok: false, reason: 'NO_OWNER_FOR_FOLLOW_THREAD' });
      } else {
        const started = performance.now();
        const snapshot = client.waitForBroadcast((m) => m.method === 'thread-stream-state-changed' && m.sourceClientId === owner && m.params?.conversationId === followThread && m.params?.change?.type === 'snapshot', requestTimeoutMs);
        client.setFollowing(owner, followThread, true);
        try {
          const { length, message } = await snapshot;
          report.steps.push({ step: 'FOLLOW', ok: true, ms: Math.round(performance.now() - started), frameBytes: length, wireVersion: message.version ?? null, ...summarizeSnapshot(message.params.change.conversationState) });
        } finally {
          if (!client.failure) client.setFollowing(owner, followThread, false);
        }
      }
    }
    report.stats = client.stats;
    return finish('READ_ONLY_PROBE_COMPLETED');
  } catch (error) {
    report.steps.push({ step: 'ERROR', code: String(error.message).slice(0, 160) });
    report.stats = client.stats;
    return finish('STOP_PROTOCOL_ERROR');
  } finally {
    client.close();
  }
}

function parseArgs(argv) {
  const out = { threads: [] };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    const value = () => {
      const next = argv[++i];
      if (next === undefined) throw new Error(`Missing value for ${arg}`);
      return next;
    };
    if (arg === '--thread') out.threads.push(value());
    else if (arg === '--follow-disposable-thread') { out.followThread = value(); out.disposableConfirmed = true; }
    else if (arg === '--asar') out.asarPath = value();
    else if (arg === '--max-frame-mib') out.maxFrame = Number(value()) * 1024 * 1024;
    else throw new Error(`Unknown argument ${arg}`);
  }
  if (out.followThread && !out.threads.includes(out.followThread)) out.threads.push(out.followThread);
  return out;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    const report = await runProbe(parseArgs(process.argv.slice(2)));
    console.log(JSON.stringify(report, null, 2));
    process.exitCode = report.conclusion === 'READ_ONLY_PROBE_COMPLETED' || report.conclusion === 'PIPE_ABSENT' ? 0 : 2;
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
