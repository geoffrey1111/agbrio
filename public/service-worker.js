// Cache only the public application shell. Never cache API, auth, messages,
// attachments or redirected Access/login pages. A tunnel 5xx gets the shell
// with a reconnect screen; it does not grant offline authenticated access.
// Isolate this corrected layout from still-running legacy workers. They must
// not overwrite its HTML while committing an older background refresh.
const SHELL_CACHE = "aiwr-public-shell-v12";
const SHELL_KEY = "/mobile";
function shellResponse(response) { return response.ok && !response.redirected && (response.headers.get("content-type") || "").includes("text/html"); }
function staticAsset(url) { return url.origin === self.location.origin && (url.pathname.startsWith("/assets/") || url.pathname.startsWith("/fonts/") || ["/icon.ico","/manifest.webmanifest"].includes(url.pathname)); }
async function withDeadline(work) {
  const abort = new AbortController();let timer;
  const deadline = new Promise((_,reject) => {timer=setTimeout(()=>{abort.abort();reject(new Error("connection timeout"));},8000);});
  try { return await Promise.race([work(abort.signal),deadline]); } finally { clearTimeout(timer); }
}
async function fetchNavigation(request) {
  return withDeadline(async signal => {
    // Cached navigation responses must not feed back through the HTTP cache
    // and overwrite a newer shell. The foreground still uses Cache Storage.
    const response = await fetch(request,{signal,cache:"no-store"});
    if (!shellResponse(response)) return response;
    // Include body consumption in the timeout: headers alone are not a page.
    const bytes = await response.arrayBuffer();
    return new Response(bytes,{status:response.status,statusText:response.statusText,headers:response.headers});
  });
}
async function prepareShell(response) {
  return withDeadline(async signal => {
    const next = response || await fetch(SHELL_KEY,{credentials:"omit",cache:"no-store",signal});
    await cacheShell(next,signal);
  });
}
async function cacheShell(response,signal) {
  if (!shellResponse(response)) return;
  const html = await response.clone().text();
  const cache = await caches.open(SHELL_CACHE);
  // Only Vite's same-origin asset links; no arbitrary URLs from page content.
  let paths = [...html.matchAll(/(?:src|href)=["'](\/assets\/[^"']+)["']/g)].map(match => match[1]);
  const manifest=html.match(/name="aiwr-shell-assets" content="(\/assets\/[A-Za-z0-9_-]+\.json)"/);
  const previous=await cache.match?.(SHELL_KEY);
  if(manifest&&previous){
    const old=await previous.clone().text();
    const oldManifest=old.match(/name="aiwr-shell-assets" content="(\/assets\/[A-Za-z0-9_-]+\.json)"/);
    if(oldManifest?.[1]===manifest[1]){
      // This version's complete graph was already committed with its HTML.
      // Avoid re-downloading the full app on every warm open, competing with
      // live auth and message requests. Inline public-shell changes still update.
      if(signal.aborted)throw new Error("connection timeout");
      await cache.put(SHELL_KEY,response.clone());return;
    }
  }
  if(manifest){
    const response=await fetch(manifest[1],{credentials:"omit",cache:"no-cache",signal});
    if(!response.ok||response.redirected)throw new Error("shell manifest unavailable");
    const declared=await response.json();
    if(!Array.isArray(declared)||declared.length>64||declared.some(path=>typeof path!=="string"||!/^\/assets\/[A-Za-z0-9_.-]+\.(js|css)$/.test(path)))throw new Error("invalid shell manifest");
    paths=[...paths,...declared];
  }
  const resources = await Promise.all([...new Set(paths)].map(async path => {
    const asset = await fetch(path, {credentials:"omit",cache:"no-cache",signal});
    if (!asset.ok || asset.redirected) throw new Error("shell asset unavailable");
    return [path,asset];
  }));
  for (const [path,asset] of resources) {if(signal.aborted)throw new Error("connection timeout");await cache.put(path,asset);}
  if(signal.aborted)throw new Error("connection timeout");
  await cache.put(SHELL_KEY,response.clone());
}
self.addEventListener("fetch", event => {
  const request = event.request, url = new URL(request.url);
  if (request.method !== "GET" || url.origin !== self.location.origin) return;
  if (request.mode === "navigate" && ["/mobile","/mobile/notifications"].includes(url.pathname)) {
    // Reuse the public shell immediately; live auth still gates every private
    // view/API. Keep the exact navigation URL (including notification event).
    const refreshed=fetchNavigation(request);
    event.waitUntil(refreshed.then(response=>shellResponse(response)?prepareShell(response.clone()):undefined).catch(()=>undefined));
    event.respondWith((async () => {
      const cached=await caches.match(SHELL_KEY,{cacheName:SHELL_CACHE});
      if(cached)return cached;
      let response;
      try { response = await refreshed; } catch { /* Initial offline install. */ }
      return response || new Response("Router 暂时无法连接，请稍后重试。",{status:503,headers:{"content-type":"text/plain; charset=utf-8"}});
    })());
  } else if (staticAsset(url)) {
    event.respondWith((async () => {
      const cache = await caches.open(SHELL_CACHE), cached = await cache.match(request);
      if (cached) return cached;
      const response = await fetch(request);
      if (response.ok && !response.redirected) await cache.put(request,response.clone());
      return response;
    })());
  }
});
self.addEventListener("install", event => {
  // Replace a legacy worker as soon as the owner opens the PWA once after an
  // upgrade.  The script contains no reply content and retains the same scope.
  event.waitUntil((async () => {
    try { await prepareShell(); } catch { /* Existing shell survives a temporary outage. */ }
    await self.skipWaiting();
  })());
});
self.addEventListener("activate", event => {
  event.waitUntil(clients.claim());
});

