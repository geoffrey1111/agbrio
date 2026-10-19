/* This runs only inside the opaque-origin sandbox. No direct network or Host credentials. */
(() => {
  'use strict';
  let channel = document.currentScript.dataset.channel || null;
  const send = (kind, data = {}) => parent.postMessage({channel, kind, ...data}, '*');
  const normalize = value => {
    try { if (value.startsWith('file:')) value = decodeURIComponent(new URL(value).pathname).replace(/^\/([a-z]:)/i, '$1'); } catch { return ''; }
    return value.replaceAll('\\', '/').toLowerCase();
  };
  let references = new Map(), ready = false, next = 0, stateTimer, saved;
  const pending = new Map(), loaded = new Map(), reviews = new Map(), reviewWaiters = new Set();
  let reviewReady=false;
  const permitReviews=()=>{if(ready&&reviewReady){for(const done of reviewWaiters)done();reviewWaiters.clear();}};
  // Defined before approval-ui.js executes. Tokens and requests live in memory only.
  window.BOARD_REVIEW_TRANSPORT={request:async input=>{
    if(!ready||!reviewReady)await new Promise((resolve,reject)=>{const timer=setTimeout(()=>{reviewWaiters.delete(done);reject(new Error('最新看板尚未确认；请恢复连接后刷新看板。'));},19000);const done=()=>{clearTimeout(timer);resolve();};reviewWaiters.add(done);});
    return new Promise((resolve,reject)=>{const request=String(++next),timer=setTimeout(()=>{reviews.delete(request);reject(new Error('回执超时；请核对已保存状态，不要重复提交。'));},20000);reviews.set(request,{resolve,reject,timer});send('review',{request,input});});
  }};
  const asset = (source,variant='original') => {
    const id = references.get(normalize(source));
    if (!id) return Promise.reject(new Error('这个文件不在当前看板的预览范围内。'));
    const key=`${id}:${variant}`;
    if (loaded.has(key)) return loaded.get(key);
    const promise = new Promise((resolve, reject) => {
      const request = String(++next);
      // Queueing is not a network timeout: current-role requests run first.
      const timer = setTimeout(() => { pending.delete(request); reject(new Error('图片暂未读取成功，请稍后重试。')); }, 120000);
      pending.set(request, {resolve, reject, timer}); send('asset', {id, request,variant,scope:location.hash});
    });
    loaded.set(key, promise);promise.then(()=>{while(loaded.size>64)loaded.delete(loaded.keys().next().value);}).catch(()=>loaded.delete(key));return promise;
  };
  function images() {
    if (!ready) return;
    document.querySelectorAll('img,svg image').forEach(element => {
      const source = element.getAttribute(element.tagName.toLowerCase() === 'img' ? 'src' : 'href');
      if (!source || /^(data:|blob:)/.test(source) || element.dataset.boardSource === source) return;
      element.dataset.boardSource = source;
      element.dataset.boardPending='true';delete element.dataset.boardAsset;delete element.dataset.boardSourceHash;
      const variant=element.closest('.character-link')?'thumbnail':'preview';
      const load = () => asset(source,variant).then(result => {
        if (!result.mime.startsWith('image/')) throw new Error('不是可预览的图片。');
        // An older response must never repaint an element reused for another source.
        if(!element.isConnected||element.dataset.boardSource!==source)return;
        const attribute=element.tagName.toLowerCase()==='img'?'src':'href';
        if(element.getAttribute(attribute)!==source)return;
        element.setAttribute(element.tagName.toLowerCase() === 'img' ? 'src' : 'href', `data:${result.mime};base64,${result.data}`);
        delete element.dataset.boardPending;
        element.dataset.boardAsset=result.assetId; element.dataset.boardSourceHash=result.sourceHash;
      }).catch(() => { if(element.dataset.boardSource!==source)return;element.setAttribute('title','图片暂不可用，点原图链接重试'); element.dataset.boardSource = ''; });
      element._boardLoad = load;
      observer.observe(element);
    });
  }
  const observer = new IntersectionObserver(entries => entries.forEach(entry => { if (entry.isIntersecting) { observer.unobserve(entry.target); entry.target._boardLoad?.(); } }), {rootMargin:'350px'});
  const mutations = new MutationObserver(images);
  const detailKey = e => `${e.closest('section[id]')?.id||''}|${e.querySelector('summary')?.textContent?.trim().slice(0,200)||''}`;
  function capture() {
    if (!ready) return;
    clearTimeout(stateTimer); stateTimer = setTimeout(() => send('state', {state: {
      hash: location.hash, scrollY, search: document.getElementById('search')?.value || '', filter: document.getElementById('filter')?.value || 'all',
      details: [...document.querySelectorAll('details')].flatMap(e => e.open ? [detailKey(e)] : [])
    }}), 120);
  }
  function restore(state) {
    if (!state) return;
    const hash = typeof state.hash === 'string' && /^#(?:overview|review-[a-z0-9_-]+)$/.test(state.hash) ? state.hash : '#overview';
    location.hash = hash;
    for (const key of ['search','filter']) { const e=document.getElementById(key); if(e&&typeof state[key]==='string'){e.value=state[key];e.dispatchEvent(new Event(key==='search'?'input':'change'));} }
    const open = new Set((state.details||[]).slice(0,100));document.querySelectorAll('details').forEach(e=>{if(open.has(detailKey(e)))e.open=true;});
    requestAnimationFrame(() => requestAnimationFrame(() => scrollTo(0, Math.max(0, Number(state.scrollY)||0))));
  }
  addEventListener('message', event => {
    if (event.source !== parent) return;
    if(channel&&event.data?.channel!==channel)return;
    if(!channel&&!(event.data?.kind==='init'&&typeof event.data.channel==='string'&&/^[a-f0-9-]{36}$/i.test(event.data.channel)))return;
    const message=event.data;
    if (message.kind==='init'&&!ready) {
      channel=message.channel;references=new Map(message.assets.map(a=>[normalize(a.source),a.id]));ready=true;saved=message.state;reviewReady=message.reviewReady===true;permitReviews();
      if(document.readyState!=='loading'){restore(saved);images();}
    } else if(message.kind==='reviewReady'){reviewReady=message.ready===true;permitReviews();
    } else if(message.kind==='reviewResult'){const p=reviews.get(message.request);if(!p)return;clearTimeout(p.timer);reviews.delete(message.request);p.resolve(message.response);
    } else if(message.kind==='assetStarted') {
      const p=pending.get(message.request);if(!p)return;clearTimeout(p.timer);
      p.timer=setTimeout(()=>{pending.delete(message.request);p.reject(new Error('图片暂未读取成功，请稍后重试。'));},47000);
    } else if(message.kind==='assetResult') {
      const p=pending.get(message.request);if(!p)return;clearTimeout(p.timer);pending.delete(message.request);
      message.error?p.reject(new Error(message.error)):p.resolve(message.asset);
    }
  });
  function start() {
    // The source already owns its responsive grids and image slots. Do not
    // override their geometry: source captions must stay beside their images.
    const style=document.createElement('style');style.textContent='html{overflow-x:hidden}body{margin:0}[data-board-pending]{opacity:0}button,input,select,textarea{font-size:max(16px,1em)}.board-overlay{position:fixed;inset:0;z-index:9999;background:#fffdf9;overflow:auto;padding:16px;box-sizing:border-box}.board-overlay button{position:sticky;top:0;min-height:44px;padding:8px 16px}.board-overlay img{display:block;margin:auto;max-width:100%;height:auto}.board-overlay h2{font-size:18px;overflow-wrap:anywhere}.board-overlay pre{white-space:pre-wrap;overflow-wrap:anywhere}';document.head.append(style);
    mutations.observe(document.body,{subtree:true,childList:true,attributes:true,attributeFilter:['src','href']});
    addEventListener('scroll',capture,{passive:true});addEventListener('hashchange',()=>{send('view',{scope:location.hash});capture();});addEventListener('input',capture);addEventListener('change',capture);document.addEventListener('toggle',capture,true);
    document.addEventListener('click',event=>{if(event.target.closest('#reload')){event.preventDefault();event.stopImmediatePropagation();send('refresh');}},{capture:true});
    document.addEventListener('click',event=>{
      const anchor=event.target.closest('a');if(!anchor)return;
      const href=anchor.getAttribute('href');if(!href)return;
      if(href.startsWith('#')){event.preventDefault();if(location.hash!==href)location.hash=href;return;}
      event.preventDefault();
      const overlay=document.createElement('section');overlay.className='board-overlay';overlay.setAttribute('role','dialog');overlay.setAttribute('aria-label','查看看板原件');
      const close=document.createElement('button');close.textContent='返回看板';close.onclick=()=>overlay.remove();overlay.append(close);
      const title=document.createElement('h2');title.textContent=anchor.closest('figure')?.querySelector('figcaption')?.textContent?.trim()||anchor.textContent?.trim()||anchor.querySelector('img')?.alt||'看板原件';overlay.append(title);
      const filename=document.createElement('p');overlay.append(filename);
      const text=document.createElement('pre');text.textContent='正在读取…';overlay.append(text);document.body.append(overlay);close.focus();
      asset(href).then(result=>{filename.textContent=result.filename;if(result.mime.startsWith('image/')){const img=document.createElement('img');img.alt=result.filename;img.src=`data:${result.mime};base64,${result.data}`;text.replaceWith(img);}else{text.textContent=new TextDecoder().decode(Uint8Array.from(atob(result.data),c=>c.charCodeAt(0)));}}).catch(error=>{text.textContent=error.message;});
    });
    document.addEventListener('keydown',event=>{if(event.key==='Escape')document.querySelector('.board-overlay')?.remove();});
    if(ready){restore(saved);images();}send('ready');
  }
  document.addEventListener('DOMContentLoaded',start,{once:true});
})();
