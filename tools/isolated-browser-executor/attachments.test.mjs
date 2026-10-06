import test from 'node:test';
import assert from 'node:assert/strict';
import { JSDOM } from 'jsdom';
import { mkdtemp, mkdir, readFile, rm } from 'node:fs/promises';
import path from 'node:path';
import puppeteer from 'puppeteer-core';
import { resourcesDom, TerminalAttachments } from './attachments.mjs';
import { BrowserExecutor, sha256 } from './executor.mjs';
import { fileSha256 } from './native-runtime.mjs';

const id = '00000000-0000-4000-8000-000000000001';
const other = '00000000-0000-4000-8000-000000000002';
const card = name => `<span class="file-card"><button aria-label="打开 ${name} 的预览"></button><span><span><button aria-label="下载文件"></button></span></span></span>`;
function inspect(html) {
  const dom = new JSDOM(html);
  globalThis.document = dom.window.document;
  try { return resourcesDom(id); }
  finally { delete globalThis.document; dom.window.close(); }
}
test('repeated identical native message tokens select its current file card; other messages are excluded', () => {
  const rows = inspect(`<div data-chatgpt-search-message-ids="${other}"><div data-markdown-text-style="assistant-message"></div>${card('unrelated.txt')}</div><div data-chatgpt-search-message-ids="${id} ${id}"><div data-markdown-text-style="assistant-message"></div>${card('selected.txt')}</div>`);
  assert.equal(rows.length, 1);
  assert.equal(rows[0].filename, 'selected.txt');
  assert.equal(rows[0].nativeKey, null);
  assert.equal(rows[0].control.getAttribute('aria-label'), '下载文件');
});
test('distinct native message identities and duplicate result nodes fail closed', () => {
  assert.deepEqual(inspect(`<div data-chatgpt-search-message-ids="${id} ${other}"><div data-markdown-text-style="assistant-message"></div>${card('selected.txt')}</div>`), []);
  const one = `<div data-chatgpt-search-message-ids="${id}"><div data-markdown-text-style="assistant-message"></div>${card('selected.txt')}</div>`;
  assert.deepEqual(inspect(one + one), []);
});
test('same filename does not merge two distinct native controls; unmatched or unsafe previews are excluded', () => {
  const rows = inspect(`<div data-chatgpt-search-message-ids="${id}"><div data-markdown-text-style="assistant-message"></div>${card('same.txt')}${card('same.txt')}${card('../unsafe.txt')}<button aria-label="下载文件"></button></div>`);
  assert.equal(rows.length, 2);
  assert.notEqual(rows[0].control, rows[1].control);
});
test('legacy exact-result sandbox anchors retain their identity; ordinary links are excluded', () => {
  const rows = inspect(`<div data-message-author-role="assistant" data-message-id="${id}"><a href="sandbox:/mnt/data/selected.txt">file</a><a href="https://example.com/unrelated.txt">reference</a></div>`);
  assert.equal(rows.length, 1);
  assert.equal(rows[0].nativeKey, 'sandbox:/mnt/data/selected.txt');
  assert.equal(rows[0].filename, 'selected.txt');
});

