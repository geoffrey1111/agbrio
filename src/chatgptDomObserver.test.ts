import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
const { JSDOM } = createRequire(process.cwd() + '/package.json')('jsdom');
import { describe, expect, it } from 'vitest';

const script = readFileSync('src-tauri/src/chatgpt_dom_observer.js', 'utf8');
function observe(html: string, path = '/c/exact-conversation') {
  const dom = new JSDOM(html, { url: `https://chatgpt.com${path}`, runScripts: 'outside-only' });
  Object.assign(dom.window, { TextEncoder, TextDecoder });
  dom.window.eval(script);
  const snapshot = dom.window.__aiwrChatGptObserver.capture();
  dom.window.close();
  return snapshot;
}
describe('progressive provider DOM observations', () => {
  it('suppresses unchanged mutation snapshots while preserving forced and changed-turn emissions', async () => {
    const dom = new JSDOM('<main><section data-turn="assistant" data-turn-id="reply"><div class="markdown">Initial fixture</div></section></main>', {
      url: 'https://chatgpt.com/c/exact-conversation', runScripts: 'outside-only',
    });
    const messages: Array<{ turns: Array<{ text: string }> }> = [];
    const scheduled: Array<() => void> = [];
    Object.assign(dom.window, {
      TextEncoder, TextDecoder,
      ipc: { postMessage: (message: string) => messages.push(JSON.parse(message)) },
      setTimeout: (callback: () => void) => { scheduled.push(callback); return scheduled.length; },
    });
    const flushMutations = async () => {
      // Deliver the actual jsdom MutationObserver microtask, then its bounded debounce.
      await Promise.resolve();
      scheduled.splice(0).forEach((callback) => callback());
      await Promise.resolve();
    };
    try {
      dom.window.eval(script);
      await flushMutations();
      expect(messages).toHaveLength(1);
      const unrelated = dom.window.document.createElement('span');
      unrelated.hidden = true;
      dom.window.document.body.appendChild(unrelated);
      await flushMutations();
      unrelated.remove();
      await flushMutations();
      expect(messages).toHaveLength(1);
      dom.window.__aiwrChatGptObserver.emit(true);
      expect(messages).toHaveLength(2);
      expect(messages[1]).toEqual(messages[0]);
      dom.window.document.querySelector('.markdown').textContent = 'Changed actual turn';
      await flushMutations();
      expect(messages).toHaveLength(3);
      expect(messages[2].turns[0].text).toBe('Changed actual turn');
      // Receiver reattachment may explicitly request the same state again.
      dom.window.eval(script);
      expect(messages).toHaveLength(4);
      expect(messages[3]).toEqual(messages[2]);
    } finally {
      dom.window.close();
    }
  });

  it('reads only dirty turn text and drains pending mutations during synchronous capture', () => {
    const dom = new JSDOM('<aside id="sidebar"></aside><main><div id="wrapper"><section data-turn="assistant" data-turn-id="old"><div class="markdown" id="old">old</div></section><section data-turn="assistant" data-turn-id="latest"><div class="markdown" id="latest">initial</div></section></div></main>', { url: 'https://chatgpt.com/c/exact-conversation', runScripts: 'outside-only' });
    let oldReads = 0, latestReads = 0;
    const old = dom.window.document.querySelector('#old');
    const latest = dom.window.document.querySelector('#latest');
    Object.defineProperty(old, 'innerText', { get: () => { oldReads++; return 'large old fixture'.repeat(2000); } });
    Object.defineProperty(latest, 'innerText', { get: () => { latestReads++; return latest.textContent; } });
    Object.assign(dom.window, { TextEncoder, TextDecoder });
    try {
      dom.window.eval(script);
      const observer = dom.window.__aiwrChatGptObserver;
      observer.capture();
      expect([oldReads, latestReads]).toEqual([1, 1]);
      latest.textContent = 'streamed update';
      expect(observer.capture().turns[1].text).toBe('streamed update');
      expect([oldReads, latestReads]).toEqual([1, 2]);
      dom.window.document.querySelector('#sidebar').textContent = 'unrelated change';
      observer.capture();
      expect([oldReads, latestReads]).toEqual([1, 2]);
      dom.window.document.querySelector('#wrapper').setAttribute('hidden', '');
      observer.capture();
      expect([oldReads, latestReads]).toEqual([2, 3]);
      dom.window.history.replaceState({}, '', '/c/other-conversation');
      observer.capture();
      expect([oldReads, latestReads]).toEqual([3, 4]);
      dom.window.document.querySelector('main').outerHTML = '<main><section data-turn="user" data-turn-id="replacement">new main</section></main>';
      expect(observer.capture().turns[0].text).toBe('new main');
    } finally { dom.window.close(); }
  });
  it('reads current controls and terminal state without rereading cached text', () => {
    const dom = new JSDOM('<main><section data-turn="assistant" data-turn-id="reply"><div class="markdown">fixture</div><button data-testid="copy-turn-action-button"></button></section></main><textarea id="prompt-textarea"></textarea><button data-testid="send-button"></button>', { url: 'https://chatgpt.com/c/exact-conversation', runScripts: 'outside-only' });
    Object.assign(dom.window, { TextEncoder, TextDecoder });
    try {
      dom.window.eval(script);
      const observer = dom.window.__aiwrChatGptObserver;
      expect(observer.capture().turns[0].terminal).toBe(true);
      const stop = dom.window.document.createElement('button'); stop.dataset.testid = 'stop-button'; dom.window.document.body.appendChild(stop);
      const send = dom.window.document.querySelector('[data-testid="send-button"]'); send.disabled = true;
      dom.window.document.querySelector('textarea').value = 'typed';
      const result = observer.capture();
      expect(result.generationActive).toBe(true);
      expect(result.turns[0].terminal).toBe(false);
      expect(result.composer).toMatchObject({ textLength: 5, sendDisabled: true });
      stop.remove();
      expect(observer.capture().turns[0].terminal).toBe(true);
    } finally { dom.window.close(); }
  });
  it('includes newly materialized older turns alongside the latest streaming reply under the cap', () => {
    const html = Array.from({ length: 205 }, (_, index) => `<section data-turn="assistant" data-turn-id="turn-${index}"><div class="markdown">${index}</div></section>`).join('');
    const dom = new JSDOM(`<main>${html}</main>`, { url: 'https://chatgpt.com/c/exact-conversation', runScripts: 'outside-only' });
    Object.assign(dom.window, { TextEncoder, TextDecoder });
    try {
      dom.window.eval(script);
      const observer = dom.window.__aiwrChatGptObserver;
      observer.capture();
      dom.window.document.querySelector('main').insertAdjacentHTML('afterbegin', '<section data-turn="user" data-turn-id="older-a">older A</section><section data-turn="assistant" data-turn-id="older-b">older B</section>');
      const latest = dom.window.document.querySelector('[data-turn-id="turn-204"]');
      latest.setAttribute('data-is-streaming', 'true');
      latest.querySelector('.markdown').textContent = 'latest changed';
      const result = observer.capture();
      expect(result.turns.length).toBeLessThanOrEqual(200);
      expect(result.turns.slice(0, 2).map((turn: { id: string }) => turn.id)).toEqual(['older-a', 'older-b']);
      expect(result.turns[result.turns.length - 1]).toMatchObject({ id: 'turn-204', text: 'latest changed', streaming: true });
      const middle = dom.window.document.querySelector('[data-turn-id="turn-2"] .markdown');
      middle.textContent = 'newly changed older turn';
      expect(observer.capture().turns.some((turn: { id: string; text: string }) => turn.id === 'turn-2' && turn.text === 'newly changed older turn')).toBe(true);
    } finally { dom.window.close(); }
  });

  it('bounds extraction to latest 200 materialized turns including current assistant', () => {
    const html = Array.from({ length: 205 }, (_, index) => `<section data-turn="assistant" data-turn-id="turn-${index}"><div class="markdown">${index}</div></section>`).join('');
    const result = observe(`<main>${html}</main>`);
    expect(result.turns).toHaveLength(200);
    expect(result.turns[0].id).toBe('turn-5');
    expect(result.turns[199].id).toBe('turn-204');
  });

  it('retains multiple assistant blocks without nested markdown duplication', () => {
    const result = observe('<main><section data-turn="assistant" data-turn-id="multi"><div class="markdown">first <span class="markdown">nested</span></div><div class="markdown">second</div></section></main>');
    expect(result.turns[0].text).toBe('first nested\n\nsecond');
  });
  it('recognizes nested user/assistant containers once using stable provider IDs', () => {
    const result = observe(`<main>
      <section data-turn="user" data-turn-id="user-1" data-testid="conversation-turn-0"><div data-message-author-role="user" data-message-id="message-1"><div class="whitespace-pre-wrap">fixture prompt</div><button>Edit</button></div></section>
      <article data-testid="conversation-turn-1"><div data-message-author-role="assistant" data-message-id="assistant-1"><div class="markdown">fixture reply</div></div><button data-testid="copy-turn-action-button">Copy</button></article>
    </main>`);
    expect(result.turns).toEqual([
      { id: 'user-1', role: 'user', text: 'fixture prompt', streaming: false, terminal: false },
      { id: 'assistant-1', role: 'assistant', text: 'fixture reply', streaming: false, terminal: true },
    ]);
  });
  it('distinguishes streaming, terminal actions, and mere absence of generation', () => {
    const result = observe(`<main><section data-turn="assistant" data-turn-id="a" data-is-streaming="true"><div class="markdown">partial</div></section><section data-turn="assistant" data-turn-id="b">unknown state</section></main>`);
    expect(result.turns.map((t: { streaming: boolean; terminal: boolean }) => [t.streaming, t.terminal])).toEqual([[true, false], [false, false]]);
  });
  it('excludes Project rows from history while preserving scoped row projection', () => {
    const result = observe(`<main><section data-turn="user" data-turn-id="stale">stale</section><a aria-label="Fixture row" href="/g/project-123/c/exact-conversation">Fixture row</a></main>`, '/g/project-123/project');
    expect(result.turns).toEqual([]);
    expect(result.projectRows).toHaveLength(1);
  });
  it('bounds UTF-8 text, ignores sidebar messages, and deduplicates stable IDs', () => {
    const result = observe(`<aside data-message-author-role="user">sidebar</aside><main><section data-turn="user" data-turn-id="a">${'界'.repeat(20000)}</section><section data-turn="user" data-turn-id="a">remounted copy</section></main>`);
    expect(result.turns).toHaveLength(1);
    expect(new TextEncoder().encode(result.turns[0].text).length).toBeLessThanOrEqual(32768);
  });
});
