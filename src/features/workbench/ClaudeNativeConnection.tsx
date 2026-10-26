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
  <p>{t("首次使用不用预装插件。点击下方按钮会安装、启用并配置内置 Claude 组件，用于完成通知和手机回复；MCP 读取与派发可以先使用。")}</p>
  {connect?<button className="r2-action" type="button" disabled={busy} onClick={()=>void prepare()}>{busy?<LoaderCircle size={16} className="v4-progress-spinner"/>:<Link2 size={16}/>} {t(busy?"正在配置…":phase?"检查本机连接":"安装并连接 Claude 完整功能")}</button>:<p>{t("网页和手机不能安装本机组件。请打开已安装的 Windows Agbrio 客户端，在本页完成安装；不要先执行 /reload-plugins。")}</p>}
  {phase&&<p role="status" aria-live="polite">{t(phase==="CONNECTED"?"已收到 Claude 连接心跳。还需在管理会话登记具体任务的完成通知。":"安装与配置已完成，尚待当前会话加载。请等 Claude 管理会话空闲后执行 /reload-plugins，再检查连接。")}</p>}
  {error&&<p role="alert">{error}</p>}
  <p>{t("需要通知时，让管理助手实际发现 codex_wait：先查看 status，再为已委托的精确轮次登记 wait，拿到 WAITING 回执后才能承诺通知。不会自动监听所有对话。")}</p>
 </div>;
}
