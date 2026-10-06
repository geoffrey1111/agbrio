import { createHash } from 'node:crypto';
import { mkdir, open, readFile, rename, stat } from 'node:fs/promises';
import { fileSha256 } from './native-runtime.mjs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import puppeteer from 'puppeteer-core';

const ORIGIN = 'https://chatgpt.com';
const COMPOSER = '#prompt-textarea, main [role="textbox"][contenteditable="true"][data-composer-markdown]';
const SEND = 'button[data-testid="send-button"], button[aria-label="Send prompt"], button[aria-label="发送提示"], button[aria-label="发送消息"], button[aria-label="发送"]';
const ID = /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/i;
export const sha256 = text => createHash('sha256').update(text, 'utf8').digest('hex');
const delay = milliseconds => new Promise(resolve => setTimeout(resolve, milliseconds));
const executeFile = promisify(execFile);

// Passive native Web request metadata only. Never reads headers/cookies, sends
// an API request, or retains a conversation payload. Provider filenames may be
// canonicalized; exact uploaded file IDs bind that canonical name to selection.
export function nativeUploadEvidence(conversationId, payload, files) {
  const uploaded = new Map();
  let evidence = null;
  return {
    get evidence() { return evidence; },
    accept(url, body) {
      const parsed = new URL(url);
      if (parsed.origin !== ORIGIN || !body || typeof body !== 'object') return;
      if (parsed.pathname === '/backend-api/files/process_upload_stream') {
        const file = files.find(item => item.filename === body.file_name);
        if (file && /^file[_-][a-z0-9_-]+$/i.test(body.file_id ?? '')) {
          const ids = uploaded.get(file.filename) ?? new Set(); ids.add(body.file_id); uploaded.set(file.filename, ids);
        }
      } else if (['/backend-api/f/conversation', '/backend-api/conversation'].includes(parsed.pathname) && body.conversation_id === conversationId) {
        const messages = (Array.isArray(body.messages) ? body.messages : []).filter(message => message.author?.role === 'user'
          && message.content?.parts?.filter(part => typeof part === 'string').length === 1 && message.content.parts.includes(payload));
        if (messages.length !== 1 || !ID.test(messages[0].id ?? '')) return;
        const attachments = messages[0].metadata?.attachments;
        if (!Array.isArray(attachments) || attachments.length !== files.length) return;
        const refs = files.map(file => {
          const ids = uploaded.get(file.filename);
          if (ids?.size !== 1) return null;
          const matches = attachments.filter(item => item.id === [...ids][0] && item.size === file.size && typeof item.name === 'string');
          return matches.length === 1 ? { fileId: matches[0].id, originalFilename: file.filename, canonicalFilename: matches[0].name, size: file.size, sha256: file.sha256 } : null;
        });
        if (refs.every(Boolean) && new Set(refs.map(ref => ref.fileId)).size === files.length) evidence = { userMessageId: messages[0].id, files: refs };
      }
    },
  };
}

