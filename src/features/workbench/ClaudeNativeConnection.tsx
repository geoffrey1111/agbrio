import {useEffect,useRef,useState} from "react";
import {Link2,LoaderCircle} from "lucide-react";
import {t,useLanguage} from "../../i18n";
import type {ClaudeConnectionResult} from "./ClaudeSessionPicker";
import {connectionError} from "./claudeConnectionErrors";

export function ClaudeNativeConnection({connect}:{connect?:()=>Promise<ClaudeConnectionResult>}){
 useLanguage();
 const[busy,setBusy]=useState(false),[phase,setPhase]=useState<string|null>(null),[error,setError]=useState<string|null>(null);
 const mounted=useRef(false),pending=useRef(false);
 useEffect(()=>{mounted.current=true;return()=>{mounted.current=false;};},[]);
 async function prepare(){
  if(!connect||pending.current)return;
  pending.current=true;setBusy(true);setError(null);setPhase(null);
  try{const result=await connect();if(!["CONNECTED","WAITING_SESSION"].includes(result?.phase))throw Error("CLAUDE_CONNECTION_CONFIGURATION_FAILED");if(mounted.current)setPhase(result.phase);}
  catch(e){if(mounted.current)setError(connectionError(e));}
  finally{pending.current=false;if(mounted.current)setBusy(false);}
 }
 return <div className="assistant-native-connection">
  <p><strong>{t("Claude 的通知与手机回复")}</strong></p>
  <p>{t("要把 Codex 完成通知送回 Claude，或从手机回复 Claude 原对话，需要启用本机连接。MCP 本身仍可用于读取与派发。")}</p>
  {connect?<button className="r2-action" type="button" disabled={busy} onClick={()=>void prepare()}>{busy?<LoaderCircle size={16} className="v4-progress-spinner"/>:<Link2 size={16}/>} {t(busy?"正在配置…":phase?"检查本机连接":"启用 Claude 通知与手机回复")}</button>:<p>{t("请在电脑端 Agbrio 的同一页面启用本机连接，再回到 Claude 管理对话核对。")}</p>}
  {phase&&<p role="status" aria-live="polite">{t(phase==="CONNECTED"?"已收到 Claude 连接心跳。还需在管理会话登记具体任务的完成通知。":"本机连接已准备。请在空闲的 Claude 管理会话执行 /reload-plugins，再检查连接。")}</p>}
  {error&&<p role="alert">{error}</p>}
  <p>{t("需要通知时，让管理助手实际发现 codex_wait：先查看 status，再为已委托的精确轮次登记 wait，拿到 WAITING 回执后才能承诺通知。不会自动监听所有对话。")}</p>
 </div>;
}
