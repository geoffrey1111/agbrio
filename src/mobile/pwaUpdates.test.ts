import {afterEach,beforeEach,expect,it,vi} from 'vitest';
let received:((e:{data:unknown})=>void)|null=null;
class FakeChannel{port1={onmessage:null as ((e:{data:unknown})=>void)|null,close:vi.fn()};port2={close:vi.fn()};constructor(){received=e=>this.port1.onmessage?.(e);}}
beforeEach(()=>{vi.resetModules();vi.stubGlobal('MessageChannel',FakeChannel);document.head.innerHTML='<meta name="aiwr-shell-assets" content="/assets/pwa-shell-old.json">';});
afterEach(()=>{vi.unstubAllGlobals();document.head.innerHTML='';});
it('checks a current live shell without requiring a worker and keeps same-origin login',async()=>{
 Object.defineProperty(navigator,'serviceWorker',{configurable:true,value:{controller:null}});
 const fetch=vi.fn(async()=>new Response('<meta name="aiwr-shell-assets" content="/assets/pwa-shell-old.json"><meta name="aiwr-shell-revision" content="2026.10.31-125">',{headers:{'content-type':'text/html'}}));vi.stubGlobal('fetch',fetch);
 const p=await import('./pwaUpdates');await p.checkPwaUpdate();expect(p.pwaUpdateSnapshot()).toMatchObject({status:'CURRENT',revision:'2026.10.31-125'});expect(fetch).toHaveBeenCalledWith('/mobile',expect.objectContaining({credentials:'same-origin',cache:'no-store'}));
});
it('reports an expired login rather than blaming the network or deleting work',async()=>{
 Object.defineProperty(navigator,'serviceWorker',{configurable:true,value:{controller:null}});vi.stubGlobal('fetch',vi.fn(async()=>new Response('login',{status:403})));sessionStorage.setItem('pwa-work-fixture','keep');
 const p=await import('./pwaUpdates');await p.checkPwaUpdate();expect(p.pwaUpdateSnapshot()).toMatchObject({status:'UNAVAILABLE',reason:'AUTH'});expect(sessionStorage.getItem('pwa-work-fixture')).toBe('keep');sessionStorage.removeItem('pwa-work-fixture');
});
it('permits preparing a changed graph beyond the old ten-second limit without reload',async()=>{
 vi.useFakeTimers();try{vi.stubGlobal('fetch',vi.fn(async()=>new Response('<meta name="aiwr-shell-assets" content="/assets/pwa-shell-new.json">')));
 const worker={postMessage:vi.fn(d=>{setTimeout(()=>received?.({data:{type:'AIWR_SHELL_READY',id:d.id,manifest:'/assets/pwa-shell-new.json',revision:'2026.10.31-125'}}),15000);})};Object.defineProperty(navigator,'serviceWorker',{configurable:true,value:{controller:worker}});
 const p=await import('./pwaUpdates');const check=p.checkPwaUpdate();await vi.advanceTimersByTimeAsync(11000);expect(p.pwaUpdateSnapshot()).toMatchObject({status:'CHECKING',preparing:true});await vi.advanceTimersByTimeAsync(4000);await check;expect(p.pwaUpdateSnapshot().status).toBe('READY');
 }finally{vi.useRealTimers();}
});
it('repairs a stale worker once without unregistering or rotating subscriptions',async()=>{
 vi.stubGlobal('fetch',vi.fn(async()=>new Response('<meta name="aiwr-shell-assets" content="/assets/pwa-shell-new.json">')));
 const fresh={postMessage:vi.fn(d=>queueMicrotask(()=>received?.({data:{type:'AIWR_SHELL_READY',id:d.id,manifest:'/assets/pwa-shell-new.json'}})))};
 const old={postMessage:vi.fn(d=>queueMicrotask(()=>received?.({data:{type:'AIWR_SHELL_UNAVAILABLE',id:d.id}})))};
 const registration={active:old,update:vi.fn(async()=>{serviceObject.controller=fresh;}),unregister:vi.fn()};const serviceObject={controller:old,getRegistration:vi.fn(async()=>registration)};Object.defineProperty(navigator,'serviceWorker',{configurable:true,value:serviceObject});
 const p=await import('./pwaUpdates');await p.checkPwaUpdate();expect(p.pwaUpdateSnapshot().status).toBe('READY');expect(registration.update).toHaveBeenCalledOnce();expect(registration.unregister).not.toHaveBeenCalled();expect(old.postMessage).toHaveBeenCalledOnce();expect(fresh.postMessage).toHaveBeenCalledOnce();
});
function service(){const worker={postMessage:vi.fn((d)=>{queueMicrotask(()=>received?.({data:{type:'AIWR_SHELL_READY',id:d.id,manifest:'/assets/pwa-shell-new.json',revision:'2026.10.09-31'}}));})};Object.defineProperty(navigator,'serviceWorker',{configurable:true,value:{controller:worker,addEventListener:vi.fn()}});return worker;}
it('prepares a changed shell without reload, preserves stored work, and blocks reload throughout a mutation',async()=>{
 const worker=service();const p=await import('./pwaUpdates');sessionStorage.setItem('aiwr-watch-draft:demo:pending','exact-request');localStorage.setItem('aiwr.role-review.v5.demo','saved-review');
 const done=p.beginPwaMutation();await p.checkPwaUpdate();expect(p.pwaUpdateSnapshot()).toMatchObject({status:'READY',busy:true});const reload=vi.fn();expect(p.applyPwaUpdate(reload)).toBe(false);expect(reload).not.toHaveBeenCalled();done();done();expect(p.applyPwaUpdate(reload)).toBe(true);expect(reload).toHaveBeenCalledOnce();expect(worker.postMessage).toHaveBeenCalledOnce();expect(sessionStorage.getItem('aiwr-watch-draft:demo:pending')).toBe('exact-request');expect(localStorage.getItem('aiwr.role-review.v5.demo')).toBe('saved-review');
 sessionStorage.removeItem('aiwr-watch-draft:demo:pending');localStorage.removeItem('aiwr.role-review.v5.demo');
});
it('recovers from no worker, does not label unknown identity current, and coalesces repeated checks',async()=>{
 Object.defineProperty(navigator,'serviceWorker',{configurable:true,value:{controller:null}});const p=await import('./pwaUpdates');await p.checkPwaUpdate();expect(p.pwaUpdateSnapshot().status).toBe('UNAVAILABLE');const worker=service();await Promise.all([p.checkPwaUpdate(),p.checkPwaUpdate()]);expect(worker.postMessage).toHaveBeenCalledOnce();expect(p.pwaUpdateSnapshot().status).toBe('READY');document.head.innerHTML='';await p.checkPwaUpdate();expect(p.pwaUpdateSnapshot().status).toBe('READY');
});
it('rechecks on a visible foreground resume with a bounded frequency, not on hidden resume',async()=>{
 const worker=service();const p=await import('./pwaUpdates');p.startPwaUpdates({active:worker} as unknown as ServiceWorkerRegistration);await p.checkPwaUpdate();worker.postMessage.mockClear();const now=Date.now();const time=vi.spyOn(Date,'now').mockReturnValue(now+31000);const visible=vi.spyOn(document,'visibilityState','get').mockReturnValue('hidden');document.dispatchEvent(new Event('visibilitychange'));expect(worker.postMessage).not.toHaveBeenCalled();visible.mockReturnValue('visible');document.dispatchEvent(new Event('visibilitychange'));await p.checkPwaUpdate(false);expect(worker.postMessage).toHaveBeenCalledOnce();window.dispatchEvent(new Event('pageshow'));expect(worker.postMessage).toHaveBeenCalledOnce();time.mockRestore();visible.mockRestore();
});
it('a mutating mobile request keeps reload blocked until the response body finishes',async()=>{
 service();const p=await import('./pwaUpdates');await p.checkPwaUpdate();let finish:(v:unknown)=>void=()=>{};vi.stubGlobal('fetch',vi.fn(async()=>({ok:true,status:200,json:()=>new Promise(r=>{finish=r;})})));(await import('./startup')).markSessionLive(true);const {request}=await import('./api');const result=request('/fixture',{method:'POST'});await vi.waitFor(()=>expect(p.pwaUpdateSnapshot().busy).toBe(true));const reload=vi.fn();expect(p.applyPwaUpdate(reload)).toBe(false);finish({ok:true});await result;expect(p.pwaUpdateSnapshot().busy).toBe(false);expect(reload).not.toHaveBeenCalled();
});
