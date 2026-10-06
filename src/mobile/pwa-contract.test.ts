import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";

const manifest = JSON.parse(readFileSync(resolve(process.cwd(), "public/manifest.webmanifest"), "utf8"));
const workerSource = readFileSync(resolve(process.cwd(), "public/service-worker.js"), "utf8");

describe("MOBILE_WEB_PUSH_V1 PWA contract", () => {
  it("routes independent watch notifications to the exact event and rejects external click targets", async () => {
    const handlers=new Map<string,(event:any)=>void>(),showNotification=vi.fn().mockResolvedValue(undefined),openWindow=vi.fn().mockResolvedValue(undefined),fetch=vi.fn();
    vi.stubGlobal("self",{addEventListener:(name:string,handler:(event:any)=>void)=>handlers.set(name,handler),registration:{showNotification},location:{origin:"https://router.example"}});
    vi.stubGlobal("clients",{matchAll:vi.fn().mockResolvedValue([]),openWindow});vi.stubGlobal("fetch",fetch);new Function(workerSource)();
    let pending:Promise<unknown>|undefined;
    handlers.get("push")!({data:{json:()=>({type:"codex_watch",eventId:"aiwr-host-42",sequence:42,label:"执行对话",state:"FAILED"})},waitUntil:(p:Promise<unknown>)=>{pending=p;}});await pending;
    expect(showNotification).toHaveBeenCalledWith("Agbrio",expect.objectContaining({body:"执行对话 · 执行失败",data:{target:"/mobile/notifications?event=42"},tag:"codex-watch-aiwr-host-42"}));expect(fetch).not.toHaveBeenCalled();
    handlers.get("notificationclick")!({notification:{close:vi.fn(),data:{target:"/mobile/notifications?event=42"}},waitUntil:(p:Promise<unknown>)=>{pending=p;}});await pending;
    expect(openWindow).toHaveBeenLastCalledWith("https://router.example/mobile/notifications?event=42");
    handlers.get("notificationclick")!({notification:{close:vi.fn(),data:{target:"https://evil.example/mobile/notifications?event=42"}},waitUntil:(p:Promise<unknown>)=>{pending=p;}});await pending;
    expect(openWindow).toHaveBeenLastCalledWith("https://router.example/mobile");
  });
  afterEach(() => vi.unstubAllGlobals());

  it("has the installable mobile manifest contract", () => {
    expect(manifest).toMatchObject({
      id: "/mobile",
      name: "Agbrio · Agent Bridge",
      short_name: "Agbrio",
      start_url: "/mobile",
      display: "standalone",
    });
    expect(manifest.icons).toEqual(expect.arrayContaining([expect.objectContaining({ src: "/icon.ico" })]));
  });

  it("replaces the legacy worker promptly when the PWA is opened after an upgrade", async () => {
    const handlers = new Map<string, (event: any) => void>();
    const skipWaiting = vi.fn(() => Promise.resolve());
    const claim = vi.fn(() => Promise.resolve());
    vi.stubGlobal("self", {
      addEventListener: (name: string, handler: (event: any) => void) => handlers.set(name, handler),
      skipWaiting,
    });
    vi.stubGlobal("clients", { claim });
    new Function(workerSource)();

    let installCompletion: Promise<unknown> | undefined;
    handlers.get("install")!({ waitUntil: (promise: Promise<unknown>) => { installCompletion = promise; } });
    await installCompletion;
    expect(skipWaiting).toHaveBeenCalledOnce();

    let activateCompletion: Promise<unknown> | undefined;
    handlers.get("activate")!({ waitUntil: (promise: Promise<unknown>) => { activateCompletion = promise; } });
    await activateCompletion;
    expect(claim).toHaveBeenCalledOnce();
  });

  it("shows a privacy-safe reply notification and navigates the exact deep link", async () => {
    const handlers = new Map<string, (event: any) => void>();
    const showNotification = vi.fn(() => Promise.resolve());
    const fetch = vi.fn(() => Promise.resolve({ ok: true }));
    const navigate = vi.fn(() => Promise.resolve({ focus }));
    const focus = vi.fn(() => Promise.resolve());
    vi.stubGlobal("self", {
      addEventListener: (name: string, handler: (event: any) => void) => handlers.set(name, handler),
      registration: { showNotification },
      location: { origin: "https://router.example" },
    });
    vi.stubGlobal("clients", { matchAll: vi.fn(() => Promise.resolve([{ url: "https://router.example/mobile", navigate, focus }])) });
    vi.stubGlobal("fetch", fetch);
    new Function(workerSource)();

    let pushCompletion: Promise<unknown> | undefined;
    handlers.get("push")!({
      data: { json: () => ({ type: "chatgpt_reply", workstreamId: "ws-1", observationId: "reply-1", workstreamName: "Release review" }) },
      waitUntil: (promise: Promise<unknown>) => { pushCompletion = promise; },
    });
    await pushCompletion;
    expect(showNotification).toHaveBeenCalledWith("Agbrio", expect.objectContaining({
      body: "Release review · ChatGPT 有新回复",
      data: { target: "/mobile?workstream=ws-1&reply=reply-1" },
      tag: "chatgpt-reply-reply-1",
    }));
    expect(workerSource).not.toContain("fullReply");
    expect(fetch).toHaveBeenCalledWith("/v1/mobile/push/rendered", expect.objectContaining({
      method: "POST",
      credentials: "same-origin",
      body: JSON.stringify({ workstreamId: "ws-1", observationId: "reply-1" }),
    }));

    let clickCompletion: Promise<unknown> | undefined;
    handlers.get("notificationclick")!({
      notification: { close: vi.fn(), data: { target: "/mobile?workstream=ws-1&reply=reply-1" } },
      waitUntil: (promise: Promise<unknown>) => { clickCompletion = promise; },
    });
    await clickCompletion;
    expect(navigate).toHaveBeenCalledWith("https://router.example/mobile?workstream=ws-1&reply=reply-1");
    expect(focus).toHaveBeenCalled();
  });

  it("identifies a Codex reply without exposing its text and retains its exact deep link", async () => {
    const handlers = new Map<string, (event: any) => void>();
    const showNotification = vi.fn(() => Promise.resolve());
    const fetch = vi.fn(() => Promise.resolve({ ok: true }));
    vi.stubGlobal("self", {
      addEventListener: (name: string, handler: (event: any) => void) => handlers.set(name, handler),
      registration: { showNotification },
      location: { origin: "https://router.example" },
    });
    vi.stubGlobal("fetch", fetch);
    new Function(workerSource)();

    let pushCompletion: Promise<unknown> | undefined;
    handlers.get("push")!({
      data: { json: () => ({ type: "codex_reply", workstreamId: "ws-game", observationId: "codex-reply-9", workstreamName: "当前游戏开发" }) },
      waitUntil: (promise: Promise<unknown>) => { pushCompletion = promise; },
    });
    await pushCompletion;

    expect(showNotification).toHaveBeenCalledWith("Agbrio", expect.objectContaining({
      body: "当前游戏开发 · Codex 有新回复",
      data: { target: "/mobile?workstream=ws-game&reply=codex-reply-9" },
      tag: "codex-reply-codex-reply-9",
    }));
    expect(fetch).toHaveBeenCalledWith("/v1/mobile/push/rendered", expect.objectContaining({
      body: JSON.stringify({ workstreamId: "ws-game", observationId: "codex-reply-9" }),
    }));
  });

  it("opens the same exact deep link when an existing PWA client cannot navigate", async () => {
    const handlers = new Map<string, (event: any) => void>();
    const navigate = vi.fn(() => Promise.resolve(null));
    const openWindow = vi.fn(() => Promise.resolve());
    vi.stubGlobal("self", {
      addEventListener: (name: string, handler: (event: any) => void) => handlers.set(name, handler),
      registration: { showNotification: vi.fn(() => Promise.resolve()) },
      location: { origin: "https://router.example" },
    });
    vi.stubGlobal("clients", {
      matchAll: vi.fn(() => Promise.resolve([{ url: "https://router.example/mobile", navigate }])),
      openWindow,
    });
    new Function(workerSource)();

    let clickCompletion: Promise<unknown> | undefined;
    handlers.get("notificationclick")!({
      notification: { close: vi.fn(), data: { target: "/mobile?workstream=ws-1&reply=reply-1" } },
      waitUntil: (promise: Promise<unknown>) => { clickCompletion = promise; },
    });
    await clickCompletion;
    expect(openWindow).toHaveBeenCalledWith("https://router.example/mobile?workstream=ws-1&reply=reply-1");
  });
});

