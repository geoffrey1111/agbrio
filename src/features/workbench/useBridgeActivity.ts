import {useEffect,useRef,useState} from "react";
import type {BridgeActivity,BridgeActivityApi} from "./bridgeActivity";
/** Only visible rows are polled. Background tabs stop; generations reject late
 * responses after navigation or list changes. Status expires instead of guessing. */
export function useBridgeActivity(ids:string[],api:BridgeActivityApi|undefined,enabled:boolean){
 const [rows,setRows]=useState<Record<string,{row:BridgeActivity;receivedAt:number}>>({}),[clock,setClock]=useState(Date.now);
 const key=ids.join("\n"),latest=useRef(ids);latest.current=ids;
 useEffect(()=>{
  if(!enabled||!api||!key)return;let alive=true,busy=false;
  async function poll(){if(!alive||busy||document.visibilityState==="hidden")return;busy=true;
   try{const shown=new Set([...document.querySelectorAll<HTMLElement>("[data-bridge-id]")].filter(n=>{const r=n.getBoundingClientRect();return r.height>0&&r.bottom>0&&r.top<innerHeight&&r.right>0&&r.left<innerWidth;}).map(n=>n.dataset.bridgeId!));const requested=latest.current.filter(id=>shown.has(id)).slice(0,20);if(!requested.length)return;const rows=await api!(requested);if(alive){const at=Date.now();setRows(old=>({...Object.fromEntries(Object.entries(old).filter(([id])=>latest.current.includes(id))),...Object.fromEntries(rows.filter(r=>requested.includes(r.workstreamId)).map(row=>[row.workstreamId,{row,receivedAt:at}]))}));setClock(at);}}
   catch{if(alive)setRows({});}finally{busy=false;}
  }
  void poll();const timer=setInterval(()=>void poll(),5000),expire=setInterval(()=>setClock(Date.now()),1000);
  let scrollTimer:ReturnType<typeof setTimeout>|undefined;const scroll=()=>{clearTimeout(scrollTimer);scrollTimer=setTimeout(()=>void poll(),120);};document.addEventListener("scroll",scroll,true);
  const visible=()=>{if(document.visibilityState!=="hidden")void poll();};document.addEventListener("visibilitychange",visible);window.addEventListener("focus",visible);
  return()=>{alive=false;clearTimeout(scrollTimer);document.removeEventListener("scroll",scroll,true);clearInterval(timer);clearInterval(expire);document.removeEventListener("visibilitychange",visible);window.removeEventListener("focus",visible);};
 },[key,api,enabled]);
 return {rows,clock};
}