// This evaluates only the one explicitly selected exact provider page. DOM,
// Chrome handles and transport details never enter Router Domain/Application.
export function snapshotDom({ beforeUserMessageIds, expectedFilenames = [] } = {}) {
  const visible = element => element && element.getClientRects().length > 0;
  const securityFrame = [...document.querySelectorAll('iframe[src*="challenges.cloudflare.com"]')].some(visible);
  const challengeHeading = document.querySelector('#challenge-running, #challenge-stage');
  const login = document.querySelector('[data-testid="login-button"], a[href^="/auth/login"]');
  const loginLabel = [...document.querySelectorAll('button,a')].some(element => visible(element) && /^(Log in|Sign in|登录|登入)$/i.test(element.innerText.trim()));
  const accountStop = [...document.querySelectorAll('[role="dialog"], [role="alert"]')].some(element => visible(element)
    && !element.closest('[data-message-author-role], [data-chatgpt-search-message-ids]')
    && /cloudflare[_ -]challenge|challenge[_ -]required|verify (?:you are human|your identity)|suspicious activity|unusual activity|temporarily (?:blocked|restricted)|account.{0,20}restricted|captcha|人机验证|安全验证|异常活动|账户.{0,12}限制/i.test(element.innerText));
  const auth = location.origin === 'https://auth.openai.com' || location.pathname.startsWith('/auth/') || securityFrame || visible(challengeHeading) || visible(login) || loginLabel || accountStop;
  const streaming = !!document.querySelector('[data-testid="stop-button"], button[aria-label="Stop streaming"], button[aria-label*="停止"], .result-streaming');
  const legacy = [...document.querySelectorAll('[data-message-author-role]')];
  const elements = legacy.length ? legacy : [...document.querySelectorAll('[data-chatgpt-search-message-ids]')].filter(element => element.querySelector('[data-user-message-bubble], [data-markdown-text-style="assistant-message"]'));
  const roleOf = element => element.getAttribute('data-message-author-role') ?? (element.querySelector('[data-user-message-bubble]') ? 'user' : 'assistant');
  const latestAssistant = elements.findLast(element => roleOf(element) === 'assistant');
  const priorIds = beforeUserMessageIds ? new Set(beforeUserMessageIds) : null;
  const messages = elements.map(element => {
    const role = roleOf(element);
    const content = element.querySelector('.markdown, [data-markdown-text-style="assistant-message"], [data-user-message-bubble] .whitespace-pre-wrap') ?? element;
    const nativeIds = [...new Set((element.getAttribute('data-chatgpt-search-message-ids') ?? '').split(/\s+/).filter(Boolean))];
    const nativeId = nativeIds.length === 1 && /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/i.test(nativeIds[0]) ? nativeIds[0] : null;
    const id = element.getAttribute('data-message-id') ?? element.closest('[data-message-id]')?.getAttribute('data-message-id') ?? nativeId;
    const includeText = element === latestAssistant || (role === 'user' && priorIds && !priorIds.has(id));
    return {
      role, id,
      identitySource: element.hasAttribute('data-message-author-role') ? 'data-message-id' : 'data-chatgpt-search-message-ids:single-unique-uuid',
      turnId: element.closest('[data-turn-id]')?.getAttribute('data-turn-id'),
      text: includeText ? (content.innerText ?? content.textContent ?? '') : '',
      fileNames: includeText && role === 'user' ? expectedFilenames.filter(name =>
        [...element.querySelectorAll('*')].some(node => visible(node) && node.textContent?.trim() === name)) : [],
    };
  });
  const composers = [...document.querySelectorAll('#prompt-textarea, main [role="textbox"][contenteditable="true"][data-composer-markdown]')].filter(visible);
  const composer = composers.length === 1 ? composers[0] : null;
  // ProseMirror serializes adjacent paragraphs with one LF; innerText inserts
  // visual paragraph spacing. Read editor structure without rewriting payload.
  const paragraphs = composer ? [...composer.children] : [];
  const composerText = !composer ? null : composer.value ?? (composer.classList.contains('ProseMirror') && paragraphs.length && paragraphs.every(element => element.tagName === 'P')
    ? paragraphs.map(element => element.textContent === '' ? '' : element.innerText).join('\n')
    : composer.innerText ?? composer.textContent ?? '');
  const buttons = [...document.querySelectorAll('button[data-testid="send-button"], button[aria-label="Send prompt"], button[aria-label="发送提示"], button[aria-label="发送消息"], button[aria-label="发送"]')].filter(visible);
  const composerRoot = composer?.closest('[data-composer-root], [class*="ComposerLayoutRoot-"]') ?? composer?.closest('form');
  const composerFiles = composerRoot ? [...composerRoot.querySelectorAll('button')]
    .filter(element => /remove file|remove attachment|移除文件|删除附件|移除附件/i.test(element.getAttribute('aria-label') ?? '')).length : 0;
  return {
    url: location.href, auth, streaming, messages,
    composerPresent: visible(composer),
    composerText,
    sendReady: buttons.length === 1 && !buttons[0].disabled,
    composerFiles,
  };
}

export function exactId(url) {
  try {
    const parsed = new URL(url);
    const match = parsed.pathname.match(/^\/(?:g\/[^/]+\/)?c\/([^/]+)\/?$/);
    return parsed.origin === ORIGIN && match && ID.test(match[1]) ? match[1] : null;
  } catch { return null; }
}

