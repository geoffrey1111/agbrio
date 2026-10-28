import {useRef} from 'react';
// Safari can end composition before reporting the key event that committed it.
// Keep the short trailing guard on keyboard submission only, never send clicks.
export function useImeSubmitGuard(){
 const active=useRef(false),endedAt=useRef<number|null>(null);
 return {
  onCompositionStart:()=>{active.current=true;endedAt.current=null;},
  onCompositionEnd:()=>{active.current=false;endedAt.current=performance.now();},
  blocks:(event:KeyboardEvent)=>active.current||event.isComposing||event.keyCode===229||(endedAt.current!==null&&performance.now()-endedAt.current<100),
 };
}
