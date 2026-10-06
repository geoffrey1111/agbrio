// Exact terminal-result resources only. Selection is an opaque native-control
// identity; a filename is presentation, never a guessed download target.
import { randomUUID } from 'node:crypto';
import { mkdir, stat, rename } from 'node:fs/promises';
import path from 'node:path';
import { exactId, snapshotDom, sha256 } from './executor.mjs';
import { fileSha256, rejectLinkedAncestors } from './native-runtime.mjs';
const MAX_BYTES = 20 * 1024 * 1024;

export function resourcesDom(messageId) {
  const legacy = [...document.querySelectorAll('[data-message-author-role="assistant"]')];
  const messages = legacy.length ? legacy : [...document.querySelectorAll('[data-chatgpt-search-message-ids]')]
    .filter(node => node.querySelector('[data-markdown-text-style="assistant-message"]'));
  const exact = messages.filter(node => {
    const id = node.getAttribute('data-message-id') ?? node.closest('[data-message-id]')?.getAttribute('data-message-id');
    const native = [...new Set((node.getAttribute('data-chatgpt-search-message-ids') ?? '').trim().split(/\s+/).filter(Boolean))];
    return id === messageId || (native.length === 1 && native[0] === messageId);
  });
  if (exact.length !== 1) return [];
  const validName = filename => filename && filename.length <= 255 && !/[\\/:\x00-\x1f]/.test(filename) && filename !== '.' && filename !== '..';
  const anchors = [...exact[0].querySelectorAll('a[href]')].flatMap(anchor => {
    const href = anchor.getAttribute('href');
    // Generated sandbox artifacts only; ordinary external references excluded.
    if (!href?.startsWith('sandbox:/mnt/data/')) return [];
    const nativePath = href.slice('sandbox:'.length);
    const filename = anchor.getAttribute('download') || decodeURIComponent(nativePath.split('/').at(-1));
    if (!validName(filename)) return [];
    return [{ nativeKey: href, filename, control: anchor }];
  });
  // Current provider file cards expose a native download button rather than a
  // sandbox link. Filename is presentation; selection identity is the button's
  // CDP backend node in its exact message/document, resolved below.
  const isDownload = button => /^(下载文件|Download file)$/.test(button.getAttribute('aria-label') ?? '');
  const cards = [...exact[0].querySelectorAll('button')].filter(isDownload).flatMap(button => {
    let card = button.parentElement;
    for (let depth = 0; card && card !== exact[0] && depth < 12; depth++, card = card.parentElement) {
      const previews = [...card.querySelectorAll('button[aria-label]')].map(node =>
        node.getAttribute('aria-label').match(/^打开 (.+) 的预览$/)?.[1]).filter(Boolean);
      const downloads = [...card.querySelectorAll('button')].filter(isDownload);
      if (previews.length === 1 && downloads.length === 1 && validName(previews[0])) {
        return [{ nativeKey: null, filename: previews[0], control: button }];
      }
    }
    return [];
  });
  return [...anchors, ...cards];
}

