import {useEffect,useLayoutEffect,useRef} from "react";
import type{WorkbenchSurface}from "./UnifiedWorkbench";

// A menu may close and open another sheet in one render. Wait for its Back
// traversal before pushing the replacement sheet, so the old traversal cannot
// accidentally dismiss the new sheet.
let returningLayer:Promise<void>|null=null;
function returnFromLayer(marker:string){
 if(history.state?.aiwrLayer!==marker)return;
 const traversal=new Promise<void>(resolve=>window.addEventListener("popstate",()=>{if(returningLayer===traversal)returningLayer=null;resolve();},{once:true}));
 returningLayer=traversal;history.back();
}

/** Back closes the top transient surface; it never approves or sends anything. */
export function useBackLayer(active:boolean,close:()=>void,clearQuery?:string){
 const callback=useRef(close);callback.current=close;
 const entry=useRef<string|null>(null);
 useEffect(()=>{
  if(!active){if(entry.current)returnFromLayer(entry.current);entry.current=null;return;}
  let cancelled=false,pop:(()=>void)|undefined;
  const open=()=>{
   if(cancelled)return;
   const marker=crypto.randomUUID();entry.current=marker;
   const url=new URL(location.href);
   if(clearQuery){const base=new URL(url);base.searchParams.delete(clearQuery);history.replaceState(history.state,"",base);}
   history.pushState({...history.state,aiwrLayer:marker},"",url);
   pop=()=>{if(history.state?.aiwrLayer!==marker){entry.current=null;callback.current();}};
   window.addEventListener("popstate",pop);
  };
  if(returningLayer)void returningLayer.then(open);else open();
  return()=>{cancelled=true;if(pop)window.removeEventListener("popstate",pop);};
 },[active]);
}

export function useWorkbenchHistory(surface:WorkbenchSurface,selected:string|null|undefined,navigate:(surface:WorkbenchSurface)=>void,select:(id:string)=>void){
 const latest=useRef({navigate,select});latest.current={navigate,select};
 const pendingPop=useRef(false),initialized=useRef(false);
 const initialBridge=useRef<{id:string;url:string}|null>(null);
 useLayoutEffect(()=>{
  const route={surface,selected};
  if(!initialized.current){
   initialized.current=true;const u=new URL(location.href),id=u.searchParams.get("workstream");
   if(surface==="WORKSPACE"&&id){initialBridge.current={id,url:u.href};for(const key of ["workstream","reply","handoff"])u.searchParams.delete(key);history.replaceState({aiwrRoute:{surface:"BRIDGES",selected:null}},"",u.href);}
   else history.replaceState({...history.state,aiwrRoute:route},"",location.href);
  }
  if(surface==="WORKSPACE"&&!selected)return;
  const entry=history.state?.aiwrBridgeEntry??initialBridge.current;
  if(entry?.id===selected&&surface==="WORKSPACE"){
   const base=new URL(entry.url);for(const key of ["workstream","reply","handoff"])base.searchParams.delete(key);
   history.replaceState({aiwrRoute:{surface:"BRIDGES",selected:null}},"",base.href);
   history.pushState({aiwrRoute:route,aiwrParentSurface:"BRIDGES"},"",entry.url);initialBridge.current=null;pendingPop.current=false;return;
  }
  if(surface==="BRIDGES"&&history.state?.aiwrRoute?.surface==="WORKSPACE"&&history.state?.aiwrParentSurface==="BRIDGES"){
   pendingPop.current=true;history.back();return;
  }
  if(pendingPop.current)pendingPop.current=false;
  else if(history.state?.aiwrRoute?.surface!==surface||(surface==="WORKSPACE"&&history.state?.aiwrRoute?.selected!==selected))history.pushState({aiwrRoute:route,aiwrParentSurface:history.state?.aiwrRoute?.surface},"",location.href);
 },[surface,selected]);
 useEffect(()=>{
  const pop=()=>{
   const route=history.state?.aiwrRoute;
   if(!route||! ["BRIDGES","WORKSPACE","NOTIFICATIONS","SETTINGS","NEW_BRIDGE","RUNTIME","RECYCLE"].includes(route.surface))return;
   if(route.surface===surface&&(route.surface!=="WORKSPACE"||route.selected===selected)){pendingPop.current=false;return;}
   pendingPop.current=true;
   if(route.surface==="WORKSPACE"&&typeof route.selected==="string")latest.current.select(route.selected);
   latest.current.navigate(route.surface);
  };
  window.addEventListener("popstate",pop);return()=>window.removeEventListener("popstate",pop);
 },[surface,selected]);
}
