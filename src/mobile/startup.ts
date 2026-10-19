export type WebSession={authenticated:boolean;method?:"DEVICE"|"CLOUDFLARE"|null;cacheScope?:string|null};

async function requestSession():Promise<WebSession>{
 const abort=new AbortController(),timer=setTimeout(()=>abort.abort(),10000);
 try{
  const response=await fetch("/v1/mobile/auth/session",{credentials:"same-origin",cache:"no-store",signal:abort.signal});
  if(!response.ok){const body=await response.json().catch(()=>null);throw new Error(body?.error??`HTTP ${response.status}`);}
  return await response.json();
 }finally{clearTimeout(timer);}
}

// One live check starts while the application bundle downloads. No login state
// is persisted: only the first mount (including StrictMode's effect replay) uses it.
let initial:Promise<WebSession>|null=null;
export function prepareMobileSession(){
 if(initial)return;
 initial=requestSession();void initial.catch(()=>undefined);
}
export function readMobileSession():Promise<WebSession>{
 if(!initial)return requestSession();
 const current=initial;
 queueMicrotask(()=>{if(initial===current)initial=null;});
 return current;
}

// Owner-approved (2026-10-19) cache-first start. Only the opaque cache
// partition of the last live session is remembered — never a credential.
// It lets cached presentation render while the live check is in flight;
// every write waits for that live check (see `awaitLiveSession`).
const HINT="agbrio.session-hint.v1",HINT_MAX_AGE=7*24*60*60*1000;
export function readSessionHint():WebSession|null{
 try{const raw=JSON.parse(localStorage.getItem(HINT)??"null");
  if(raw&&typeof raw.cacheScope==="string"&&raw.cacheScope&&(raw.method==="DEVICE"||raw.method==="CLOUDFLARE")&&typeof raw.at==="number"&&raw.at<=Date.now()&&Date.now()-raw.at<HINT_MAX_AGE)return{authenticated:true,method:raw.method,cacheScope:raw.cacheScope};
 }catch{/* unavailable storage only disables the fast start */}
 return null;
}
export function rememberSession(next:WebSession){
 try{if(next.authenticated&&next.cacheScope)localStorage.setItem(HINT,JSON.stringify({method:next.method,cacheScope:next.cacheScope,at:Date.now()}));else localStorage.removeItem(HINT);}catch{/* best effort */}
}

let live=false,waiters:(()=>void)[]=[];
/** Called only with a live authenticated session response. */
export const sessionIsLive=()=>live;
export function markSessionLive(value:boolean){live=value;if(value){const ready=waiters;waiters=[];ready.forEach(fn=>fn());}}
/** Writes made from a cached (not yet confirmed) page wait for the live
 * session; they never reach the Host on the strength of the cache alone. */
export function awaitLiveSession(timeout=15000):Promise<void>{
 if(live)return Promise.resolve();
 return new Promise((resolve,reject)=>{const done=()=>{clearTimeout(timer);resolve();};const timer=setTimeout(()=>{waiters=waiters.filter(fn=>fn!==done);reject(new Error("ROUTER_SESSION_NOT_CONFIRMED"));},timeout);waiters.push(done);});
}
