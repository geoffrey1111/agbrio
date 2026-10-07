import {useEffect,useRef,type RefObject} from "react";
/** An unread card must be half visible for 600ms in the foreground. Scrolling
 * past a sliver, opening another tab or backgrounding never acknowledges it. */
export function useNotificationSeen(container:RefObject<HTMLDivElement|null>,enabled:boolean,cards:number[],unread:number[],mark:((sequence:number)=>Promise<unknown>)|undefined,onSeen:(sequence:number)=>void,onError:(error:unknown)=>void){
 const latest=useRef({mark,onSeen,onError,allowed:new Set(unread)});latest.current={mark,onSeen,onError,allowed:new Set(unread)};
 const mounted=useRef(true);useEffect(()=>{mounted.current=true;return()=>{mounted.current=false;};},[]);
 const signature=cards.join(',');
 useEffect(()=>{
  if(!enabled||!mark||!container.current||typeof IntersectionObserver==='undefined')return;
  let live=true;const timers=new Map<Element,ReturnType<typeof setTimeout>>(),inflight=new Set<number>();
  const nodes=[...container.current.querySelectorAll<HTMLElement>('[data-notification-sequence]')];
  const clear=(node:Element)=>{const timer=timers.get(node);if(timer)clearTimeout(timer);timers.delete(node);};
  const observer=new IntersectionObserver(entries=>{for(const entry of entries){
   const seq=Number((entry.target as HTMLElement).dataset.notificationSequence);clear(entry.target);
   if(!live||document.visibilityState!=='visible'||!entry.isIntersecting||entry.intersectionRatio<.5||!latest.current.allowed.has(seq)||inflight.has(seq))continue;
   timers.set(entry.target,setTimeout(()=>{timers.delete(entry.target);if(!live||document.visibilityState!=='visible'||!latest.current.allowed.has(seq))return;inflight.add(seq);
    void latest.current.mark?.(seq).then(()=>{if(mounted.current)latest.current.onSeen(seq);if(live)observer.unobserve(entry.target);}).catch(error=>{inflight.delete(seq);if(mounted.current)latest.current.onError(error);});
   },600));
  }},{threshold:[0,.5]});
  const visible=()=>{for(const node of nodes)clear(node);observer.disconnect();if(document.visibilityState==='visible')for(const node of nodes)observer.observe(node);};
  visible();document.addEventListener('visibilitychange',visible);
  return()=>{live=false;for(const node of nodes)clear(node);observer.disconnect();document.removeEventListener('visibilitychange',visible);};
 },[container,enabled,signature,mark]);
}
