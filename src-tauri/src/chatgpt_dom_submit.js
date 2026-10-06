(async () => {
  const { text, expectedId, token } = __AIWR_SEND_ARGUMENTS__;
  const observer = window.__aiwrChatGptObserver;
  let method = "NONE";
  const post = (phase, snapshot, payloadEqual = false, rejectionCode = null) => {
    const payload = {kind:'AIWR_CHATGPT_SEND', token, phase, snapshot, payloadEqual, method, rejectionCode};
    if (window.ipc?.postMessage) window.ipc.postMessage(JSON.stringify(payload));
    else window.chrome?.webview?.postMessage(payload);
  };
  const exact = () => {
    const snapshot = observer?.capture();
    const parts = new URL(location.href).pathname.split('/').filter(Boolean);
    const id = parts.length === 2 && parts[0] === 'c' ? parts[1] : parts.length === 4 && parts[0] === 'g' && parts[2] === 'c' ? parts[3] : null;
    return snapshot && id === expectedId ? snapshot : null;
  };
  // One transaction per surface until accepted/rejected by the host. No retries.
  if (window.__aiwrPendingSend || !exact()) { post('REJECTED'); return; }
  window.__aiwrPendingSend = token;
  try {
    const composers = document.querySelectorAll('#prompt-textarea, textarea[data-testid="prompt-textarea"], [contenteditable="true"][data-testid="prompt-textarea"]');
    const composer = composers.length === 1 ? composers[0] : null;
    if (!composer || exact().generationActive || exact().turns.some(turn => turn.streaming)) throw new Error('PRECONDITION');
    // innerText adds visual blank lines between ProseMirror paragraphs. Read
    // editor content structurally: one paragraph boundary is one payload newline.
    const editorText = node => {
      if (node.nodeType === Node.TEXT_NODE) return node.nodeValue || '';
      if (node.nodeName === 'BR') return node.classList?.contains('ProseMirror-trailingBreak') ? '' : '\n';
      const children = Array.from(node.childNodes);
      const block = child => child.nodeType === Node.ELEMENT_NODE && /^(P|DIV|LI|PRE)$/.test(child.nodeName);
      return children.map((child, i) => (i > 0 && (block(child) || block(children[i - 1])) ? '\n' : '') + editorText(child)).join('');
    };
    const read = () => composer instanceof HTMLTextAreaElement ? composer.value : editorText(composer);
    // Preserve unrelated human draft; it cannot be silently replaced.
    if (read() && read() !== text) throw new Error('DRAFT_PRESENT');
    composer.focus();
    if (composer instanceof HTMLTextAreaElement) {
      Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(composer, text);
    } else {
      const selection = window.getSelection(); const range = document.createRange();
      range.selectNodeContents(composer); selection.removeAllRanges(); selection.addRange(range);
      if (!document.execCommand('insertText', false, text)) throw new Error('INPUT_REJECTED');
    }
    composer.dispatchEvent(new InputEvent('input', {bubbles:true,inputType:'insertText',data:text}));
    // Allow the provider's input handler to commit button state, without retrying any action.
    await new Promise(resolve => setTimeout(resolve, 100));
    const pre = exact();
    const controls = document.querySelectorAll('button[data-testid="send-button"], button[aria-label="Send prompt"]');
    const send = controls.length === 1 ? controls[0] : null;
    if (!pre) throw new Error('IDENTITY_CHANGED');
    if (!composer.isConnected) throw new Error('COMPOSER_DETACHED');
    if (read() !== text) throw new Error('PAYLOAD_MISMATCH');
    if (pre.generationActive || pre.turns.some(turn => turn.streaming)) throw new Error('GENERATION_ACTIVE');
    if (!send || send.disabled || send.getAttribute('aria-disabled') === 'true') throw new Error('SEND_UNAVAILABLE');
    // One explicit submission route; never chain click, form, or keyboard attempts.
    // Select ONE route before dispatch; never chain click/form/keyboard attempts.
    const composerForm = composer.form || composer.closest('form');
    const sendForm = send.form || send.closest('form');
    if (composerForm && sendForm && composerForm !== sendForm) throw new Error('FORM_MISMATCH');
    const form = composerForm || sendForm;
    if (form && typeof form.requestSubmit !== 'function') throw new Error('FORM_UNAVAILABLE');
    if (form && !form.checkValidity()) throw new Error('FORM_INVALID');
    method = form ? 'FORM_REQUEST_SUBMIT' : 'BUTTON_CLICK';
    post('PREPARED', pre, true);
    post('DISPATCHED');
    if (form) form.requestSubmit();
    else send.click();
    observer.emit();
  } catch (error) {
    const allowed = ['PRECONDITION', 'DRAFT_PRESENT', 'INPUT_REJECTED', 'IDENTITY_CHANGED', 'COMPOSER_DETACHED', 'PAYLOAD_MISMATCH', 'GENERATION_ACTIVE', 'SEND_UNAVAILABLE', 'FORM_MISMATCH', 'FORM_UNAVAILABLE', 'FORM_INVALID'];
    post('REJECTED', undefined, false, allowed.includes(error?.message) ? error.message : 'SUBMIT_EXCEPTION');
  }
})()
