import {prepareMobileSession} from "./mobile/startup";
import {installStandaloneTopEdge} from "./mobile/standaloneTopEdge";

const mobile=location.pathname.startsWith("/mobile");
if(mobile){
 installStandaloneTopEdge();
 document.documentElement.dataset.aiwrMobile="true";
 document.getElementById("aiwr-startup")?.removeAttribute("hidden");
 prepareMobileSession();
}
// Preserve the existing desktop entry; no Host/Agent launch or transport change.
void import("./main").catch(()=>{
 if(!mobile)return;
 const message=document.getElementById("aiwr-startup-status");
 if(message)message.textContent="连接暂时不可用";
 const retry=document.getElementById("aiwr-startup-retry");
 retry?.removeAttribute("hidden");retry?.addEventListener("click",()=>location.reload(),{once:true});
});