it("uses only the public shell for a tunnel 530/offline and never caches authenticated API",async()=>{
 const handlers=new Map<string,(event:any)=>void>();const shell=new Response("<html>public reconnect shell</html>",{headers:{"content-type":"text/html"}});
 const cached=vi.fn().mockResolvedValue(shell),put=vi.fn(),fetch=vi.fn().mockResolvedValue(new Response("Error 1033",{status:530}));
 vi.stubGlobal("self",{addEventListener:(n:string,h:(e:any)=>void)=>handlers.set(n,h),location:{origin:"https://router.example"}});vi.stubGlobal("caches",{match:cached,open:vi.fn().mockResolvedValue({match:cached,put})});vi.stubGlobal("fetch",fetch);new Function(workerSource)();
 let pending:Promise<Response>|undefined;const respondWith=(p:Promise<Response>)=>{pending=p;};
 handlers.get("fetch")!({request:{url:"https://router.example/mobile/notifications?event=42",method:"GET",mode:"navigate"},respondWith,waitUntil:vi.fn()});expect(await (await pending!).text()).toContain("reconnect shell");
 fetch.mockRejectedValue(new TypeError("offline"));handlers.get("fetch")!({request:{url:"https://router.example/mobile",method:"GET",mode:"navigate"},respondWith,waitUntil:vi.fn()});expect(await pending).toBe(shell);
 pending=undefined;handlers.get("fetch")!({request:{url:"https://router.example/v1/mobile/auth/session",method:"GET",mode:"cors"},respondWith});expect(pending).toBeUndefined();expect(put).not.toHaveBeenCalled();
 cached.mockResolvedValue(undefined);fetch.mockResolvedValue(new Response("Access denied",{status:403}));handlers.get("fetch")!({request:{url:"https://router.example/mobile",method:"GET",mode:"navigate"},respondWith,waitUntil:vi.fn()});expect((await pending!).status).toBe(403);vi.unstubAllGlobals();
});