self.addEventListener("push", event => {
  let data = {};
  try { data = event.data ? event.data.json() : {}; } catch {
    data = event.data && event.data.text() === "AI_WORK_ROUTER_WEB_PUSH_V1_OK" ? { type: "test" } : {};
  }
  const workstreamId = typeof data.workstreamId === "string" ? data.workstreamId : "";
  const observationId = typeof data.observationId === "string" ? data.observationId : "";
  const workstreamName = typeof data.workstreamName === "string" && data.workstreamName.trim() ? data.workstreamName.trim().slice(0, 120) : "Workstream";
  const provider = data.type === "codex_reply" ? "Codex" : "ChatGPT";
  const watch = data.type === "codex_watch";
  const sequence = Number.isSafeInteger(data.sequence) && data.sequence > 0 ? data.sequence : null;
  const label = typeof data.label === "string" ? data.label.slice(0, 120) : "Codex 对话";
  const target = watch ? `/mobile/notifications${sequence ? `?event=${sequence}` : ""}` : workstreamId ? `/mobile?workstream=${encodeURIComponent(workstreamId)}${observationId ? `&reply=${encodeURIComponent(observationId)}` : ""}` : "/mobile";
  event.waitUntil((async () => {
    await self.registration.showNotification("Agbrio", {
      body: watch ? `${label} · ${data.state === "ACTION_REQUIRED" ? "需要你确认或回答" : data.state === "FAILED" ? "执行失败" : data.state === "TEST" ? "通知通道测试" : "新结果已到达"}` : data.type === "test" ? "AI_WORK_ROUTER_WEB_PUSH_V1_OK" : `${workstreamName} · ${provider} 有新回复`,
      data: { target },
      tag: watch ? `codex-watch-${data.eventId || sequence || "test"}` : observationId ? `${provider.toLowerCase()}-reply-${observationId}` : `${provider.toLowerCase()}-reply`
    });
    // A receipt is meaningful only for a real persisted observation. It is a
    // Service Worker rendering receipt, not proof that iOS surfaced a banner
    // or that the owner saw it.
    if (!watch && workstreamId && observationId) {
      await fetch("/v1/mobile/push/rendered", {
        method: "POST",
        credentials: "same-origin",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ workstreamId, observationId })
      }).catch(() => undefined);
    }
  })());
});
self.addEventListener("notificationclick", event => {
  event.notification.close();
  const requested = event.notification.data && event.notification.data.target || "/mobile";
  let target = new URL(requested, self.location.origin);
  if (target.origin !== self.location.origin || !["/mobile", "/mobile/notifications"].includes(target.pathname)) {
    target = new URL("/mobile", self.location.origin);
  }
  event.waitUntil((async () => {
    const windows = await clients.matchAll({ type: "window", includeUncontrolled: true });
    const existing = windows.find(windowClient => {
      try {
        const location = new URL(windowClient.url);
        return location.origin === target.origin && ["/mobile", "/mobile/notifications"].includes(location.pathname);
      } catch {
        return false;
      }
    });
    if (existing) {
      try {
        const client = await existing.navigate(target.href);
        if (client) {
          await client.focus();
          return;
        }
      } catch {
        // iOS may reject navigation of an existing standalone client.  Fall
        // through to its documented open-window path with the same exact URL.
      }
    }
    await clients.openWindow(target.href);
  })());
});
