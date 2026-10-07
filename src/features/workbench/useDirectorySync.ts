import {useEffect,useRef} from "react";
/** Refresh list authority only; never reload a reader, form or provider session. */
export function useDirectorySync(refresh:()=>Promise<unknown>){
 const latest=useRef(refresh);latest.current=refresh;
 useEffect(()=>{
  let stopped=false,inFlight=false;
  async function sync(){if(stopped||inFlight||document.visibilityState==="hidden")return;inFlight=true;try{await latest.current();}catch{/* Keep last successful index; the next foreground check retries. */}finally{inFlight=false;}}
  const visible=()=>{if(document.visibilityState!=="hidden")void sync();};
  const timer=setInterval(()=>void sync(),5000);window.addEventListener("focus",visible);document.addEventListener("visibilitychange",visible);
  return()=>{stopped=true;clearInterval(timer);window.removeEventListener("focus",visible);document.removeEventListener("visibilitychange",visible);};
 },[]);
}