it("bounds a stalled navigation and falls back without caching private responses",async()=>{
 vi.useFakeTimers();const handlers=new Map<string,(event:any)=>void>();const shell=new Response("public shell");vi.stubGlobal("self",{addEventListener:(n:string,h:(e:any)=>void)=>handlers.set(n,h),location:{origin:"https://router.example"}});vi.stubGlobal("caches",{match:vi.fn().mockResolvedValue(shell)});vi.stubGlobal("fetch",vi.fn((_:unknown,options:{signal:AbortSignal})=>new Promise((_,reject)=>options.signal.addEventListener("abort",()=>reject(new Error("aborted"))))));new Function(workerSource)();let pending:Promise<Response>|undefined;handlers.get("fetch")!({request:{url:"https://router.example/mobile",method:"GET",mode:"navigate"},respondWith:(p:Promise<Response>)=>pending=p,waitUntil:vi.fn()});expect(await pending).toBe(shell);await vi.advanceTimersByTimeAsync(8000);vi.useRealTimers();vi.unstubAllGlobals();
});

it("activates even when public-shell install/precache hangs",async()=>{
 vi.useFakeTimers();const handlers=new Map<string,(event:any)=>void>(),skipWaiting=vi.fn().mockResolvedValue(undefined);vi.stubGlobal("self",{addEventListener:(n:string,h:(e:any)=>void)=>handlers.set(n,h),skipWaiting});vi.stubGlobal("fetch",vi.fn(()=>new Promise(()=>{})));new Function(workerSource)();let pending:Promise<unknown>|undefined;handlers.get("install")!({waitUntil:(p:Promise<unknown>)=>pending=p});await vi.advanceTimersByTimeAsync(8000);await pending;expect(skipWaiting).toHaveBeenCalledOnce();vi.useRealTimers();vi.unstubAllGlobals();
});

it("returns a warm public shell before network and preserves it when new dependencies fail",async()=>{
 const handlers=new Map<string,(event:any)=>void>(),shell=new Response("old public shell");
 const put=vi.fn(),cached=vi.fn().mockResolvedValue(shell);
 let release!:(response:Response)=>void;
 const fetch=vi.fn().mockImplementationOnce(()=>new Promise<Response>(resolve=>release=resolve))
  .mockImplementation(async(path:string)=>path.endsWith('.json')?new Response(JSON.stringify(['/assets/entry-new.js','/assets/main-new.js'])):path.includes('main-new')?new Response('unavailable',{status:503}):new Response('public script'));
 vi.stubGlobal("self",{addEventListener:(n:string,h:(e:any)=>void)=>handlers.set(n,h),location:{origin:"https://router.example"}});
 vi.stubGlobal("caches",{match:cached,open:vi.fn().mockResolvedValue({put})});vi.stubGlobal("fetch",fetch);new Function(workerSource)();
 let foreground!:Promise<Response>,background!:Promise<unknown>;
 handlers.get("fetch")!({request:{url:"https://router.example/mobile/notifications?event=42",method:"GET",mode:"navigate"},respondWith:(p:Promise<Response>)=>foreground=p,waitUntil:(p:Promise<unknown>)=>background=p});
 expect(await foreground).toBe(shell);expect(fetch).toHaveBeenCalledTimes(1);
 release(new Response('<html><meta name="aiwr-shell-assets" content="/assets/pwa-shell-new.json"><script src="/assets/entry-new.js"></script></html>',{headers:{'content-type':'text/html'}}));
 await background;expect(put).not.toHaveBeenCalled();
});

