import {startupText as t} from "./i18n/startupCopy";
import {prepareMobileSession} from "./mobile/startup";
import {installStandaloneTopEdge} from "./mobile/standaloneTopEdge";

const mobile=location.pathname.startsWith("/mobile");
const startup=document.getElementById("aiwr-startup");
startup?.setAttribute("aria-label",t("Agbrio 正在连接"));
const startupStatus=document.getElementById("aiwr-startup-status");
if(startupStatus)startupStatus.textContent=t("正在连接…");
const startupRetry=document.getElementById("aiwr-startup-retry");
if(startupRetry)startupRetry.textContent=t("重试");
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
 if(message)message.textContent=t("连接暂时不可用");
 const retry=document.getElementById("aiwr-startup-retry");
 retry?.removeAttribute("hidden");retry?.addEventListener("click",()=>location.reload(),{once:true});
});
