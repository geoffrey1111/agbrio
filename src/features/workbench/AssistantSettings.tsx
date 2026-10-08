import {useEffect,useRef,useState} from "react";
import {invoke} from "@tauri-apps/api/core";
import {Bot,Plus,ShieldOff} from "lucide-react";
import {t,useLanguage,getLanguage} from "../../i18n";
import {CopyAction} from "./NativeReaderActions";
type Endpoint={endpoint:{label:string;externalId:string}};
type Bridge={id:string;name:string;bindings:{bindingRevision:number;decision:Endpoint;execution:Endpoint}};
type Grant={id:string;scope?:"INSTANCE"|"BRIDGE";approvalMode?:"BRIEF_RULES"|"CONVERSATION_REVIEW";workstreamId:string;sourceRole:string;label:string;rules:{id:string;text:string}[];expiresAt:number;revokedAt:number|null};
type View={bridges:Bridge[];grants:Grant[];mcpUrl:string|null};
type Input={label:string;expiresAt:number};
export type AssistantSettingsApi={view:()=>Promise<View>;create:(input:Input)=>Promise<unknown>;revoke:(id:string)=>Promise<unknown>};
const desktopApi:AssistantSettingsApi={view:()=>invoke("assistant_settings"),create:input=>invoke("assistant_connect_instance",{input}),revoke:grantId=>invoke("assistant_revoke_grant",{grantId})};
export function AssistantSettings({api=desktopApi}:{api?:AssistantSettingsApi}){
 useLanguage();const[view,setView]=useState<View|null>(null),[form,setForm]=useState(false),[label,setLabel]=useState("Dot"),[busy,setBusy]=useState(false),[error,setError]=useState(""),[status,setStatus]=useState("");const pending=useRef(false),mounted=useRef(true);
 useEffect(()=>{mounted.current=true;void api.view().then(v=>{if(mounted.current){setView(v);}}).catch(()=>{if(mounted.current)setError(t("助手设置暂时不可用，请重试。"));});return()=>{mounted.current=false;};},[api]);
 async function act(work:()=>Promise<void>){if(pending.current)return;pending.current=true;setBusy(true);setError("");setStatus("");try{await work();if(mounted.current){const v=await api.view();setView(v);}}catch(e){if(mounted.current)setError(String(e).includes("BRIDGE_CHANGED")?t("Bridge 已变化，请刷新后重新授权。"):t("操作未完成，请检查连接后重试。"));}finally{pending.current=false;if(mounted.current)setBusy(false);}}
 
 async function create(){if(!label.trim())return;await act(async()=>{await api.create({label:label.trim(),expiresAt:Date.now()+30*86400000});if(mounted.current){setForm(false);setStatus(t("授权已添加"));}});}
 const current=view?.grants.filter(g=>!g.revokedAt&&g.expiresAt>Date.now())??[];
 return <div className="r2-assistant-settings"><p className="r2-assistant-description">{t("助手读取结果后处理交接；需要你决策时，在助手对话里问你。")}</p>{view?.mcpUrl?<div className="r2-device-url"><code>{view.mcpUrl}</code><CopyAction text={view.mcpUrl} label={t("复制 MCP 地址")} onError={()=>setError(t("网址未复制，请选择链接复制。"))}/></div>:view&&<p className="v4-meta">{t("先在设备设置中配置 HTTPS 入口。")}</p>}
 {!view&&!error&&<p role="status">{t("加载中…")}</p>}
 {view&&!form&&<><div className="r2-settings-group">{current.map(g=><div className="r2-assistant-grant" key={g.id}><Bot size={20}/><div><strong>{g.label}</strong><small>{g.scope==="INSTANCE"?t("整个 Agbrio · 包含以后新建的 Bridge"):t("旧版单 Bridge 授权")+" · "+(view.bridges.find(b=>b.id===g.workstreamId)?.name??g.workstreamId)}</small><small>{t("至")} {new Date(g.expiresAt).toLocaleDateString(getLanguage())}</small></div><button type="button" className="r2-icon" disabled={busy} aria-label={t("撤销 {0} 的授权",g.label)} onClick={()=>void act(async()=>{await api.revoke(g.id);setStatus(t("授权已撤销"));})}><ShieldOff size={20}/></button></div>)}</div>{!current.length&&<p className="v4-meta">{t("先添加助手，再用 MCP 地址在 ChatGPT 中连接。")}</p>}<button type="button" className="r2-action" disabled={busy} onClick={()=>setForm(true)}><Plus size={18}/>{t("添加助手")}</button></>}
 {form&&<form onSubmit={e=>{e.preventDefault();void create();}}><label>{t("助手名称")}<input value={label} maxLength={80} onChange={e=>setLabel(e.target.value)} disabled={busy} required/></label><p className="r2-assistant-target">{t("整个 Agbrio · 包含以后新建的 Bridge")}</p><p className="v4-meta">{t("决策要求直接告诉助手即可。连接有效 30 天，可随时撤销。")}</p><div className="r2-assistant-actions"><button type="button" disabled={busy} onClick={()=>setForm(false)}>{t("取消")}</button><button className="r2-action" type="submit" disabled={busy||!label.trim()}>{busy?t("正在处理…"):t("连接助手")}</button></div></form>}
 {error&&<div role="alert"><p>{error}</p><button type="button" disabled={busy} onClick={()=>void act(async()=>{})}>{t("重试")}</button></div>}{status&&<p role="status">{status}</p>}
 </div>;
}
