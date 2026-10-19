import {useLayoutEffect,useRef,type RefObject} from "react";
import {motion,useReducedMotion} from "motion/react";

/** Adapted motion tuning from Caho1/open-claude-ui (MIT), pinned in
 * Docs/references/OPEN_CLAUDE_UI_MOTION.md. Never owns navigation or data reads. */
export function SelectionIndicator({active,group}:{active:boolean;group:string}){
 const reduced=useReducedMotion();
 return active?<motion.span aria-hidden="true" className="agbrio-selection" layoutId={`selection-${group}`} initial={false}
  transition={reduced?{duration:0}:{type:"spring",stiffness:500,damping:35}}/>:null;
}

/** Animate an explicit view change, not a poll, mount, or refreshed message.
 * Keep the same DOM/scroll/draft owners; a rapid next navigation cancels motion. */
export function useSurfaceTransition(ref:RefObject<HTMLElement|null>,key:string,detail:boolean,keyboard:boolean){
 const previous=useRef<{key:string;detail:boolean}|null>(null);
 useLayoutEffect(()=>{
  const before=previous.current;previous.current={key,detail};
  if(!before||before.key===key||keyboard)return;
  const element=ref.current,media=window.matchMedia("(prefers-reduced-motion: reduce)");
  if(!element?.animate||media.matches)return;
  // Opacity keeps fixed/sticky descendants in their existing coordinate space.
  // A transform on this scroll owner would reparent their containing block.
  const animation=element.animate([
   {opacity:.86},
   {opacity:1},
  ],{duration:140,easing:"cubic-bezier(0.16, 1, 0.3, 1)"});
  const stop=()=>animation.cancel();
  media.addEventListener("change",stop);
  return()=>{media.removeEventListener("change",stop);stop();};
 },[ref,key,detail,keyboard]);
}