it("refuses private paths in a public dependency manifest without fetching them",async()=>{
 const handlers=new Map<string,(event:any)=>void>(),put=vi.fn();
 const fetch=vi.fn().mockImplementation(async(input:string|{url:string})=>{const path=typeof input==='string'?input:input.url;return path.endsWith('.json')?new Response(JSON.stringify(['/v1/mobile/auth/session'])):new Response('<html><meta name="aiwr-shell-assets" content="/assets/pwa-shell-new.json"></html>',{headers:{'content-type':'text/html'}});});
 vi.stubGlobal("self",{addEventListener:(n:string,h:(e:any)=>void)=>handlers.set(n,h),location:{origin:"https://router.example"}});
 vi.stubGlobal("caches",{match:vi.fn().mockResolvedValue(new Response('old shell')),open:vi.fn().mockResolvedValue({put})});vi.stubGlobal("fetch",fetch);new Function(workerSource)();
 let background!:Promise<unknown>;handlers.get("fetch")!({request:{url:"https://router.example/mobile",method:"GET",mode:"navigate"},respondWith:()=>{},waitUntil:(p:Promise<unknown>)=>background=p});
 await background;expect(fetch.mock.calls.some(call=>String(call[0]).includes('/v1/'))).toBe(false);expect(put).not.toHaveBeenCalled();
});

it("caches a complete dynamic resource version before replacing its public HTML",async()=>{
 const handlers=new Map<string,(event:any)=>void>(),put=vi.fn().mockResolvedValue(undefined);
 const fetch=vi.fn().mockImplementation(async(input:string|{url:string})=>{const path=typeof input==='string'?input:input.url;return path.endsWith('.json')?new Response(JSON.stringify(['/assets/entry-new.js','/assets/main-new.js','/assets/main-new.css'])):path.includes('/assets/')?new Response('public resource'):new Response('<html><meta name="aiwr-shell-assets" content="/assets/pwa-shell-new.json"><script src="/assets/entry-new.js"></script></html>',{headers:{'content-type':'text/html'}});});
 vi.stubGlobal("self",{addEventListener:(n:string,h:(e:any)=>void)=>handlers.set(n,h),location:{origin:"https://router.example"}});
 vi.stubGlobal("caches",{match:vi.fn().mockResolvedValue(new Response('old shell')),open:vi.fn().mockResolvedValue({put})});vi.stubGlobal("fetch",fetch);new Function(workerSource)();
 let background!:Promise<unknown>;handlers.get("fetch")!({request:{url:"https://router.example/mobile",method:"GET",mode:"navigate"},respondWith:()=>{},waitUntil:(p:Promise<unknown>)=>background=p});
 await background;expect(put.mock.calls.map(call=>call[0])).toEqual(['/assets/entry-new.js','/assets/main-new.js','/assets/main-new.css','/mobile']);
});

it("does not re-download a complete unchanged version during a warm navigation",async()=>{
 const html='<html><meta name="aiwr-shell-assets" content="/assets/pwa-shell-same.json"><script src="/assets/entry-same.js"></script></html>';
 const handlers=new Map<string,(event:any)=>void>(),put=vi.fn().mockResolvedValue(undefined),match=vi.fn().mockImplementation(()=>Promise.resolve(new Response(html)));
 const fetch=vi.fn().mockResolvedValue(new Response(html,{headers:{'content-type':'text/html'}}));
 vi.stubGlobal("self",{addEventListener:(n:string,h:(e:any)=>void)=>handlers.set(n,h),location:{origin:"https://router.example"}});
 vi.stubGlobal("caches",{match,open:vi.fn().mockResolvedValue({match,put})});vi.stubGlobal("fetch",fetch);new Function(workerSource)();
 let background!:Promise<unknown>;handlers.get("fetch")!({request:{url:"https://router.example/mobile",method:"GET",mode:"navigate"},respondWith:()=>{},waitUntil:(p:Promise<unknown>)=>background=p});
 await background;expect(fetch).toHaveBeenCalledOnce();expect(put.mock.calls.map(call=>call[0])).toEqual(['/mobile']);
});

