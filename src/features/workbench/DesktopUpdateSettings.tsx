import {useEffect,useRef,useState} from "react";
import {invoke} from "@tauri-apps/api/core";
import {listen} from "@tauri-apps/api/event";
import {Download,RefreshCw,LoaderCircle,CheckCircle2,ExternalLink} from "lucide-react";
import {t,useLanguage} from "../../i18n";
export type UpdateView={state:string;currentVersion:string;version:string|null;notes:string|null;releaseUrl:string|null;downloaded:number;total:number|null;error:string|null};
export type UpdateApi={status:()=>Promise<UpdateView>;check:()=>Promise<UpdateView>;download:(version:string)=>Promise<UpdateView>;install:(version:string)=>Promise<void>;openRelease:()=>Promise<unknown>;subscribe?:(cb:(value:UpdateView)=>void)=>Promise<()=>void>};
const api:UpdateApi={status:()=>invoke("desktop_update_status"),check:()=>invoke("desktop_update_check"),download:version=>invoke("desktop_update_download",{version}),install:version=>invoke("desktop_update_install",{version}),openRelease:()=>invoke("desktop_update_open_release"),subscribe:cb=>listen<UpdateView>("agbrio-update-progress",e=>cb(e.payload))};
const empty:UpdateView={state:"IDLE",currentVersion:"0.1.0",version:null,notes:null,releaseUrl:null,downloaded:0,total:null,error:null};
const errors:Record<string,string>={UPDATE_NETWORK:"暂时无法检查更新，请检查网络后重试。",UPDATE_TASK_RUNNING:"Agbrio 的任务仍在执行，完成后再安装。",UPDATE_BACKUP_FAILED:"数据备份未完成，更新尚未安装。请重试。",UPDATE_DOWNLOAD_OR_SIGNATURE_FAILED:"下载或签名验证未完成，更新尚未安装。请重试。",UPDATE_INSTALL_FAILED:"安装未启动，当前版本仍可使用。请重试。",UPDATE_CHANGED:"更新信息已变化，请重新检查。",UPDATE_PLATFORM_UNSUPPORTED:"此平台暂不支持应用内更新。"};
export function DesktopUpdateSettings({updateApi=api}:{updateApi?:UpdateApi}){
 useLanguage();const[view,setView]=useState<UpdateView>(empty),[busy,setBusy]=useState(false),[error,setError]=useState("");const pending=useRef(false),alive=useRef(true);
 useEffect(()=>{alive.current=true;let dispose=()=>{};void updateApi.status().then(v=>{if(alive.current)setView(v);}).catch(()=>{if(alive.current)setError(t("暂时无法检查更新，请检查网络后重试。"));});void updateApi.subscribe?.(v=>{if(alive.current)setView(v);}).then(fn=>{if(alive.current)dispose=fn;else fn();}).catch(()=>{});return()=>{alive.current=false;dispose();};},[updateApi]);
 async function act(work:()=>Promise<UpdateView|void>){if(pending.current)return;pending.current=true;setBusy(true);setError("");try{const next=await work();if(alive.current&&next)setView(next);}catch(e){if(alive.current){const code=Object.keys(errors).find(code=>String(e).includes(code));setError(t(code?errors[code]:"更新未完成，请重新检查或查看发布页面。"));try{const current=await updateApi.status();if(alive.current)setView(current);}catch{if(alive.current)setView(v=>({...v,state:"FAILED"}));}}}finally{pending.current=false;if(alive.current)setBusy(false);}}
 const working=busy||["CHECKING","DOWNLOADING","INSTALLING"].includes(view.state),percent=view.total?Math.min(100,Math.floor(view.downloaded/view.total*100)):null;
 return <section className="r2-update-settings"><p className="v4-meta">{t("当前版本")} {view.currentVersion}</p>
 <div className="r2-update-state" role="status">{working?<LoaderCircle size={18} className="r2-update-spinner"/>:view.state==="UP_TO_DATE"?<CheckCircle2 size={18}/>:<Download size={18}/>}<span>{t(view.state==="CHECKING"?"正在检查更新…":view.state==="DOWNLOADING"?"正在下载更新…":view.state==="INSTALLING"?"正在安装，Agbrio 将重新打开…":view.state==="AVAILABLE"?"有新版本 {0}":view.state==="READY"?"更新已下载并验证签名":view.state==="UP_TO_DATE"?"当前已是最新版本":view.state==="NO_SIGNED_RELEASE"?"当前没有可应用内安装的更新":view.state==="MANUAL_AVAILABLE"?"新版本 {0} 需要手动安装":"从 GitHub 检查新版本",view.version)}</span></div>
 {view.state==="DOWNLOADING"&&<div className="r2-update-progress"><progress aria-label={t("更新下载进度")} max={view.total??1} value={view.total?view.downloaded:undefined}/><small>{percent===null?t("正在下载…"):`${percent}%`}</small></div>}
 {view.notes&&<details className="r2-update-notes"><summary>{t("发布说明")}</summary><p>{view.notes}</p></details>}
 {error&&<p role="alert">{error}</p>}
 <div className="r2-update-actions"><button type="button" className="r2-action" disabled={working} onClick={()=>void act(async()=>{setView(v=>({...v,state:"CHECKING"}));return updateApi.check();})}><RefreshCw size={17}/>{t("检查更新")}</button>
 {view.state==="AVAILABLE"&&view.version&&<button type="button" className="r2-action" disabled={working} onClick={()=>void act(()=>updateApi.download(view.version!))}><Download size={17}/>{t("下载更新")}</button>}
 {view.state==="READY"&&view.version&&<button type="button" className="r2-action" disabled={working} onClick={()=>void act(async()=>{setView(v=>({...v,state:"INSTALLING"}));await updateApi.install(view.version!);})}>{t("安装并重启 Agbrio")}</button>}
 <button type="button" disabled={working} onClick={()=>void act(async()=>{await updateApi.openRelease();})}><ExternalLink size={16}/>{t("查看发布页面")}</button></div>
 {view.state==="READY"&&<p className="v4-meta">{t("安装前自动备份数据；共享 Codex 不重启，连接设置保留。")}</p>}
 </section>;
}
