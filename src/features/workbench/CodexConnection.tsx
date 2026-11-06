import {ConnectionGuide} from "./ConnectionGuides";
import {t as uiText,useLanguage} from "../../i18n";
import {useEffect,useState} from "react";
import {invoke} from "@tauri-apps/api/core";
export type CodexConnectionStatus={enabled:boolean;ready:boolean;desktopConnected:boolean;state:string;detail?:string|null};
const nativeStatus=()=>invoke<CodexConnectionStatus>("codex_connection_status");
/** Nothing to set up: Windows always uses the light connection. This only reports it. */
export function CodexConnection({status=nativeStatus}:{status?:()=>Promise<CodexConnectionStatus>}){
 useLanguage();
 const[state,setState]=useState<CodexConnectionStatus|null>(null),[failed,setFailed]=useState(false);
 useEffect(()=>{let alive=true;const check=()=>void status().then(value=>{if(alive){setState(value);setFailed(false);}}).catch(()=>{if(alive)setFailed(true);});check();const timer=setInterval(check,10000);return()=>{alive=false;clearInterval(timer);};},[status]);
 const label=uiText("Codex 连接");
 if(state?.state==="SHARED_RETIRING")return <section className="v4-shared-codex" aria-label={label}><h2>{uiText("正在等待停用旧的共享后台")}</h2><p role="status">{uiText("结束任务并退出 Codex 后，Agbrio 会自动停用它并改用轻量连接，不会中断任务。在此之前 Agbrio 暂时连不上 Codex。")}</p></section>;
 if(state?.state==="SHARED_RETIREMENT_BLOCKED")return <section className="v4-shared-codex" aria-label={label}><h2>{uiText("旧的共享后台未能停用")}</h2><p role="alert">{uiText(state.detail==="SHARED_UAC_CANCELLED"?"已取消 Windows 授权，旧的共享后台保持原样。":"没有结束任何进程，旧的共享后台保持原样。")} {uiText("在此之前 Agbrio 暂时连不上 Codex。重新打开 Agbrio 后会再试一次。")}</p>{state.detail&&<small>{state.detail}</small>}</section>;
 if(state?.state==="UNSUPPORTED")return <section className="v4-shared-codex" aria-label={label}><h2>{label}</h2><p>{uiText("这个平台使用独立的 Codex 后台，不需要在这里设置。")}</p></section>;
 const connected=state?.state==="LIGHT_CONNECTED";
 return <section className="v4-shared-codex" aria-label={label}><h2>{uiText(connected?"轻量连接已接入 Codex":state?"轻量连接已开启":"Codex 连接")}</h2>{state&&<p>{uiText(connected?"直接使用正常打开的 Codex，不需要退出或专用入口。":"请正常打开 Codex，Agbrio 会自动连接。")}</p>}{state&&<p className="v4-shared-permission-note">{uiText("轻量连接不支持：管理 Goal、以管理员身份启动、退出 Codex 后继续执行任务。")}</p>}{failed&&<p role="alert">{uiText("连接状态暂时无法读取。")}</p>}{state&&<ConnectionGuide kind="codex" phase={connected?2:1} open={!connected}/>}</section>;
}