export class BrowserExecutor {
  constructor({ browserURL = 'http://127.0.0.1:9229', receiptDirectory, connectionProvider } = {}) {
    const control = new URL(browserURL);
    if (control.protocol !== 'http:' || control.hostname !== '127.0.0.1' || control.username || control.password || control.pathname !== '/') {
      throw new Error('Dedicated Chrome CDP must use a trusted local loopback endpoint');
    }
    this.control = control;
    this.receiptDirectory = receiptDirectory ?? fileURLToPath(new URL('../../runtime/isolated-browser-executor/receipts/', import.meta.url));
    // Production supplies the narrow native runtime's already-owned CDP
    // connection. The historical Spike guard remains only for its retained CLI.
    this.connectionProvider = connectionProvider;
  }

  async _use(operation) {
    if (this.connectionProvider) {
      try { return await operation(await this.connectionProvider()); }
      catch (error) { return { status: 'UNAVAILABLE', code: error.code ?? (/^[A-Z_]{1,80}$/.test(error.message ?? '') ? error.message : 'CDP_OPERATION_FAILED') }; }
    }
    let browser;
    try {
      const verified = await executeFile('pwsh.exe', ['-NoProfile', '-NonInteractive', '-File', fileURLToPath(new URL('./Assert-OwnedChromium.ps1', import.meta.url))], { timeout: 10000, windowsHide: true });
      const ownership = JSON.parse(verified.stdout);
      if (ownership.status !== 'OWNED_CDP_VERIFIED' || new URL(ownership.control).origin !== this.control.origin) return { status: 'UNAVAILABLE', code: 'DEDICATED_CDP_UNPROVEN' };
      const response = await fetch(new URL('/json/version', this.control), { signal: AbortSignal.timeout(5000) });
      if (!response.ok) return { status: 'UNAVAILABLE', code: 'CDP_NOT_AVAILABLE' };
      const version = await response.json();
      const websocket = new URL(version.webSocketDebuggerUrl);
      if (!websocket.pathname.startsWith('/devtools/browser/')) return { status: 'UNAVAILABLE', code: 'CDP_ENDPOINT_INVALID' };
      websocket.hostname = this.control.hostname;
      websocket.port = this.control.port;
      browser = await puppeteer.connect({ browserWSEndpoint: websocket.href, defaultViewport: null, protocolTimeout: 15000 });
      return await operation(browser);
    } catch {
      return { status: 'UNAVAILABLE', code: 'CDP_OPERATION_FAILED' };
    } finally {
      // Never launch/close/kill Chrome. A client disconnect leaves the OS-owned
      // browser, page, profile and session intact.
      if (browser) await browser.disconnect();
    }
  }

  async health() {
    return this._use(async browser => ({ status: 'AVAILABLE', browser: await browser.version(), substrate: 'OFFICIAL_NON_BRANDED_CHROMIUM', browserLifecycleOwner: 'EXECUTOR_NATIVE_ENTRY', persistentProfile: true }));
  }

  async binding() {
    return this._use(async browser => {
      const pages = await browser.pages();
      if (pages.length !== 1) return { status: 'AMBIGUOUS_PAGE' };
      const state = await pages[0].evaluate(snapshotDom);
      if (state.auth) return { status: 'AUTH_REQUIRED' };
      const conversationId = exactId(state.url);
      return conversationId ? { status: 'EXACT_CONVERSATION', conversationId, title: await pages[0].title() }
        : { status: 'SELECT_CONVERSATION_IN_BROWSER' };
    });
  }

  async verify(conversationId) {
    if (!ID.test(conversationId ?? '')) return { status: 'INVALID_CONVERSATION_ID' };
    return this._use(async browser => {
      const selected = await this._page(browser, conversationId, true);
      if (!selected.page) return selected;
      const state = await selected.page.evaluate(snapshotDom);
      if (state.auth) return { status: 'AUTH_REQUIRED' };
      return exactId(state.url) === conversationId ? { status: 'EXACT_CONVERSATION', conversationId }
        : { status: 'DESTINATION_MISMATCH' };
    });
  }

  async _page(browser, conversationId, navigate) {
    const pages = await browser.pages();
    const matches = pages.filter(page => exactId(page.url()) === conversationId);
    if (matches.length > 1) return { status: 'AMBIGUOUS_PAGE' };
    if (matches.length === 1) return { page: matches[0] };
    if (!navigate || pages.length !== 1) return { status: 'DESTINATION_MISMATCH' };
    const page = pages[0];
    const current = page.url();
    const before = await page.evaluate(snapshotDom);
    if (before.auth) return { status: 'AUTH_REQUIRED' };
    if ((before.composerText ?? '').trim() || before.composerFiles) return { status: 'HUMAN_DRAFT' };
    if (current !== 'about:blank' && new URL(current).origin !== ORIGIN) return { status: 'DESTINATION_MISMATCH' };
    await page.goto(`${ORIGIN}/c/${conversationId}`, { waitUntil: 'domcontentloaded', timeout: 20000 });
    const after = await page.evaluate(snapshotDom);
    if (after.auth) return { status: 'AUTH_REQUIRED' };
    return exactId(after.url) === conversationId ? { page } : { status: 'DESTINATION_MISMATCH' };
  }

