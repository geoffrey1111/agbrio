(() => {
  if (window.__aiwrChatGptObserver?.emit) {
    window.__aiwrChatGptObserver.emit(true);
    return;
  }
  const encoder = new TextEncoder();
  const boundedText = (value, max = 32768) => {
    const text = String(value || '').slice(0, max);
    const bytes = encoder.encode(text);
    // Streaming decode drops an incomplete final codepoint without exceeding
    // the byte cap; avoid allocating an encoder/string for every character.
    return bytes.length <= max ? text : new TextDecoder().decode(bytes.subarray(0, max), { stream: true });
  };
  const turnSelector = '[data-turn], [data-message-author-role], [data-testid^="conversation-turn-"]';
  const controlSelector = '#prompt-textarea, [data-testid="prompt-textarea"], [data-testid="send-button"], [data-testid="stop-button"], button[aria-label="Send prompt"]';
  const outerTurn = node => {
    const element = node?.nodeType === 1 ? node : node?.parentElement;
    return element?.closest('[data-turn], [data-testid^="conversation-turn-"]') || element?.closest('[data-message-author-role]');
  };
  let cachedRoot = null, cachedHref = '', textCache = new WeakMap();
  let knownTurns = new WeakSet(), dirtyTurns = new WeakSet();
  const dirty = turn => { textCache.delete(turn); dirtyTurns.add(turn); };
  const invalidate = records => {
    if (!window.document?.documentElement) return false;
    const root = document.querySelector('main, [role="main"]');
    let relevant = root !== cachedRoot || location.href !== cachedHref;
    if (relevant) { textCache = new WeakMap(); knownTurns = new WeakSet(); dirtyTurns = new WeakSet(); cachedRoot = root; cachedHref = location.href; }
    for (const record of records) {
      const element = record.target.nodeType === 1 ? record.target : record.target.parentElement;
      const turn = outerTurn(element);
      if (turn && root?.contains(turn)) { dirty(turn); relevant = true; continue; }
      if (root?.contains(element)) {
        relevant = true;
        if (record.type === 'attributes') {
          // A layout wrapper can hide/reveal several turns at once.
          for (const child of element.querySelectorAll(turnSelector)) {
            const affected = outerTurn(child);
            if (affected) dirty(affected);
          }
        }
      }
      // Ancestor style/visibility changes can alter rendered text without a text mutation.
      if (record.type === 'attributes' && element?.contains(root)) {
        textCache = new WeakMap(); relevant = true;
      }
      if (element?.matches(controlSelector) || element?.closest(controlSelector)) relevant = true;
      for (const node of [...(record.addedNodes || []), ...(record.removedNodes || [])]) {
        if (node.nodeType === 1 && (node.matches(controlSelector) || node.querySelector(controlSelector))) relevant = true;
      }
    }
    return relevant;
  };
  const capture = () => {
    if (location.protocol !== 'https:' || location.hostname !== 'chatgpt.com') return null;
    // Send preflight may run before the observer microtask; never return stale text.
    invalidate(observer.takeRecords());
    const seen = new Set();
    const turns = [];
    const conversation = /^\/c\/[A-Za-z0-9_-]+\/?$/.test(location.pathname)
      || /^\/g\/[^/]+\/c\/[A-Za-z0-9_-]+\/?$/.test(location.pathname);
    const root = document.querySelector('main, [role="main"]');
    const seenIds = new Set();
    const generationActive = Boolean(document.querySelector('[data-testid="stop-button"]'));
    const candidates = [];
    for (const node of (conversation ? root?.querySelectorAll('[data-turn], [data-message-author-role], [data-testid^="conversation-turn-"]') || [] : [])) {
      const turn = node.closest('[data-turn], [data-testid^="conversation-turn-"]') || node;
      if (seen.has(turn)) continue;
      seen.add(turn);
      const message = turn.matches('[data-message-author-role]') ? turn : turn.querySelector('[data-message-author-role]');
      const role = (turn.getAttribute('data-turn') || message?.getAttribute('data-message-author-role') || '').toLowerCase();
      if (role !== 'user' && role !== 'assistant') continue;
      const id = turn.getAttribute('data-turn-id') || message?.getAttribute('data-message-id') || turn.getAttribute('data-message-id') || turn.getAttribute('data-testid') || undefined;
      if (id && seenIds.has(id)) continue;
      if (id) seenIds.add(id);
      candidates.push({ turn, message, role, id });
    }
    // Always retain the current tail, but include newly materialized/changed
    // older turns when users load history without removing the newer DOM.
    const selected = new Set(candidates.slice(-20).map(candidate => candidate.turn));
    const changed = candidates.filter(({ turn }) => !selected.has(turn) && (!knownTurns.has(turn) || dirtyTurns.has(turn)));
    for (const { turn } of changed.slice(-180)) selected.add(turn);
    for (let index = candidates.length - 1; index >= 0 && selected.size < 200; index--) selected.add(candidates[index].turn);
    for (const { turn } of candidates) knownTurns.add(turn);
    for (const { turn, message, role, id } of candidates.filter(candidate => selected.has(candidate.turn))) {
      dirtyTurns.delete(turn);
      let text = textCache.get(turn);
      if (text === undefined) {
        const content = message || turn;
        const textRoots = role === 'user' ? [content.querySelector('.whitespace-pre-wrap') || content]
          : Array.from(content.querySelectorAll('.markdown')).filter(node => !node.parentElement?.closest('.markdown'));
        if (!textRoots.length) textRoots.push(content);
        text = boundedText(textRoots.map(node => node.innerText ?? node.textContent ?? '').join('\n\n'));
        textCache.set(turn, text);
      }
      const streaming = role === 'assistant' && Boolean(turn.matches('[data-is-streaming="true"], [data-state="streaming"]') || turn.querySelector('[data-is-streaming="true"], [data-state="streaming"], .result-streaming'));
      // A missing Stop button alone is not terminal evidence.
      const terminal = role === 'assistant' && !generationActive && !streaming && Boolean(turn.querySelector('[data-testid="copy-turn-action-button"], [data-testid="good-response-turn-action-button"], [data-testid="bad-response-turn-action-button"]'));
      turns.push({ id, role, text, streaming, terminal });
    }
    const projectRows = [];
    const project = location.pathname.match(/^\/g\/([A-Za-z0-9_-]{3,128})\/project$/);
    if (project) {
      const prefix = `/g/${project[1]}/c/`;
      const rows = new Set();
      for (const anchor of document.querySelectorAll(`main a[href^="${prefix}"]`)) {
        const href = new URL(anchor.href, location.href);
        if (href.origin !== location.origin || !href.pathname.startsWith(prefix)) continue;
        const conversationId = href.pathname.slice(prefix.length).split('/')[0];
        const title = boundedText((anchor.innerText || anchor.getAttribute('aria-label') || '').trim(), 240);
        if (!conversationId || !title || rows.has(conversationId)) continue;
        rows.add(conversationId);
        projectRows.push({ href: href.href, title });
        if (projectRows.length >= 200) break;
      }
    }
    const composer = document.querySelector('#prompt-textarea, textarea[data-testid="prompt-textarea"], [contenteditable="true"][data-testid="prompt-textarea"]');
    const composerText = composer instanceof HTMLTextAreaElement ? composer.value : (composer?.innerText || '');
    const send = document.querySelector('button[data-testid="send-button"], button[aria-label="Send prompt"]');
    const composerState = { present: Boolean(composer), textLength: composerText.length, sendAvailable: Boolean(send), sendDisabled: Boolean(send?.disabled) };
    return { kind: 'AIWR_CHATGPT_DOM_SNAPSHOT', href: location.href, turns, generationActive, composer: composerState, projectRows };
  };
  let lastSerialized = null;
  const emit = (force = false) => {
    const payload = capture();
    if (!payload) return;
    const serialized = JSON.stringify(payload);
    if (!force && serialized === lastSerialized) return;
    lastSerialized = serialized;
    if (window.ipc?.postMessage) window.ipc.postMessage(serialized);
    else window.chrome?.webview?.postMessage(payload);
  };
  let pending = false;
  const schedule = () => { if (!pending) { pending = true; setTimeout(() => { pending = false; emit(); }, 150); } };
  const observer = new MutationObserver(records => { if (invalidate(records)) schedule(); });
  observer.observe(document.documentElement, { childList: true, subtree: true, characterData: true, attributes: true, attributeFilter: ['data-is-streaming', 'data-state', 'data-message-id', 'data-turn-id', 'data-turn', 'data-message-author-role', 'data-testid', 'disabled', 'aria-disabled', 'aria-label', 'class', 'style', 'hidden', 'href', 'id', 'role'] });
  window.addEventListener('popstate', schedule);
  document.addEventListener('input', event => { if (event.target?.matches(controlSelector)) schedule(); }, true);
  window.__aiwrChatGptObserver = { capture, emit };
  schedule();
})();
