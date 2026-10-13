// One owned retry. A rejected native integrity check must close the supervisor,
// rather than leave a gateway that waits forever for a backend that cannot start.
export async function verifiedBackendStart(isClosing,prepare,verify,launch){
 if(isClosing())return null;
 const candidate=await prepare();
 if(isClosing())return null;
 await verify(candidate);
 if(isClosing())return null;
 return launch(candidate);
}
export function backendRecovery(start,onFailure,{setTimer=setTimeout,clearTimer=clearTimeout}={}){
 let timer=null,stopped=false;
 return {
  schedule(delay){
   if(stopped||timer!==null)return;
   timer=setTimer(()=>{timer=null;if(stopped)return;Promise.resolve().then(()=>stopped?undefined:start()).catch(error=>{if(!stopped){stopped=true;return onFailure(error);}}).catch(()=>{});},delay);
  },
  cancel(){stopped=true;if(timer!==null)clearTimer(timer);timer=null;}
 };
}
