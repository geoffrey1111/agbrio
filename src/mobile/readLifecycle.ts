/** A suspended page must not retain observation requests or their queue slots.
 * Commands never register here: their outcome must not be inferred or replayed. */
export function cancelReadOnSuspend(controller:AbortController){
 const cancel=()=>controller.abort(new DOMException("ROUTER_READ_SUSPENDED","AbortError"));
 const hidden=()=>{if(document.visibilityState==="hidden")cancel();};
 window.addEventListener("pagehide",cancel);
 document.addEventListener("visibilitychange",hidden);
 return()=>{window.removeEventListener("pagehide",cancel);document.removeEventListener("visibilitychange",hidden);};
}
