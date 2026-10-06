(() => {
  const expected = __AIWR_PROJECT_URL__;
  let finished = false, observer, timeout;
  const report = (activated, mode) => {
    if (finished) return;
    finished = true;
    observer?.disconnect();
    clearTimeout(timeout);
    window.removeEventListener('popstate', attempt);
    const payload = { kind: 'AIWR_CHATGPT_NEW_CHAT', activated, mode };
    if (window.ipc?.postMessage) window.ipc.postMessage(JSON.stringify(payload));
    else window.chrome?.webview?.postMessage(payload);
  };
  const attempt = () => {
    if (finished) return;
    if (location.href !== expected) { report(false); return; }
    const main = document.querySelector('main, [role="main"]');
    if (!main) return;
    const composers = [...main.querySelectorAll('#prompt-textarea, [data-testid="prompt-textarea"]')];
    if (composers.length > 1) { report(false); return; }
    const composer = composers[0];
    // Do not clear, replace, or abandon any human draft, including whitespace.
    const draft = composer instanceof HTMLTextAreaElement ? composer.value : (composer?.innerText ?? composer?.textContent ?? '');
    if (draft.length) { report(false); return; }
    const exact = main.querySelectorAll('[data-testid="create-new-chat-button"], [data-testid="new-chat-button"]');
    const controls = exact.length ? [...exact] : [...main.querySelectorAll('button, a[role="button"]')].filter(node => /^(New chat|Start a new chat|新聊天|发起新聊天)$/i.test((node.getAttribute('aria-label') || node.innerText || node.textContent || '').trim()));
    if (controls.length > 1) { report(false); return; }
    if (controls.length === 1) {
      const control = controls[0];
      if (control.disabled || control.getAttribute('aria-disabled') === 'true') return;
      // Disconnect before the sole provider action so its mutations cannot retry it.
      observer?.disconnect();
      control.click();
      report(true, 'NEW_CHAT_CONTROL');
      return;
    }
    if (!composer || composer.disabled || composer.readOnly || composer.getAttribute('aria-disabled') === 'true') return;
    if (!(composer instanceof HTMLTextAreaElement) && composer.getAttribute('contenteditable') !== 'true') return;
    observer?.disconnect();
    composer.focus();
    report(document.activeElement === composer, 'PROJECT_DRAFT_COMPOSER');
  };
  // Observe readiness only: no action is retried and no history is scrolled.
  observer = new MutationObserver(attempt);
  observer.observe(document.documentElement, { childList: true, subtree: true, attributes: true, characterData: true });
  window.addEventListener('popstate', attempt);
  timeout = setTimeout(() => report(false), 8000);
  attempt();
})()
