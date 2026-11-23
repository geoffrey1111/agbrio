import {useEffect,useState} from 'react';
import type {CodexWatch,NotificationApi} from './CodexNotifications';
import {isSupersededRead} from './readModelCache';

/** One poller per NotificationApi, shared by the conversation list and the
 * to-do count, so adding a reader never multiplies Host reads. Reads the
 * existing watch projection: cached first, then the live list. */
type Snapshot={rows:CodexWatch[];failed:boolean};
const empty:Snapshot={rows:[],failed:false};
type Store={snapshot:Snapshot;listeners:Set<(snapshot:Snapshot)=>void>;start:()=>void;stop?:()=>void;refresh:()=>Promise<CodexWatch[]>};
const stores=new WeakMap<NotificationApi,Store>();
function storeFor(api:NotificationApi):Store{
 const prior=stores.get(api);if(prior)return prior;
 let inFlight:Promise<CodexWatch[]>|undefined,wokeDuringRead=false,timer:ReturnType<typeof setTimeout>|undefined;
 const store:Store={snapshot:{rows:api.cachedWatches?.()??[],failed:false},listeners:new Set(),start,refresh};
 const publish=(next:Snapshot)=>{store.snapshot=next;for(const l of store.listeners)l(next);};
 function poll(){if(store.listeners.size&&document.visibilityState!=='hidden')void refresh().catch(()=>{});}
 function refresh(){
  if(inFlight)return inFlight;
  clearTimeout(timer);
  let failed=false;
  inFlight=Promise.resolve().then(()=>api.refreshWatches?.()??api.watches()).then(rows=>{
   if(store.listeners.size)publish({rows,failed:false});return rows;
  }).catch(error=>{
   failed=true;
   if(store.listeners.size&&!isSupersededRead(error)&&!String(error).includes('ROUTER_READ_SUSPENDED'))publish({...store.snapshot,failed:true});
   throw error;
  }).finally(()=>{
   inFlight=undefined;const retry=failed&&wokeDuringRead;wokeDuringRead=false;
   if(store.listeners.size){if(retry&&document.visibilityState!=='hidden')poll();else timer=setTimeout(poll,5000);}
  });
  return inFlight;
 }
 function start(){
  if(api.cachedWatches)publish({...store.snapshot,rows:api.cachedWatches()??[]});
  const unsubscribe=api.subscribe?.(()=>{if(api.cachedWatches)publish({...store.snapshot,rows:api.cachedWatches()??[]});});
  const wake=()=>{if(document.visibilityState!=='hidden'){if(inFlight)wokeDuringRead=true;else poll();}};
  document.addEventListener('visibilitychange',wake);window.addEventListener('focus',wake);window.addEventListener('pageshow',wake);window.addEventListener('online',wake);
  store.stop=()=>{clearTimeout(timer);unsubscribe?.();document.removeEventListener('visibilitychange',wake);window.removeEventListener('focus',wake);window.removeEventListener('pageshow',wake);window.removeEventListener('online',wake);store.stop=undefined;store.snapshot=empty;wokeDuringRead=false;};
  poll();
 }
 stores.set(api,store);return store;
}
export function useWatches(api?:NotificationApi){
 const [snapshot,setSnapshot]=useState<Snapshot>(()=>api?storeFor(api).snapshot:empty);
 useEffect(()=>{
  if(!api){setSnapshot(empty);return;}const store=storeFor(api);
  const first=!store.listeners.size;store.listeners.add(setSnapshot);
  setSnapshot(store.snapshot);
  if(first)store.start();
  return()=>{store.listeners.delete(setSnapshot);if(!store.listeners.size)store.stop?.();};
 },[api]);
 const refresh=()=>api?storeFor(api).refresh():Promise.resolve([]);
 return [snapshot.rows,refresh,snapshot.failed] as const;
}
