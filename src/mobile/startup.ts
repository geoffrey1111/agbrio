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
