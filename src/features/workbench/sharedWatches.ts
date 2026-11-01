import {useEffect,useRef,useState} from 'react';
import type {CodexWatch,NotificationApi} from './CodexNotifications';

/** One poller per NotificationApi, shared by the conversation list and the
 * to-do count, so adding a reader never multiplies Host reads. Reads the
 * existing watch projection: cached first, then the live list. */
type Store={rows:CodexWatch[];listeners:Set<(rows:CodexWatch[])=>void>;stop?:()=>void;poll:(force?:boolean)=>Promise<void>};
const stores=new WeakMap<NotificationApi,Store>();
function storeFor(api:NotificationApi):Store{
 const prior=stores.get(api);if(prior)return prior;
 let inFlight=false,refreshAgain=false,timer:ReturnType<typeof setTimeout>|undefined;
 const store:Store={rows:api.cachedWatches?.()??[],listeners:new Set(),poll:async()=>{}};
 const publish=(rows:CodexWatch[])=>{store.rows=rows;for(const l of store.listeners)l(rows);};
 store.poll=async(force=false)=>{
  if(!store.listeners.size||document.visibilityState==='hidden')return;
  if(inFlight){refreshAgain ||= force;return;}inFlight=true;
  try{publish(await(force&&api.refreshWatches?api.refreshWatches():api.watches()));}catch{/* keep the cached list */}
  finally{inFlight=false;clearTimeout(timer);if(store.listeners.size){if(refreshAgain){refreshAgain=false;void store.poll(true);}else timer=setTimeout(()=>void store.poll(),5000);}}
 };
 const start=()=>{
  const unsubscribe=api.subscribe?.(()=>{const cached=api.cachedWatches?.();if(cached)publish(cached);});
  const wake=()=>{if(document.visibilityState!=='hidden'){clearTimeout(timer);void store.poll(true);}};
  document.addEventListener('visibilitychange',wake);window.addEventListener('focus',wake);window.addEventListener('pageshow',wake);window.addEventListener('online',wake);
  store.stop=()=>{clearTimeout(timer);unsubscribe?.();document.removeEventListener('visibilitychange',wake);window.removeEventListener('focus',wake);window.removeEventListener('pageshow',wake);window.removeEventListener('online',wake);store.stop=undefined;};
  void store.poll(true);
 };
 (store as Store&{start:()=>void}).start=start;
 stores.set(api,store);return store;
}
export function useWatches(api?:NotificationApi){
 const [watches,setWatches]=useState<CodexWatch[]>(()=>api?(stores.get(api)?.rows??api.cachedWatches?.()??[]):[]);
 useEffect(()=>{
  if(!api)return;const store=storeFor(api);
  const listener=(rows:CodexWatch[])=>setWatches(rows);
  const first=!store.listeners.size;store.listeners.add(listener);
  setWatches(store.rows);
  if(first)(store as Store&{start:()=>void}).start();else void store.poll(true);
  return()=>{store.listeners.delete(listener);if(!store.listeners.size)store.stop?.();};
 },[api]);
 const activeApi=useRef(api);activeApi.current=api;
 const alive=useRef(true);useEffect(()=>{alive.current=true;return()=>{alive.current=false;};},[]);
 const refresh=async()=>{if(!api)return;const rows=await(api.refreshWatches?.()??api.watches());const store=stores.get(api);if(store){store.rows=rows;for(const l of store.listeners)l(rows);}else if(alive.current&&activeApi.current===api)setWatches(rows);};
 return [watches,refresh] as const;
}