  async observe(conversationId, navigate = true) {
    if (!ID.test(conversationId ?? '')) return { status: 'INVALID_CONVERSATION_ID' };
    return this._use(async browser => {
      const selection = await this._page(browser, conversationId, navigate);
      if (!selection.page) return selection;
      const first = await selection.page.evaluate(snapshotDom);
      if (first.auth) return { status: 'AUTH_REQUIRED' };
      const reply = first.messages.filter(message => message.role === 'assistant').at(-1);
      if (exactId(first.url) !== conversationId || first.streaming || !reply?.text || !(reply.id || reply.turnId)) return { status: 'INCOMPLETE' };
      await delay(300);
      const second = await selection.page.evaluate(snapshotDom);
      const confirmed = second.messages.filter(message => message.role === 'assistant').at(-1);
      if (second.auth) return { status: 'AUTH_REQUIRED' };
      if (exactId(second.url) !== conversationId || second.streaming || confirmed?.id !== reply.id || confirmed?.turnId !== reply.turnId || confirmed?.text !== reply.text) return { status: 'INCOMPLETE' };
      const replyIndex = second.messages.findLastIndex(message => message.role === 'assistant');
      const preceding = second.messages[replyIndex - 1];
      return { status: 'OBSERVED', conversationId, messageId: reply.id ?? null, turnId: reply.turnId ?? null, identitySource: reply.identitySource, text: reply.text, textSha256: sha256(reply.text), precedingUserMessageId: preceding?.role === 'user' ? preceding.id : null };
    });
  }

  _receiptPath(dispatchId) { return path.join(this.receiptDirectory, `${sha256(dispatchId)}.json`); }

  async _read(dispatchId) {
    try {
      const row = JSON.parse(await readFile(this._receiptPath(dispatchId), 'utf8'));
      if (row.dispatchId !== dispatchId) throw new Error('Durable receipt identity mismatch');
      return row;
    }
    catch (error) { if (error.code === 'ENOENT') return null; throw error; }
  }

  async _store(row, reserve = false) {
    await mkdir(this.receiptDirectory, { recursive: true });
    const destination = this._receiptPath(row.dispatchId);
    const temporary = reserve ? destination : `${destination}.${process.pid}.${Date.now()}.tmp`;
    const file = await open(temporary, 'wx', 0o600);
    try { await file.writeFile(JSON.stringify(row)); await file.sync(); } finally { await file.close(); }
    if (!reserve) await rename(temporary, destination);
  }

  async _accepted(page, row) {
    const expectedFilenames = row.nativeUploadEvidence?.files.map(item => item.canonicalFilename)
      ?? (row.uploadedAttachments ?? []).map(item => item.filename);
    const snapshot = await page.evaluate(snapshotDom, { beforeUserMessageIds: row.beforeUserMessageIds, expectedFilenames });
    if (snapshot.auth) return { ...row, authRequired: true };
    if (exactId(snapshot.url) !== row.conversationId) return row;
    const accepted = snapshot.messages.filter(message => message.role === 'user' && message.id && !row.beforeUserMessageIds.includes(message.id)
      && (!row.nativeUploadEvidence || message.id === row.nativeUploadEvidence.userMessageId)
      && sha256(message.text) === row.payloadSha256 && message.fileNames.length === expectedFilenames.length);
    if (accepted.length !== 1) return row;
    const result = { ...row, status: 'SENT', acceptedMessageId: accepted[0].id, acceptedAt: new Date().toISOString() };
    await this._store(result);
    return result;
  }

