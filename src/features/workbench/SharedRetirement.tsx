import * as Dialog from "@radix-ui/react-dialog";
import {invoke,isTauri} from "@tauri-apps/api/core";
import {useEffect,useState} from "react";
import {t as uiText,useLanguage} from "../../i18n";
import type {CodexConnectionStatus} from "./CodexConnection";

export const retireShared=()=>invoke<void>("codex_retire_shared");

/** The second confirmation. Only the desktop reaches it; the phone and MCP have no such call. */
export function SharedRetirementDialog({status,open,onClose,retire=retireShared}:{status:CodexConnectionStatus|null;open:boolean;onClose:()=>void;retire?:()=>Promise<void>}){
 useLanguage();
 const[working,setWorking]=useState(false),[failed,setFailed]=useState(false);
 const turns=status?.sharedActiveTurns??0;
 async function confirm(){setWorking(true);setFailed(false);try{await retire();onClose();}catch{setFailed(true);}finally{setWorking(false);}}
 return <Dialog.Root open={open} onOpenChange={next=>{if(!next&&!working)onClose();}}><Dialog.Portal><Dialog.Overlay className="v3-role-dialog-overlay shared-retirement-overlay"/><Dialog.Content className="r2-confirmation shared-retirement" aria-describedby="shared-retirement-description"><Dialog.Title>{uiText("关闭旧的共享后台？")}</Dialog.Title>
  <p id="shared-retirement-description">{uiText("这台电脑还在运行旧版的共享后台。关闭后 Agbrio 才能改用轻量连接，并完成这次更新。")}</p>
  <ul>
   {turns>0&&<li>{uiText("正在运行的 {0} 个任务会中断，不会自动重发。",turns)}</li>}
   {status?.sharedDesktopConnected&&<li>{uiText("正在使用它的 Codex 窗口要退出后重新打开。")}</li>}
   <li>{uiText("如果它以管理员身份运行，Windows 会请求授权。")}</li>
   <li>{uiText("Codex 的对话记录和你的登录不受影响。")}</li>
  </ul>
  {failed&&<p role="alert">{uiText("这次没有开始关闭，请稍后再试。")}</p>}
  <footer><Dialog.Close asChild><button type="button" disabled={working}>{uiText("稍后再说")}</button></Dialog.Close><button type="button" className="r2-action" disabled={working} onClick={()=>void confirm()}>{uiText(working?"正在关闭…":"关闭共享后台")}</button></footer>
 </Dialog.Content></Dialog.Portal></Dialog.Root>;
}

const nativeStatus=()=>invoke<CodexConnectionStatus>("codex_connection_status");

/** Asks once per launch; choosing later leaves the same action in Settings. */
export function SharedRetirementPrompt({status=nativeStatus,retire=retireShared,native=isTauri()}:{status?:()=>Promise<CodexConnectionStatus>;retire?:()=>Promise<void>;native?:boolean}){
 const[state,setState]=useState<CodexConnectionStatus|null>(null),[answered,setAnswered]=useState(false);
 useEffect(()=>{if(!native)return;let alive=true;const check=()=>void status().then(value=>{if(alive)setState(value);}).catch(()=>{});check();const timer=setInterval(check,5000);return()=>{alive=false;clearInterval(timer);};},[native,status]);
 // A backend that is not answering retires by itself, so there is nothing to ask.
 const inUse=state?.state==="SHARED_RETIRING"&&((state.sharedActiveTurns??0)>0||state.sharedDesktopConnected===true);
 return <SharedRetirementDialog status={state} open={native&&inUse&&!answered} onClose={()=>setAnswered(true)} retire={retire}/>;
}
