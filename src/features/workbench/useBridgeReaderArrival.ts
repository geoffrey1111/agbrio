import {useLayoutEffect,useRef,type RefObject} from "react";

/** Arrival follows the newest message, not an old persisted scroll offset.
 * Once the reader deliberately moves, polling and late media preserve that view. */
export function useBridgeReaderArrival({panel,enabled,visitKey,contentKey,roles,paused=false}:{panel:RefObject<HTMLElement|null>;enabled:boolean;visitKey:string;contentKey:string;roles:readonly string[];paused?:boolean}){
 const visits=useRef(new Map<string,{key:string;following:boolean}>());
 const blocked=useRef(paused);blocked.current=paused;
 const rolesKey=roles.join(":");
 useLayoutEffect(()=>{
  if(!enabled){visits.current.clear();return;}
  if(panel.current?.closest("[hidden]"))return;
  const cleanup:Array<()=>void>=[];
  for(const role of rolesKey.split(":")){
   const reader=panel.current?.querySelector<HTMLElement>(`#role-reader-${role} .v4-role-original`);
   if(!reader||!reader.querySelector(".r2-bridge-message"))continue;
   let visit=visits.current.get(role);
   if(visit?.key!==visitKey){visit={key:visitKey,following:true};visits.current.set(role,visit);}
   const current=visit;
   let frame:number|undefined;
   const align=()=>{if(!current.following||blocked.current||document.visibilityState==="hidden"||panel.current?.closest("[hidden]"))return;const rows=reader.querySelectorAll<HTMLElement>(".r2-bridge-message");const latest=rows[rows.length-1];if(!latest)return;reader.scrollTop=Math.max(0,reader.scrollTop+latest.getBoundingClientRect().top-reader.getBoundingClientRect().top-12);};
   const schedule=()=>{if(frame!==undefined)cancelAnimationFrame(frame);frame=requestAnimationFrame(()=>{frame=undefined;align();});};
   const stop=()=>{current.following=false;};
   const keys=(event:KeyboardEvent)=>{if(["ArrowUp","ArrowDown","PageUp","PageDown","Home","End"," "].includes(event.key))stop();};
   align();schedule();
   const observer=typeof ResizeObserver!=="undefined"?new ResizeObserver(schedule):undefined;
   for(const message of reader.querySelectorAll<HTMLElement>(".r2-bridge-message"))observer?.observe(message);
   observer?.observe(reader);
   reader.addEventListener("wheel",stop,{passive:true});reader.addEventListener("touchstart",stop,{passive:true});reader.addEventListener("pointerdown",stop,{passive:true});reader.addEventListener("keydown",keys);
   document.addEventListener("visibilitychange",schedule);
   cleanup.push(()=>{if(frame!==undefined)cancelAnimationFrame(frame);observer?.disconnect();reader.removeEventListener("wheel",stop);reader.removeEventListener("touchstart",stop);reader.removeEventListener("pointerdown",stop);reader.removeEventListener("keydown",keys);document.removeEventListener("visibilitychange",schedule);});
  }
  return()=>{for(const remove of cleanup)remove();};
 },[panel,enabled,visitKey,contentKey,rolesKey,paused]);
 return(role:string)=>{const visit=visits.current.get(role);if(visit)visit.following=false;};
}
