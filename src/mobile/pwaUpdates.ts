type State={status:'IDLE'|'CHECKING'|'CURRENT'|'READY'|'UNAVAILABLE';busy:boolean;revision:string|null};
let state:State={status:'IDLE',busy:false,revision:null},writes=0,lastCheck=0;
let registration:ServiceWorkerRegistration|null=null,work:Promise<void>|null=null,started=false,recheck=false;
const listeners=new Set<()=>void>();
const publish=(next:Partial<State>)=>{state={...state,...next,busy:writes>0};for(const fn of listeners)fn();};
export const pwaUpdateSnapshot=()=>state;
export const subscribePwaUpdates=(fn:()=>void)=>{listeners.add(fn);return()=>listeners.delete(fn);};
export function beginPwaMutation(){writes++;publish({});let ended=false;return()=>{if(ended)return;ended=true;writes--;publish({});};}
export function applyPwaUpdate(reload=()=>location.reload()){if(state.status!=='READY'||writes>0)return false;reload();return true;}
function requestShell(worker:ServiceWorker):Promise<{manifest:string;revision:string|null}>{
 return new Promise((resolve,reject)=>{const channel=new MessageChannel(),id=crypto.randomUUID();const timer=setTimeout(()=>finish(Error('shell update timeout')),10000);
  const finish=(error?:Error,value?:{manifest:string;revision:string|null})=>{clearTimeout(timer);channel.port1.close();channel.port2.close();if(error)reject(error);else resolve(value!);};
  channel.port1.onmessage=event=>{const d=event.data;if(d?.id!==id)return;if(d.type!=='AIWR_SHELL_READY'||typeof d.manifest!=='string'||!/^\/assets\/[A-Za-z0-9_-]+\.json$/.test(d.manifest)){finish(Error('shell unavailable'));return;}finish(undefined,{manifest:d.manifest,revision:typeof d.revision==='string'?d.revision:null});};
  try{worker.postMessage({type:'AIWR_CHECK_SHELL',id},[channel.port2]);}catch{finish(Error('worker unavailable'));}
 });
}
export function checkPwaUpdate(force=true):Promise<void>{
 if(work)return work;if(!force&&Date.now()-lastCheck<30000)return Promise.resolve();lastCheck=Date.now();const wasReady=state.status==='READY';publish({status:'CHECKING'});
 const task=(async()=>{try{const worker=navigator.serviceWorker?.controller??registration?.active;if(!worker)throw Error('worker unavailable');const next=await requestShell(worker);const current=document.querySelector<HTMLMetaElement>('meta[name="aiwr-shell-assets"]')?.content;if(!current)throw Error('running shell identity unavailable');publish({status:next.manifest!==current?'READY':'CURRENT',revision:next.revision});}catch{publish({status:wasReady?'READY':'UNAVAILABLE'});}})();
 work=task.finally(()=>{work=null;if(recheck){recheck=false;void checkPwaUpdate();}});return work;
}
export function startPwaUpdates(next:ServiceWorkerRegistration){registration=next;if(started)return;started=true;
 const foreground=()=>{if(document.visibilityState==='visible')void checkPwaUpdate(false);};
 document.addEventListener('visibilitychange',foreground);window.addEventListener('pageshow',foreground);window.addEventListener('online',()=>void checkPwaUpdate());
 navigator.serviceWorker.addEventListener('controllerchange',()=>{if(work)recheck=true;else void checkPwaUpdate();});void checkPwaUpdate();
}