it("refreshes an old shell from the network rather than recycling HTTP-cached navigation",async()=>{
 const headers={'content-type':'text/html'};
 const old='<html>old shell</html>',next='<html>current shell</html>';
 let stored=new Response(old,{headers});
 const handlers=new Map<string,(event:any)=>void>();
 const match=vi.fn(async()=>stored.clone());
 const put=vi.fn(async(_key:string,response:Response)=>{stored=response.clone();});
 const fetch=vi.fn(async(_request:unknown,options:{cache?:string})=>new Response(options.cache==='no-store'?next:old,{headers}));
 vi.stubGlobal('self',{addEventListener:(n:string,h:(e:any)=>void)=>handlers.set(n,h),location:{origin:'https://router.example'}});
 vi.stubGlobal('caches',{match,open:vi.fn(async()=>({match,put}))});vi.stubGlobal('fetch',fetch);
 new Function(workerSource)();
 let foreground!:Promise<Response>,background!:Promise<unknown>;
 handlers.get('fetch')!({request:{url:'https://router.example/mobile',method:'GET',mode:'navigate'},respondWith:(p:Promise<Response>)=>foreground=p,waitUntil:(p:Promise<unknown>)=>background=p});
 expect(await (await foreground).text()).toBe(old);
 await background;expect(await stored.text()).toBe(next);
});

it("does not serve a late legacy worker's cached shell over the new layout",async()=>{
 const handlers=new Map<string,(event:any)=>void>();
 const legacy=new Response('legacy layout'),current=new Response('corrected layout');
 const cacheContents=new Map([['aiwr-public-shell-v9',legacy],['aiwr-public-shell-v12',current]]);
 vi.stubGlobal('self',{addEventListener:(n:string,h:(e:any)=>void)=>handlers.set(n,h),location:{origin:'https://router.example'}});
 vi.stubGlobal('caches',{match:vi.fn(async(_key:string,options:{cacheName:string})=>cacheContents.get(options.cacheName))});
 vi.stubGlobal('fetch',vi.fn(async()=>new Response('network unavailable',{status:503})));
 new Function(workerSource)();
 let foreground!:Promise<Response>,background!:Promise<unknown>;
 handlers.get('fetch')!({request:{url:'https://router.example/mobile',method:'GET',mode:'navigate'},respondWith:(p:Promise<Response>)=>foreground=p,waitUntil:(p:Promise<unknown>)=>background=p});
 cacheContents.set('aiwr-public-shell-v9',new Response('late obsolete layout'));
 expect(await (await foreground).text()).toBe('corrected layout');await background;
});

it("falls through to current network HTML when new-version precache stalls, without using the legacy cache",async()=>{
 vi.useFakeTimers();
 try{
  const handlers=new Map<string,(event:any)=>void>(),skipWaiting=vi.fn(async()=>{}),put=vi.fn();
  const html='<html><script src="/assets/pending.js"></script></html>';
  const cacheNames:string[]=[];
  const fetch=vi.fn(async(input:string|{url:string},options:{signal:AbortSignal})=>{
   if(typeof input==='string'&&input.includes('/assets/'))return await new Promise<Response>((_,reject)=>options.signal.addEventListener('abort',()=>reject(new Error('slow asset'))));
   return new Response(html,{headers:{'content-type':'text/html'}});
  });
  vi.stubGlobal('self',{addEventListener:(n:string,h:(e:any)=>void)=>handlers.set(n,h),location:{origin:'https://router.example'},skipWaiting});
  vi.stubGlobal('caches',{open:vi.fn(async()=>({match:vi.fn(async()=>undefined),put})),match:vi.fn(async(_key:string,options:{cacheName:string})=>{cacheNames.push(options.cacheName);return options.cacheName==='aiwr-public-shell-v9'?new Response('obsolete layout'):undefined;})});
  vi.stubGlobal('fetch',fetch);new Function(workerSource)();
  let install!:Promise<unknown>;
  handlers.get('install')!({waitUntil:(p:Promise<unknown>)=>install=p});await vi.advanceTimersByTimeAsync(8000);await install;
  expect(skipWaiting).toHaveBeenCalledOnce();expect(put).not.toHaveBeenCalled();
  let foreground!:Promise<Response>,background!:Promise<unknown>;
  handlers.get('fetch')!({request:{url:'https://router.example/mobile',method:'GET',mode:'navigate'},respondWith:(p:Promise<Response>)=>foreground=p,waitUntil:(p:Promise<unknown>)=>background=p});
  expect(await (await foreground).text()).toBe(html);expect(cacheNames).toEqual(['aiwr-public-shell-v12']);
  await vi.advanceTimersByTimeAsync(8000);await background;
 }finally{vi.useRealTimers();vi.unstubAllGlobals();}
});
