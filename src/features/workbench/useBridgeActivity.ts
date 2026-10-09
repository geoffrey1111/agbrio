import {useEffect,useRef,useState} from "react";
import type {BridgeActivity,BridgeActivityApi} from "./bridgeActivity";
/** Visible rows register a shared cached batch. Navigation never refreshes a
 * warm batch; the foreground cache clock owns network synchronization. */
export function useBridgeActivity(ids:string[],api:BridgeActivityApi|undefined,enabled:boolean){
 const [rows,setRows]=useState<Record<string,{row:BridgeActivity;receivedAt:number}>>({}),[clock,setClock]=useState(Date.now);
 const key=ids.join("\n"),latest=useRef(ids);latest.current=ids;
 useEffect(()=>{
  if(!enabled||!api||!key)return;
  const activityApi=api;
  let alive=true,selected="",generation=0,stop:(()=>void)|undefined;
  function apply(requested:string[],data:BridgeActivity[],at:number){
   if(!alive)return;
   setRows(old=>({...Object.fromEntries(Object.entries(old).filter(([id])=>latest.current.includes(id))),...Object.fromEntries(data.filter(r=>requested.includes(r.workstreamId)).map(row=>[row.workstreamId,{row,receivedAt:at}]))}));
   setClock(Date.now());
  }
  async function selectVisible(){
   if(!alive||document.visibilityState==="hidden")return;
   const shown=new Set([...document.querySelectorAll<HTMLElement>("[data-bridge-id]")].filter(n=>{const r=n.getBoundingClientRect();return r.height>0&&r.bottom>0&&r.top<innerHeight&&r.right>0&&r.left<innerWidth;}).map(n=>n.dataset.bridgeId!));
   const requested=latest.current.filter(id=>shown.has(id)).slice(0,20),batch=JSON.stringify([...requested].sort());
   if(!requested.length||activityApi.subscribe&&batch===selected)return;
   selected=batch;const current=++generation;stop?.();stop=undefined;
   const publish=()=>{if(!alive||generation!==current)return;const data=activityApi.cached?.(requested);if(data)apply(requested,data,activityApi.observedAt?.(requested)??0);else if(activityApi.cached)setRows(old=>Object.fromEntries(Object.entries(old).filter(([id])=>!requested.includes(id))));};
   stop=activityApi.subscribe?.(requested,publish);publish();
   try{const data=await activityApi(requested);if(alive&&generation===current)apply(requested,data,activityApi.observedAt?.(requested)??Date.now());}
   catch{/* Retain old readings with their real age; never invent fresh status. */}
  }
  void selectVisible();
  // Legacy injected APIs keep their clock. Production APIs use one shared clock.
  const timer=activityApi.subscribe?undefined:setInterval(()=>void selectVisible(),5000),expire=setInterval(()=>setClock(Date.now()),1000);
  let scrollTimer:ReturnType<typeof setTimeout>|undefined;const scroll=()=>{clearTimeout(scrollTimer);scrollTimer=setTimeout(()=>void selectVisible(),120);};
  document.addEventListener("scroll",scroll,true);window.addEventListener("resize",scroll);
  const visible=()=>{if(document.visibilityState!=="hidden")void selectVisible();};document.addEventListener("visibilitychange",visible);window.addEventListener("focus",visible);
  return()=>{alive=false;generation++;stop?.();clearTimeout(scrollTimer);document.removeEventListener("scroll",scroll,true);window.removeEventListener("resize",scroll);clearInterval(timer);clearInterval(expire);document.removeEventListener("visibilitychange",visible);window.removeEventListener("focus",visible);};
 },[key,api,enabled]);
 return {rows,clock};
}
