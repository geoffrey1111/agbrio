import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { describe, expect, it } from 'vitest';
const { JSDOM } = createRequire(process.cwd() + '/package.json')('jsdom');
const script = readFileSync('src-tauri/src/chatgpt_dom_submit.js', 'utf8');

async function submit(options: { draft?: string; expected?: string; generation?: boolean; disabled?: boolean; twice?: boolean; navigateDuringInput?: boolean; form?: boolean; externalForm?: boolean; conflictingForms?: boolean; invalidForm?: boolean; formThrows?: boolean; richText?: boolean; payload?: string } = {}) {
  const dom = new JSDOM('<main><textarea id="prompt-textarea"></textarea><button data-testid="send-button">Send</button></main>', { url: 'https://chatgpt.com/c/exact-123', runScripts: 'outside-only' });
  const page = dom.window;
  let forms = 0;
  let keys = 0;
  if (options.form || options.externalForm || options.conflictingForms) {
    const form = page.document.createElement('form'); form.id = 'send-form';
    page.document.querySelector('main').append(form);
    form.append(page.document.querySelector('button'));
    if (options.form) form.append(page.document.querySelector('textarea'));
    if (options.conflictingForms) {
      const other = page.document.createElement('form');
      page.document.querySelector('main').append(other); other.append(page.document.querySelector('textarea'));
    }
    form.checkValidity = () => !options.invalidForm;
    form.requestSubmit = () => { forms += 1; if (options.formThrows) throw new Error('provider route failure'); };
  }
  page.document.addEventListener('keydown', () => { keys += 1; });
  let composer = page.document.querySelector('textarea');
  if (options.richText) {
    const editor = page.document.createElement('div'); editor.id = 'prompt-textarea'; editor.contentEditable = 'true';
    composer.replaceWith(editor); composer = editor;
    Object.defineProperty(editor, 'innerText', { get: () => Array.from(editor.children).map((child: any) => child.textContent).join('\n\n') });
    page.document.execCommand = (_command: string, _ui: boolean, text: string) => {
      editor.replaceChildren(...text.split('\n').map(line => { const p = page.document.createElement('p'); p.textContent = line; return p; })); return true;
    };
  }
  composer.value = options.draft || '';
  const button = page.document.querySelector('button');
  button.disabled = Boolean(options.disabled);
  let clicks = 0;
  const events: Array<{ phase: string; payloadEqual: boolean }> = [];
  button.addEventListener('click', () => { clicks += 1; composer.value = ''; });
  composer.addEventListener('input', () => { if (options.navigateDuringInput) page.history.replaceState({}, '', '/c/wrong-123'); });
  page.ipc = { postMessage: (text: string) => events.push(JSON.parse(text)) };
  page.__aiwrChatGptObserver = { capture: () => ({ href: page.location.href, turns: [], generationActive: Boolean(options.generation), composer: { present: true, sendAvailable: true, sendDisabled: button.disabled } }), emit: () => {} };
  const program = script.replace('__AIWR_SEND_ARGUMENTS__', JSON.stringify({ text: options.payload || 'approved fixture', expectedId: options.expected || 'exact-123', token: 'fixture-nonce' }));
  await page.eval(program);
  if (options.twice) await page.eval(program);
  const draft = composer.value;
  page.close();
  return { clicks, forms, keys, events, draft };
}

describe('one-shot provider submit primitive', () => {
  it('proves prepared payload, emits dispatch once and never retries a second invocation', async () => {
    const result = await submit({ twice: true });
    expect(result.clicks).toBe(1);
    expect(result.events.map(event => event.phase)).toEqual(['PREPARED', 'DISPATCHED', 'REJECTED']);
    expect(result.events[0].payloadEqual).toBe(true);
  });
  it('verifies multiline rich-editor content independently of visual paragraph spacing', async () => {
    const result = await submit({richText:true, payload:'line one\n\nline three', twice:true});
    expect(result.clicks).toBe(1);
    expect(result.events.map(event => event.phase)).toEqual(['PREPARED', 'DISPATCHED', 'REJECTED']);
    expect(result.events[0].payloadEqual).toBe(true);
  });
  it('uses one associated form submission with no click or keyboard fallback', async () => {
    for (const options of [{form:true}, {externalForm:true}, {form:true, formThrows:true}]) {
      const result = await submit({...options, twice:true});
      expect(result.forms).toBe(1);
      expect(result.clicks).toBe(0);
      expect(result.keys).toBe(0);
    }
  });
  it('rejects conflicting or invalid forms before any physical submit', async () => {
    for (const options of [{conflictingForms:true}, {form:true, invalidForm:true}]) {
      const result = await submit(options);
      expect(result.forms + result.clicks + result.keys).toBe(0);
      expect(result.events.map(event => event.phase)).toEqual(['REJECTED']);
    }
  });
  it('preserves a different human draft without dispatch', async () => {
    const result = await submit({ draft: 'human draft' });
    expect(result.clicks).toBe(0);
    expect(result.draft).toBe('human draft');
    expect(result.events.map(event => event.phase)).toEqual(['REJECTED']);
  });
  it('rejects mismatched endpoint, preexisting generation and unavailable send', async () => {
    for (const options of [{ expected: 'wrong-123' }, { generation: true }, { disabled: true }]) {
      const result = await submit(options);
      expect(result.clicks).toBe(0);
      expect(result.events.map(event => event.phase)).toEqual(['REJECTED']);
    }
  });
  it('rechecks exact URL after provider input handlers before dispatch', async () => {
    const result = await submit({ navigateDuringInput: true });
    expect(result.clicks).toBe(0);
    expect(result.events.map(event => event.phase)).toEqual(['REJECTED']);
  });
});
