import {useEffect,useRef,useState} from "react";
import {Link2,LoaderCircle} from "lucide-react";
import {t,getLanguage,useLanguage} from "../../i18n";
import type {ClaudeConnectionResult} from "./ClaudeSessionPicker";
import {connectionError} from "./claudeConnectionErrors";
import {CopyAction} from "./NativeReaderActions";
import {claudeCompletionInstructions,claudeFailureStage,claudeManualInstallPowerShell,claudeRecoveryKind} from "./claudeConnectionRecovery";

export function ClaudeNativeConnection({connect}:{connect?:()=>Promise<ClaudeConnectionResult>}){
 useLanguage();
 const[busy,setBusy]=useState(false),[phase,setPhase]=useState<string|null>(null),[error,setError]=useState<string|null>(null);
 const mounted=useRef(false),pending=useRef(false);
 useEffect(()=>{mounted.current=true;return()=>{mounted.current=false;};},[]);
 async function prepare(){
  if(!connect||pending.current)return;
  pending.current=true;setBusy(true);setError(null);setPhase(null);
  try{const result=await connect();if(!["CONNECTED","WAITING_SESSION"].includes(result?.phase))throw Error("CLAUDE_CONNECTION_CONFIGURATION_FAILED");if(mounted.current)setPhase(result.phase);}
  catch(e){if(mounted.current)setError(String(e));}
  finally{pending.current=false;if(mounted.current)setBusy(false);}
 }
 return <div className="assistant-native-connection">
  <p><strong>{t("Claude 的通知与手机回复")}</strong></p>
  <p>{t("首次使用不用预装插件。点击下方按钮会安装、启用并配置内置 Claude 组件，用于完成通知和手机回复；MCP 读取与派发可以先使用。")}</p>
  {connect?<button className="r2-action" type="button" disabled={busy} onClick={()=>void prepare()}>{busy?<LoaderCircle size={16} className="v4-progress-spinner"/>:<Link2 size={16}/>} {t(busy?"正在配置…":phase?"检查本机连接":"安装并连接 Claude 完整功能")}</button>:<p>{t("网页和手机不能安装本机组件。请打开已安装的 Windows Agbrio 客户端，在本页完成安装；不要先执行 /reload-plugins。")}</p>}
  {phase&&<p role="status" aria-live="polite">{t(phase==="CONNECTED"?"已收到 Claude 连接心跳。还需在管理会话登记具体任务的完成通知。":"安装与配置已完成，尚待当前会话加载。请等 Claude 管理会话空闲后执行 /reload-plugins，再检查连接。")}</p>}
  {phase==="WAITING_SESSION"&&<ol className="assistant-native-next"><li>{t("等管理会话空闲，在 Claude 中执行以下命令，再点击上方“检查本机连接”。")} <CopyAction showLabel text="/reload-plugins" label={t("复制重载命令")}/></li><li>{t("仍未连接？先保存工作，自己完整退出并重新打开 Claude 桌面应用，再回到原管理对话检查。不要在任务执行时强制重启。")}</li></ol>}
  {phase&&<div className="assistant-guide-prompt"><CopyAction showLabel text={claudeCompletionInstructions(getLanguage())} label={t("复制给管理助手的后续指令")}/><p>{t("把指令发给实际使用的 Claude 管理对话，检查 MCP 与通知登记。连接心跳不代表所有任务都会自动通知。")}</p></div>}
  {error&&<div className="assistant-native-recovery">
   <p role="alert">{connectionError(error)}</p>
   {claudeFailureStage(error)&&<p className="assistant-guide-note">{t("失败步骤")}: {claudeFailureStage(error)}</p>}
   {claudeRecoveryKind(error)==='manual'?<><ol><li>{t("保持已安装的 Windows Agbrio 客户端运行。在同一台电脑的普通 PowerShell 中执行手动安装命令；遇到失败会停止，请保留原始报错。")}</li><li>{t("手动安装成功后，回到这里再次点击安装连接按钮，完成连接配置。安装插件不会替你完成登录或填写连接凭据。")}</li></ol>
   <div className="assistant-guide-prompt"><CopyAction showLabel text={claudeManualInstallPowerShell} label={t("复制手动安装命令")}/><details><summary>{t("查看 PowerShell 命令")}</summary><pre>{claudeManualInstallPowerShell}</pre></details></div>
   <p>{t("配置完成后，等管理会话空闲执行 /reload-plugins；仍未加载时，保存工作后自行完整退出并重开 Claude，再检查连接。")}</p></>:claudeRecoveryKind(error)==='configuration'?<><p>{t("插件已找到，失败发生在连接配置。保持 Agbrio 运行，再点击连接按钮；仍失败时保留上方步骤和退出码，请助手排查。不要反复安装或手动填写令牌。")}</p><CopyAction showLabel text={claudeCompletionInstructions(getLanguage())} label={t("复制给管理助手的后续指令")}/></>:<p>{t(claudeRecoveryKind(error)==='wait'?"等待当前配置操作结束，再检查本机连接；不要同时重复安装。":claudeRecoveryKind(error)==='host'?"先核对并保留原 Agbrio 连接地址，不要用重新安装来覆盖另一台电脑的连接。":"先更新 Claude 桌面应用，打开 Code 本地会话确认 /status 版本，再回到这里安装连接。")}</p>}
  </div>}
  {!phase&&!error&&connect&&<details className="assistant-native-help"><summary>{t("已手动安装，或重载后仍没有通知？")}</summary><p>{t("手动安装后仍需点击上方按钮完成连接配置。配置成功后，先在空闲会话重载插件；无效时保存工作并完整退出、重新打开 Claude。最后复制后续指令，检查实际工具和任务通知登记。")}</p><CopyAction showLabel text={claudeCompletionInstructions(getLanguage())} label={t("复制给管理助手的后续指令")}/></details>}
 </div>;
}
