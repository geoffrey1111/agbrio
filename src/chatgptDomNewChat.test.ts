import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { describe, expect, it } from 'vitest';
const { JSDOM } = createRequire(process.cwd() + '/package.json')('jsdom');
const script = readFileSync('src-tauri/src/chatgpt_dom_new_chat.js', 'utf8');
async function activate(html: string, options: { expected?: string; late?: (window: any) => void } = {}) {
  const dom = new JSDOM(html, { url: 'https://chatgpt.com/g/project-123/project', runScripts: 'outside-only' });
  let clicks = 0, focuses = 0;
  const reports: { activated: boolean; mode?: string }[] = [];
  let deadline: (() => void) | undefined;
  dom.window.setTimeout = (callback: () => void, ms: number) => { expect(ms).toBe(8000); deadline = callback; return 1; };
  dom.window.clearTimeout = () => { deadline = undefined; };
  dom.window.document.addEventListener('click', () => { clicks++; });
  dom.window.document.addEventListener('focus', () => { focuses++; }, true);
  dom.window.ipc = { postMessage: (message: string) => { const { activated, mode } = JSON.parse(message); reports.push(mode ? { activated, mode } : { activated }); } };
  try {
    dom.window.eval(script.replace('__AIWR_PROJECT_URL__', JSON.stringify(options.expected ?? 'https://chatgpt.com/g/project-123/project')));
    options.late?.(dom.window);
    await Promise.resolve();
    await Promise.resolve();
    deadline?.();
    // Subsequent provider changes after success/timeout must never trigger another action.
    dom.window.document.querySelector('main')?.insertAdjacentHTML('beforeend', '<button data-testid="new-chat-button">late</button>');
    await Promise.resolve();
    const composer = dom.window.document.querySelector('#prompt-textarea');
    return { clicks, focuses, reports, draft: composer?.value ?? composer?.textContent ?? null };
  } finally { dom.window.close(); }
}
describe('exact Project New Chat action', () => {
  it('invokes one exact control with precedence over an empty Project composer', async () => {
    expect(await activate('<main><textarea id="prompt-textarea"></textarea><button data-testid="create-new-chat-button">Fixture</button></main>')).toMatchObject({ clicks: 1, focuses: 0, reports: [{ activated: true, mode: 'NEW_CHAT_CONTROL' }] });
  });
  it('refuses wrong Project and query-bearing expected URL', async () => {
    for (const expected of ['https://chatgpt.com/g/other-project/project', 'https://chatgpt.com/g/project-123/project?x=1']) {
      expect(await activate('<main><button data-testid="new-chat-button">New chat</button></main>', { expected })).toMatchObject({ clicks: 0, focuses: 0, reports: [{ activated: false }] });
    }
  });
  it('waits for a late native button and clicks exactly once', async () => {
    expect(await activate('<main></main>', { late: window => window.document.querySelector('main').innerHTML = '<button data-testid="new-chat-button">New chat</button>' })).toMatchObject({ clicks: 1, reports: [{ activated: true, mode: 'NEW_CHAT_CONTROL' }] });
  });
  it('waits for a disabled control to become ready', async () => {
    expect(await activate('<main><button data-testid="new-chat-button" disabled>New chat</button></main>', { late: window => window.document.querySelector('button').disabled = false })).toMatchObject({ clicks: 1, reports: [{ activated: true, mode: 'NEW_CHAT_CONTROL' }] });
  });
  it('focuses one late empty provider Project draft without submitting or creating a conversation', async () => {
    expect(await activate('<main></main>', { late: window => window.document.querySelector('main').innerHTML = '<div id="prompt-textarea" contenteditable="true" role="textbox" aria-label="ai-work-router中的新聊天"></div>' })).toMatchObject({ clicks: 0, focuses: 1, draft: '', reports: [{ activated: true, mode: 'PROJECT_DRAFT_COMPOSER' }] });
  });
  it('focuses an empty native textarea without clicking', async () => {
    expect(await activate('<main><textarea id="prompt-textarea" aria-label="ai-work-router中的新聊天"></textarea></main>')).toMatchObject({ clicks: 0, focuses: 1, reports: [{ activated: true, mode: 'PROJECT_DRAFT_COMPOSER' }] });
  });
  it('preserves human drafts including whitespace, even with an available New Chat button', async () => {
    for (const draft of ['human draft', ' ']) {
      expect(await activate(`<main><textarea id="prompt-textarea">${draft}</textarea><button data-testid="new-chat-button">New chat</button></main>`)).toMatchObject({ clicks: 0, focuses: 0, draft, reports: [{ activated: false }] });
    }
  });
  it('fails closed for absent, ambiguous, disabled, outside-main and noneditable controls', async () => {
    for (const html of [
      '<main></main>',
      '<main><button data-testid="new-chat-button"></button><button data-testid="create-new-chat-button"></button></main>',
      '<main><button data-testid="new-chat-button" disabled></button><textarea id="prompt-textarea"></textarea></main>',
      '<main><button data-testid="new-chat-button" aria-disabled="true"></button></main>',
      '<aside><button data-testid="new-chat-button"></button></aside><main></main>',
      '<main><div id="prompt-textarea"></div></main>',
      '<main><textarea id="prompt-textarea"></textarea><textarea data-testid="prompt-textarea"></textarea></main>',
    ]) expect(await activate(html)).toMatchObject({ clicks: 0, focuses: 0, reports: [{ activated: false }] });
  });
  it('recognizes only exact fallback accessible names', async () => {
    expect(await activate('<main><button aria-label="New chat"></button></main>')).toMatchObject({ clicks: 1, reports: [{ activated: true, mode: 'NEW_CHAT_CONTROL' }] });
    for (const html of ['<main><button aria-label="New chat settings"></button></main>', '<main><button aria-label="New chat"></button><button aria-label="新聊天"></button></main>']) {
      expect(await activate(html)).toMatchObject({ clicks: 0, reports: [{ activated: false }] });
    }
  });
  it('rejects navigation while waiting for provider readiness', async () => {
    expect(await activate('<main></main>', { late: window => { window.history.pushState({}, '', '/g/other-project/project'); window.document.querySelector('main').innerHTML = '<textarea id="prompt-textarea"></textarea>'; } })).toMatchObject({ clicks: 0, focuses: 0, reports: [{ activated: false }] });
  });
});