export class TerminalAttachments {
  constructor(executor, directory) { this.executor = executor; this.directory = directory; }
  async _resources(page, messageId) {
    const capture = await page.evaluateHandle(resourcesDom, messageId);
    const records = [];
    let session;
    try {
      const handles = await capture.getProperties();
      let documentKey;
      try {
        for (const handle of handles.values()) {
          const info = await handle.evaluate(value => ({ filename: value.filename, nativeKey: value.nativeKey }));
          const controlHandle = await handle.getProperty('control');
          const control = controlHandle.asElement();
          if (!control) { await controlHandle.dispose(); throw new Error('NATIVE_DOWNLOAD_CONTROL_REQUIRED'); }
          try {
            let nativeKey = info.nativeKey;
            if (!nativeKey) {
              if (!documentKey) {
                session = await page.createCDPSession();
                const { frameTree } = await session.send('Page.getFrameTree');
                if (!frameTree.frame.id || !frameTree.frame.loaderId) throw new Error('DOWNLOAD_DOCUMENT_UNPROVEN');
                documentKey = `${frameTree.frame.id}:${frameTree.frame.loaderId}`;
              }
              nativeKey = `button:${documentKey}:${await control.backendNodeId()}`;
            }
            const resourceId = sha256(`${messageId}\0${nativeKey}`);
            if (records.some(record => record.resourceId === resourceId)) throw new Error('AMBIGUOUS_RESOURCE');
            records.push({ resourceId, filename: info.filename, control });
          } catch (error) { await control.dispose(); throw error; }
        }
      } finally { await Promise.all([...handles.values()].map(handle => handle.dispose())); }
      return records;
    } catch (error) {
      await Promise.all(records.map(record => record.control.dispose()));
      throw error;
    } finally { await capture.dispose(); await session?.detach(); }
  }
  async _selectedPage(browser, conversationId, messageId) {
    if ((await browser.pages()).length !== 1) return { status: 'SINGLE_OWNED_PAGE_REQUIRED' };
    const selected = await this.executor._page(browser, conversationId, false);
    if (!selected.page) return selected;
    const state = await selected.page.evaluate(snapshotDom);
    if (state.auth) return { status: 'AUTH_REQUIRED' };
    if (exactId(state.url) !== conversationId || state.streaming || state.messages.findLast(message => message.role === 'assistant')?.id !== messageId) return { status: 'TERMINAL_SOURCE_CHANGED' };
    return selected;
  }
  async list(conversationId, messageId) {
    return this.executor._use(async browser => {
      const selected = await this._selectedPage(browser, conversationId, messageId);
      if (!selected.page) return selected;
      const records = await this._resources(selected.page, messageId);
      try { return { status: 'RESOURCES', conversationId, messageId, resources: records.map(({ resourceId, filename }) => ({ resourceId, filename })) }; }
      finally { await Promise.all(records.map(record => record.control.dispose())); }
    });
  }
  async materialize(conversationId, messageId, resourceId) {
    if (!/^[a-f0-9]{64}$/.test(resourceId ?? '')) return { status: 'INVALID_RESOURCE_SELECTION' };
    return this.executor._use(async browser => {
      const selected = await this._selectedPage(browser, conversationId, messageId);
      if (!selected.page) return selected;
      const page = selected.page;
      const resources = await this._resources(page, messageId);
      try {
        const matches = resources.filter(resource => resource.resourceId === resourceId);
        if (matches.length !== 1) return { status: 'EXACT_RESOURCE_REQUIRED' };
        await rejectLinkedAncestors(this.directory);
        const capture = path.join(this.directory, randomUUID());
        await mkdir(capture, { recursive: true });
        const browserSession = await browser.target().createCDPSession();
        const pageSession = await page.createCDPSession();
        let timer;
        const activeDownloads = new Set();
        let captureComplete = false;
        try {
          await page.emulateFocusedPage(true);
          const { frameTree } = await pageSession.send('Page.getFrameTree');
          let begun;
          let ambiguous = false;
          const completion = new Promise((resolve, reject) => {
            browserSession.on('Browser.downloadWillBegin', event => {
              activeDownloads.add(event.guid);
              if (event.frameId !== frameTree.frame.id || begun) { ambiguous = true; reject(new Error('DOWNLOAD_AMBIGUOUS')); return; }
              if (event.suggestedFilename !== matches[0].filename) { reject(new Error('DOWNLOAD_SELECTION_MISMATCH')); return; }
              begun = event;
            });
            browserSession.on('Browser.downloadProgress', event => {
              if (event.guid !== begun?.guid || ambiguous) return;
              if (event.totalBytes > MAX_BYTES || event.receivedBytes > MAX_BYTES || event.state === 'canceled') reject(new Error('DOWNLOAD_FAILED'));
              else if (event.state === 'completed') resolve(event);
            });
            timer = setTimeout(() => reject(new Error('DOWNLOAD_UNOBSERVED_NO_RETRY')), 30000);
          });
          // Handle rejection immediately even when a click/CDP operation fails.
          completion.catch(() => {});
          await browserSession.send('Browser.setDownloadBehavior', { behavior: 'allowAndName', downloadPath: capture, eventsEnabled: true });
          const element = matches[0].control;
          if (!element) return { status: 'EXACT_RESOURCE_REQUIRED' };
          // One exact file-control click, no fallback/retry or arbitrary URL fetch.
          // Preserve an already-visible native control. Scrolling it anyway
          // can trigger the provider's conversation anchoring and move the
          // hit target between hover and the one permitted click.
          await element.evaluate(node => {
            const r = node.getBoundingClientRect();
            if (r.top < 0 || r.left < 0 || r.bottom > innerHeight || r.right > innerWidth) {
              node.scrollIntoView({ block: 'center', behavior: 'instant' });
            }
          });
          const box = await element.boundingBox();
          if (!box || box.width <= 0 || box.height <= 0) return { status: 'DOWNLOAD_CONTROL_UNAVAILABLE' };
          const point = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
          // Provider cards enable their download button on hover. Move only
          // within the selected renderer, then require that exact native control
          // to receive the click; never click the preview overlay or retry it.
          await page.mouse.move(point.x, point.y);
          try {
            await page.waitForFunction((node, point) => {
              const hit = document.elementFromPoint(point.x, point.y);
              return hit === node || node.contains(hit);
            }, { timeout: 1000, polling: 50 }, element, point);
          } catch { return { status: 'DOWNLOAD_CONTROL_UNAVAILABLE' }; }
          if (!await element.evaluate((node, point) => {
            const hit = document.elementFromPoint(point.x, point.y);
            return hit === node || node.contains(hit);
          }, point)) return { status: 'DOWNLOAD_CONTROL_UNAVAILABLE' };
          if ((await page.evaluate(snapshotDom)).auth) return { status: 'AUTH_REQUIRED' };
          await page.mouse.click(point.x, point.y, { clickCount: 1 });
          await completion;
          const staged = path.join(capture, begun.guid);
          const metadata = await stat(staged);
          if (!metadata.isFile() || !metadata.size || metadata.size > MAX_BYTES) return { status: 'INVALID_DOWNLOAD' };
          const destination = path.join(capture, matches[0].filename);
          await rename(staged, destination);
          captureComplete = true;
          return { status: 'MATERIALIZED', conversationId, messageId, resourceId,
            filename: matches[0].filename, path: destination, size: metadata.size,
            sha256: await fileSha256(destination) };
        } finally {
          clearTimeout(timer);
          if (!captureComplete) {
            for (const guid of activeDownloads) await browserSession.send('Browser.cancelDownload', { guid }).catch(() => {});
          }
          await browserSession.send('Browser.setDownloadBehavior', { behavior: 'deny', eventsEnabled: false }).catch(() => {});
          await pageSession.detach().catch(() => {});
          await browserSession.detach().catch(() => {});
          await page.emulateFocusedPage(false).catch(() => {});
        }
      } finally { await Promise.all(resources.map(resource => resource.control.dispose())); }
    });
  }
}