  async send(conversationId, dispatchId, immutablePayload, payloadSha256, attachments = []) {
    if (!ID.test(conversationId ?? '') || typeof dispatchId !== 'string' || !dispatchId || typeof immutablePayload !== 'string') return { status: 'INVALID_REQUEST' };
    if (!/^[a-f0-9]{64}$/i.test(payloadSha256 ?? '') || sha256(immutablePayload) !== payloadSha256.toLowerCase()) return { status: 'HASH_MISMATCH' };
    payloadSha256 = payloadSha256.toLowerCase();
    if (!Array.isArray(attachments) || attachments.length > 20 || attachments.some(item =>
      !item || !path.isAbsolute(item.path ?? '') || !/^[a-f0-9]{64}$/i.test(item.sha256 ?? ''))) return { status: 'INVALID_ATTACHMENTS' };
    if (new Set(attachments.map(item => path.basename(item.path))).size !== attachments.length) return { status: 'AMBIGUOUS_ATTACHMENT_NAMES' };
    const attachmentsSha256 = sha256(JSON.stringify(attachments.map(item => ({ path: item.path, sha256: item.sha256.toLowerCase() }))));
    const retained = await this._read(dispatchId);
    if (retained) return retained.conversationId === conversationId && retained.payloadSha256 === payloadSha256 &&
      (retained.attachmentsSha256 === attachmentsSha256 || (!retained.attachmentsSha256 && !attachments.length)) ? retained : { status: 'DISPATCH_CONFLICT' };
    // The real Web path normalizes outer whitespace. Never rewrite an approved
    // payload or make an unprovable submit; reject before any browser operation.
    if (immutablePayload !== immutablePayload.trim()) return { status: 'UNAVAILABLE', code: 'IMMUTABLE_OUTER_WHITESPACE_UNSUPPORTED' };
    for (const item of attachments) {
      const file = await stat(item.path).catch(() => null);
      if (!file?.isFile() || await fileSha256(item.path) !== item.sha256.toLowerCase()) return { status: 'ATTACHMENT_CHANGED_BEFORE_UPLOAD' };
    }
    let row;
    const result = await this._use(async browser => {
      const selection = await this._page(browser, conversationId, true);
      if (!selection.page) return selection;
      const page = selection.page;
      const before = await page.evaluate(snapshotDom);
      if (before.auth) return { status: 'AUTH_REQUIRED' };
      if (exactId(before.url) !== conversationId) return { status: 'DESTINATION_MISMATCH' };
      if (before.streaming || !before.composerPresent) return { status: 'UNAVAILABLE' };
      if ((before.composerText !== '' && before.composerText !== '\n') || before.composerFiles) return { status: 'HUMAN_DRAFT' };
      // Standard CDP focus keeps this selected renderer active while its
      // dedicated native window is occluded. No OS focus or daily control.
      await page.emulateFocusedPage(true);
      const selectedFiles = await Promise.all(attachments.map(async item => ({ filename: path.basename(item.path), size: (await stat(item.path)).size, sha256: item.sha256.toLowerCase() })));
      const nativeEvidence = nativeUploadEvidence(conversationId, immutablePayload, selectedFiles);
      const onRequest = request => {
        if (request.method() !== 'POST') return;
        const url = new URL(request.url());
        if (url.origin !== ORIGIN || !['/backend-api/files/process_upload_stream', '/backend-api/f/conversation', '/backend-api/conversation'].includes(url.pathname)) return;
        try { nativeEvidence.accept(request.url(), JSON.parse(request.postData() ?? 'null')); } catch { /* Unproved native schema remains UNKNOWN. */ }
      };
      if (attachments.length) page.on('request', onRequest);
      try {
        // Persist fail-closed intent before the first composer write. A crash can
        // leave UNKNOWN, never an automatically resendable unrecorded dispatch.
        row = { status: 'UNKNOWN', dispatchId, conversationId, payloadSha256, attachmentsSha256, beforeAssistantMessageId: before.messages.findLast(message => message.role === 'assistant')?.id ?? null, beforeUserMessageIds: before.messages.filter(message => message.role === 'user' && message.id).map(message => message.id), createdAt: new Date().toISOString(), submissionIntentCount: 0, automaticResend: false };
        try { await this._store(row, true); }
        catch (error) {
          if (error.code !== 'EEXIST') throw error;
          const concurrent = await this._read(dispatchId);
          return concurrent.conversationId === conversationId && concurrent.payloadSha256 === payloadSha256 && concurrent.attachmentsSha256 === attachmentsSha256 ? concurrent : { status: 'DISPATCH_CONFLICT' };
        }
        if (attachments.length) {
          const inputs = await page.$$('input[type="file"]:not([accept])');
          if (inputs.length !== 1) return row;
          // Rehash the entire exact selected set immediately before this one
          // browser import. Upload loss is UNKNOWN, never another assignment.
          for (const item of attachments) {
            if (await fileSha256(item.path) !== item.sha256.toLowerCase()) return row;
          }
          await inputs[0].uploadFile(...attachments.map(item => item.path));
          let assigned = false;
          for (let attempt = 0; attempt < 120; attempt++) {
            if ((await page.evaluate(snapshotDom)).auth) return { ...row, authRequired: true };
            assigned = await page.evaluate(names => {
            const composer = document.querySelector('#prompt-textarea, main [role="textbox"][contenteditable="true"][data-composer-markdown]');
            const root = composer?.closest('[data-composer-root], [class*="ComposerLayoutRoot-"]') ?? composer?.closest('form');
            return root && names.every(name => [...root.querySelectorAll('*')].some(element =>
              element.getClientRects().length && element.textContent?.trim() === name));
            }, attachments.map(item => path.basename(item.path)));
            if (assigned) break;
            await delay(250);
          }
          if (!assigned) return row;
          row.uploadedAttachments = attachments.map(item => ({ filename: path.basename(item.path), sha256: item.sha256.toLowerCase() }));
          await this._store(row);
        }
        // CDP insertText preserves newlines without synthesizing Enter (which a
        // provider composer may treat as submit). No locator action retry.
        const composer = await page.$(COMPOSER);
        if (!composer) return row;
        await composer.focus();
        await page.keyboard.sendCharacter(immutablePayload);
        let ready = await page.evaluate(snapshotDom);
        if (attachments.length) {
          for (let attempt = 0; !ready.sendReady && !ready.auth && attempt < 120; attempt++) {
            await delay(250);
            ready = await page.evaluate(snapshotDom);
          }
        }
        if (ready.auth) return { ...row, authRequired: true };
        if (exactId(ready.url) !== conversationId || ready.composerText !== immutablePayload || !ready.sendReady) return row;
        row.submissionIntentCount = 1;
        await this._store(row);
        const button = await page.$(SEND);
        if (!button) return row;
        // ElementHandle.click/locator checks can wait on animation frames in a
        // hidden dedicated window. Synchronous DOM scroll + mature CDP mouse
        // input avoids an implicit action retry; recheck the hit target first.
        await button.evaluate(element => element.scrollIntoView({ block: 'center', inline: 'center', behavior: 'instant' }));
        const box = await button.boundingBox();
        if (!box || box.width <= 0 || box.height <= 0) return row;
        const point = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
        const hit = await button.evaluate((element, point) => {
          const target = document.elementFromPoint(point.x, point.y);
          return !element.disabled && (target === element || element.contains(target));
        }, point);
        if (!hit) return row;
        const final = await page.evaluate(snapshotDom);
        if (final.auth) return { ...row, authRequired: true };
        if (final.streaming || exactId(final.url) !== conversationId || final.composerText !== immutablePayload || !final.sendReady) return row;
        await page.mouse.click(point.x, point.y, { clickCount: 1 });
        for (let attempt = 0; attempt < 40; attempt++) {
          if (nativeEvidence.evidence && !row.nativeUploadEvidence) {
            row = { ...row, nativeUploadEvidence: nativeEvidence.evidence };
            await this._store(row);
          }
          const reconciled = await this._accepted(page, row);
          if (reconciled.status === 'SENT' || reconciled.authRequired) return reconciled;
          await delay(250);
        }
        return row;
      } finally { if (attachments.length) page.off('request', onRequest); await page.emulateFocusedPage(false); }
    });
    return row && result.status === 'UNAVAILABLE' ? row : result;
  }

  async receipt(dispatchId) {
    if (typeof dispatchId !== 'string' || !dispatchId) return { status: 'INVALID_REQUEST' };
    const row = await this._read(dispatchId);
    if (!row) return { status: 'NOT_FOUND' };
    if (row.status === 'SENT' || row.submissionIntentCount !== 1) return row;
    const observed = await this._use(async browser => {
      const selection = await this._page(browser, row.conversationId, false);
      return selection.page ? this._accepted(selection.page, row) : row;
    });
    return observed.status === 'UNAVAILABLE' ? row : observed;
  }
}
