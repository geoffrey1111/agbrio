type State={status:'IDLE'|'CHECKING'|'CURRENT'|'READY'|'UNAVAILABLE';busy:boolean;revision:string|null;reason?:'AUTH'|'WORKER'|'TIMEOUT'|'NETWORK'|'IDENTITY'|null;preparing?:boolean};
let state:State={status:'IDLE',busy:false,revision:null},writes=0,lastCheck=0;
let registration:ServiceWorkerRegistration|null=null,work:Promise<void>|null=null,started=false,recheck=false;
const listeners=new Set<()=>void>();
const publish=(next:Partial<State>)=>{state={...state,...next,busy:writes>0};for(const fn of listeners)fn();};
export const pwaUpdateSnapshot=()=>state;
export const subscribePwaUpdates=(fn:()=>void)=>{listeners.add(fn);return()=>listeners.delete(fn);};
export function beginPwaMutation(){writes++;publish({});let ended=false;return()=>{if(ended)return;ended=true;writes--;publish({});};}
export function applyPwaUpdate(reload=()=>location.reload()){if(state.status!=='READY'||writes>0)return false;reload();return true;}
function requestShell(worker:ServiceWorker):Promise<{manifest:string;revision:string|null}>{
 return new Promise((resolve,reject)=>{const channel=new MessageChannel(),id=crypto.randomUUID();const timer=setTimeout(()=>finish(Error('shell update timeout')),40000);
  const finish=(error?:Error,value?:{manifest:string;revision:string|null})=>{clearTimeout(timer);channel.port1.close();channel.port2.close();if(error)reject(error);else resolve(value!);};
  channel.port1.onmessage=event=>{const d=event.data;if(d?.id!==id)return;if(d.type==='AIWR_SHELL_UNAVAILABLE'&&['AUTH','TIMEOUT','NETWORK','IDENTITY'].includes(d.reason)){finish(Error(d.reason));return;}if(d.type!=='AIWR_SHELL_READY'||typeof d.manifest!=='string'||!/^\/assets\/[A-Za-z0-9_-]+\.json$/.test(d.manifest)){finish(Error('shell unavailable'));return;}finish(undefined,{manifest:d.manifest,revision:typeof d.revision==='string'?d.revision:null});};
  try{worker.postMessage({type:'AIWR_CHECK_SHELL',id},[channel.port2]);}catch{finish(Error('worker unavailable'));}
 });
}
async function liveShell():Promise<{manifest:string;revision:string|null}>{
 const abort=new AbortController();const timer=setTimeout(()=>abort.abort(),12000);
 try{const response=await fetch('/mobile',{cache:'no-store',credentials:'same-origin',signal:abort.signal});
  if(response.status===401||response.status===403||response.redirected)throw Error('AUTH');if(!response.ok)throw Error('NETWORK');
  const html=await response.text();const manifest=html.match(/name="aiwr-shell-assets" content="(\/assets\/[A-Za-z0-9_-]+\.json)"/)?.[1];
  if(!manifest)throw Error('IDENTITY');return {manifest,revision:html.match(/name="aiwr-shell-revision" content="([0-9.-]+)"/)?.[1]??null};
 }finally{clearTimeout(timer);}
}
function updateReason(error:unknown):NonNullable<State['reason']>{const value=String(error);return /AUTH/.test(value)?'AUTH':/timeout|AbortError/i.test(value)?'TIMEOUT':/worker|shell unavailable/i.test(value)?'WORKER':/IDENTITY/.test(value)?'IDENTITY':'NETWORK';}
export function checkPwaUpdate(force=true):Promise<void>{
 if(work)return work;if(!force&&Date.now()-lastCheck<30000)return Promise.resolve();lastCheck=Date.now();const wasReady=state.status==='READY';publish({status:'CHECKING',reason:null,preparing:false});
 const task=(async()=>{let liveError:unknown;try{
  const current=document.querySelector<HTMLMetaElement>('meta[name="aiwr-shell-assets"]')?.content;if(!current)throw Error('IDENTITY');
  try{const latest=await liveShell();if(latest.manifest===current){publish({status:'CURRENT',revision:latest.revision});return;}}catch(error){liveError=error;}
  let worker=navigator.serviceWorker?.controller??registration?.active;
  if(!worker){registration??=await navigator.serviceWorker?.getRegistration?.()??null;worker=navigator.serviceWorker?.controller??registration?.active;}
  if(!worker)throw liveError??Error('worker unavailable');
  publish({preparing:true});let next;
  try{next=await requestShell(worker);}catch(error){
   // Revalidate a legacy/stale worker once; keep its PushSubscription and all
   // pairing/draft state. Never unregister or clear storage to repair updates.
   registration??=await navigator.serviceWorker?.getRegistration?.()??null;
   if(!registration?.update)throw liveError??error;
   let timer:ReturnType<typeof setTimeout>|undefined;try{await Promise.race([registration.update(),new Promise<never>((_,reject)=>{timer=setTimeout(()=>reject(Error('worker update timeout')),8000);})]);}finally{clearTimeout(timer);}
   const fresh=navigator.serviceWorker?.controller??registration.active;
   if(!fresh||fresh===worker)throw liveError??error;next=await requestShell(fresh);
  }
  publish({status:next.manifest!==current?'READY':'CURRENT',revision:next.revision,preparing:false,reason:null});
 }catch(error){publish({status:wasReady?'READY':'UNAVAILABLE',reason:updateReason(error),preparing:false});}})();
 work=task.finally(()=>{work=null;if(recheck){recheck=false;void checkPwaUpdate();}});return work;
}

export function startPwaUpdates(next:ServiceWorkerRegistration){registration=next;if(started)return;started=true;
 const foreground=()=>{if(document.visibilityState==='visible')void checkPwaUpdate(false);};
 document.addEventListener('visibilitychange',foreground);window.addEventListener('pageshow',foreground);window.addEventListener('online',()=>void checkPwaUpdate());
 navigator.serviceWorker.addEventListener('controllerchange',()=>{if(work)recheck=true;else void checkPwaUpdate();});void checkPwaUpdate();
}