test('empty-profile native file controls download exact bytes; stale/reloaded selection cannot click', { skip: !process.env.AIWR_ATTACHMENT_NATIVE_TEST_BINARY }, async t => {
  const binary = process.env.AIWR_ATTACHMENT_NATIVE_TEST_BINARY;
  assert.equal(await fileSha256(binary), 'ac2b50c11c536aff132304f1c9ec33709e43ae7f34357153f7359e52fe35f12c');
  const root = path.resolve(import.meta.dirname, '../../runtime/integrated-provider-router');
  await mkdir(root, { recursive: true });
  const directory = await mkdtemp(path.join(root, 'native-file-card-'));
  const browser = await puppeteer.launch({ executablePath: binary, headless: true, userDataDir: path.join(directory, 'empty-profile') });
  t.after(async () => { await browser.close(); assert.equal(path.dirname(directory), root); await rm(directory, { recursive: true }); });
  const [page] = await browser.pages();
  const bytes = 'public fixture bytes\n';
  const html = `<!doctype html><meta charset="utf-8"><style>body{min-height:2000px}.file-card{display:block;margin-top:600px}.file-card button{width:36px;height:36px}.file-card button[aria-label="下载文件"]{pointer-events:none}.file-card:hover button[aria-label="下载文件"]{pointer-events:auto}</style><div data-chatgpt-search-message-ids="${id} ${id}"><div data-markdown-text-style="assistant-message">fixture terminal</div>${card('selected.txt')}</div><script>window.clicks=0;document.querySelector('button[aria-label="下载文件"]').onclick=event=>{if(!event.isTrusted)throw Error('Native click required');window.clicks++;const a=document.createElement('a');a.href=URL.createObjectURL(new Blob([${JSON.stringify(bytes)}]));a.download='selected.txt';a.click();};</script>`;
  await page.setRequestInterception(true);
  page.on('request', request => { if (request.url() === `https://chatgpt.com/c/${id}`) void request.respond({ status: 200, contentType: 'text/html', body: html }); else void request.abort(); });
  await page.goto(`https://chatgpt.com/c/${id}`, { waitUntil: 'domcontentloaded' });
  await page.evaluate(async () => {
    scrollTo(0, 240);
    await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    window.anchoringEvents = 0;
    // Model a virtualized feed replacing its control when conversation
    // anchoring runs. A visible selected control must remain intact.
    addEventListener('scroll', () => {
      window.anchoringEvents++;
      const button = document.querySelector('button[aria-label="下载文件"]');
      const replacement = button.cloneNode(true);
      replacement.onclick = button.onclick;
      button.replaceWith(replacement);
    }, { once: true });
  });
  const executor = new BrowserExecutor({ connectionProvider: async () => browser });
  const attachments = new TerminalAttachments(executor, path.join(directory, 'materializations'));
  const list = await attachments.list(id, id);
  assert.equal(list.status, 'RESOURCES');
  assert.equal(list.resources.length, 1);
  const selected = list.resources[0].resourceId;
  assert.equal((await attachments.list(id, id)).resources[0].resourceId, selected);
  const downloaded = await attachments.materialize(id, id, selected);
  assert.equal(downloaded.status, 'MATERIALIZED', JSON.stringify({ downloaded, geometry: await page.evaluate(() => { const r = document.querySelector('button[aria-label="下载文件"]').getBoundingClientRect(); return { y:r.y, bottom:r.bottom, height:innerHeight, scrollY, anchoringEvents:window.anchoringEvents }; }) }));
  assert.equal(await readFile(downloaded.path, 'utf8'), bytes);
  assert.equal(downloaded.sha256, sha256(bytes));
  assert.equal(await page.evaluate(() => window.clicks), 1);
  assert.equal(await page.evaluate(() => window.anchoringEvents), 0);
  await page.evaluate(() => { const button = document.querySelector('button[aria-label="下载文件"]'); button.replaceWith(button.cloneNode(true)); });
  assert.notEqual((await attachments.list(id, id)).resources[0].resourceId, selected);
  assert.equal((await attachments.materialize(id, id, selected)).status, 'EXACT_RESOURCE_REQUIRED');
  assert.equal(await page.evaluate(() => window.clicks), 1);
  const current = (await attachments.list(id, id)).resources[0].resourceId;
  await page.reload({ waitUntil: 'domcontentloaded' });
  assert.notEqual((await attachments.list(id, id)).resources[0].resourceId, current);
  assert.equal((await attachments.materialize(id, id, current)).status, 'EXACT_RESOURCE_REQUIRED');
  assert.equal(await page.evaluate(() => window.clicks), 0);
});
