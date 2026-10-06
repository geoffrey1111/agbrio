import {useLayoutEffect} from "react";
import {recordViewportEvidence} from "./displayDiagnostics";
export {PHONE_LAYOUT_REVISION} from "./interfaceRevision";

type Insets = {top:number;right:number;bottom:number;left:number};
type Frame = {top:number;left:number;width:number;height:number};

/** Some Home Screen WebKit viewports already exclude both safe areas. */
export function resolveVisibleFrame(frame:Frame,visible:Frame,safe:Insets,standalone:boolean){
 const cropped=frame.height-visible.height;
 const safeCrop=standalone&&visible.top===0&&visible.left===0&&cropped>1&&cropped<=safe.top+safe.bottom+2&&Math.abs(frame.width-visible.width)<2;
 return safeCrop?frame:visible;
}

/** Insets covered by the keyboard or viewport pan must not be reserved twice. */
export function visibleViewportInsets(layout:{width:number;height:number}, visible:{top:number;left:number;width:number;height:number}, safe:Insets){
 const bottom=Math.max(0,layout.height-visible.top-visible.height);
 const right=Math.max(0,layout.width-visible.left-visible.width);
 return {bottom, safe:{
  top:Math.max(0,safe.top-visible.top),
  right:Math.max(0,safe.right-right),
  bottom:Math.max(0,safe.bottom-bottom),
  left:Math.max(0,safe.left-visible.left),
 }};
}

/** The visible iOS viewport may pan as well as shrink when the keyboard opens. */
export function useVisibleViewport(){
 useLayoutEffect(()=>{
  const viewport=window.visualViewport;
  // Measure the CSS fixed containing block, independently of document height
  // and visualViewport's already-cropped standalone measurements.
  const probe=document.createElement("div");
  probe.dataset.aiwrViewportProbe="true";
  probe.style.cssText="position:fixed;inset:0;visibility:hidden;pointer-events:none;margin:0;padding:0;border:0;z-index:-1;";
  document.body.append(probe);
  const update=()=>{
   if(viewport&&Math.abs(viewport.scale-1)>.05)return;
   const root=document.documentElement, style=root.style;
   const measured=probe.getBoundingClientRect();
   const frame={top:measured.top,left:measured.left,width:measured.width||innerWidth,height:measured.height||innerHeight};
   const reported={height:viewport?.height??innerHeight,width:viewport?.width??innerWidth,top:viewport?.offsetTop??0,left:viewport?.offsetLeft??0};
   const computed=getComputedStyle(root);
   const read=(edge:string)=>parseFloat(computed.getPropertyValue(`--aiwr-safe-area-${edge}`))||0;
   const safe={top:read("top"),right:read("right"),bottom:read("bottom"),left:read("left")};
   const standalone=window.matchMedia?.("(display-mode: standalone),(display-mode: fullscreen)").matches||Boolean((navigator as Navigator&{standalone?:boolean}).standalone);
   const visible=resolveVisibleFrame(frame,reported,safe,standalone);
   recordViewportEvidence({frame,reported,resolved:visible,safe,standalone});
   const edges=visibleViewportInsets(frame,visible,safe);
   style.setProperty("--aiwr-shell-height",`${visible.height}px`);
   style.setProperty("--aiwr-shell-width",`${visible.width}px`);
   style.setProperty("--aiwr-shell-top",`${visible.top}px`);
   style.setProperty("--aiwr-shell-left",`${visible.left}px`);
   style.setProperty("--aiwr-shell-bottom",`${edges.bottom}px`);
   for(const edge of ["top","right","bottom","left"] as const)style.setProperty(`--aiwr-inset-${edge}`,`${edges.safe[edge]}px`);
   root.dataset.aiwrCompact=String(visible.height<400);
  };
  update();window.addEventListener("resize",update);viewport?.addEventListener("resize",update);viewport?.addEventListener("scroll",update);
  return()=>{probe.remove();window.removeEventListener("resize",update);viewport?.removeEventListener("resize",update);viewport?.removeEventListener("scroll",update);};
 },[]);
}
