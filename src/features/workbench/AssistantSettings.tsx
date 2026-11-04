import {EventKeychainSettings,type EventKeychainApi} from "./EventKeychainSettings";
import type {ClaudeConnectionResult} from "./ClaudeSessionPicker";
import {request} from "../../mobile/api";
import {useContext,useEffect,useRef,useState} from "react";
import {invoke} from "@tauri-apps/api/core";
import {Bot,Plus,ShieldOff} from "lucide-react";
import {t,useLanguage,getLanguage} from "../../i18n";
import {CopyAction} from "./NativeReaderActions";
import {AssistantConnectionGuide,type AssistantHelpPage} from "./AssistantConnectionGuide";
import {ConnectionSettingsNavigation} from "./NativeSurfaceContext";
type Endpoint={endpoint:{label:string;externalId:string}};
type Bridge={id:string;name:string;bindings:{bindingRevision:number;decision:Endpoint;execution:Endpoint}};
type Grant={id:string;scope?:"INSTANCE"|"BRIDGE";approvalMode?:"BRIEF_RULES"|"CONVERSATION_REVIEW";workstreamId:string;sourceRole:string;label:string;rules:{id:string;text:string}[];expiresAt:number;revokedAt:number|null};
type View={bridges:Bridge[];grants:Grant[];mcpUrl:string|null};
type Input={label:string;expiresAt:number};
export type AssistantSettingsApi={eventsKeychain?:EventKeychainApi;view:()=>Promise<View>;create:(input:Input)=>Promise<unknown>;revoke:(id:string)=>Promise<unknown>;openHelp?:(page:AssistantHelpPage)=>Promise<void>;claudeConnect?:()=>Promise<ClaudeConnectionResult>};
export const mobileAssistantSettingsApi:AssistantSettingsApi={view:()=>request("/assistant/settings"),create:input=>request("/assistant/settings",{method:"POST",body:JSON.stringify(input)}),revoke:id=>request(`/assistant/grants/${encodeURIComponent(id)}/revoke`,{method:"POST",body:"{}"})};
const desktopApi:AssistantSettingsApi={eventsKeychain:{status:()=>invoke("assistant_event_keychain_status"),authorize:after=>invoke("assistant_event_keychain_authorize",{after})},claudeConnect:()=>invoke("claude_code_connect"),view:()=>invoke("assistant_settings"),create:input=>invoke("assistant_connect_instance",{input}),revoke:grantId=>invoke("assistant_revoke_grant",{grantId}),openHelp:page=>invoke("assistant_open_help",{page})};
export function AssistantSettings({api=desktopApi}:{api?:AssistantSettingsApi}){
 const openConnectionSettings=useContext(ConnectionSettingsNavigation);
 useLanguage();const[view,setView]=useState<View|null>(null),[form,setForm]=useState(false),[label,setLabel]=useState("Dot"),[busy,setBusy]=useState(false),[error,setError]=useState(""),[status,setStatus]=useState("");const pending=useRef(false),mounted=useRef(true);
 const viewPending=useRef(false),viewGeneration=useRef(0);const[assistantRoute,setAssistantRoute]=useState<string|null>(null);
 async function refreshView(){if(viewPending.current)return;viewPending.current=true;const ticket=++viewGeneration.current;try{const next=await api.view();if(mounted.current&&ticket===viewGeneration.current){setView(next);setError("");}}catch{if(mounted.current&&ticket===viewGeneration.current)setError(t("助手设置暂时不可用，请重试。"));}finally{if(ticket===viewGeneration.current)viewPending.current=false;}}
 useEffect(()=>{mounted.current=true;void refreshView();const refresh=()=>{if(!document.hidden)void refreshView();};window.addEventListener('focus',refresh);window.addEventListener('online',refresh);document.addEventListener('visibilitychange',refresh);return()=>{mounted.current=false;viewGeneration.current++;viewPending.current=false;window.removeEventListener('focus',refresh);window.removeEventListener('online',refresh);document.removeEventListener('visibilitychange',refresh);};},[api]);
 async function act(work:()=>Promise<void>){if(pending.current)return;pending.current=true;viewGeneration.current++;viewPending.current=false;setBusy(true);setError("");setStatus("");try{await work();if(mounted.current){const v=await api.view();setView(v);}}catch(e){if(mounted.current)setError(String(e).includes("BRIDGE_CHANGED")?t("Bridge 已变化，请刷新后重新授权。"):t("操作未完成，请检查连接后重试。"));}finally{pending.current=false;if(mounted.current)setBusy(false);}}
 
 async function create(){if(!label.trim())return;await act(async()=>{await api.create({label:label.trim(),expiresAt:Date.now()+30*86400000});if(mounted.current){setForm(false);setStatus(t("授权已添加"));}});}
 const current=view?.grants.filter(g=>!g.revokedAt&&g.expiresAt>Date.now())??[];
 const grantForm=form?<form className="assistant-grant-form" aria-label={t("助手授权")} onSubmit={e=>{e.preventDefault();void create();}}><label>{t("助手名称")}<input value={label} maxLength={80} onChange={e=>setLabel(e.target.value)} disabled={busy} required/></label><p className="r2-assistant-target">{t("整个 Agbrio · 包含以后新建的 Bridge")}</p><p className="v4-meta">{t("决策要求直接告诉助手即可。连接有效 30 天，可随时撤销。")}</p>{error&&<p role="alert">{error}</p>}<div className="r2-assistant-actions"><button type="button" disabled={busy} onClick={()=>setForm(false)}>{t("取消")}</button><button className="r2-action" type="submit" disabled={busy||!label.trim()}>{busy?t("正在处理…"):t(assistantRoute==="claude"?"保存授权":"连接助手")}</button></div></form>:null;
 const grantsList=<div className="r2-settings-group">{current.map(g=><div className="r2-assistant-grant" key={g.id}><Bot size={20}/><div><strong>{g.label}</strong><small>{g.scope==="INSTANCE"?t("整个 Agbrio · 包含以后新建的 Bridge"):t("旧版单 Bridge 授权")+" · "+(view?.bridges.find(b=>b.id===g.workstreamId)?.name??g.workstreamId)}</small><small>{t("至")} {new Date(g.expiresAt).toLocaleDateString(getLanguage())}</small></div><button type="button" className="r2-icon" disabled={busy} aria-label={t("撤销 {0} 的授权",g.label)} onClick={()=>void act(async()=>{await api.revoke(g.id);setStatus(t("授权已撤销"));})}><ShieldOff size={20}/></button></div>)}</div>;
 return <div className="r2-assistant-settings">
 {api.eventsKeychain&&<EventKeychainSettings api={api.eventsKeychain}/>}
 {!view&&!error&&<p role="status">{t("加载中…")}</p>}
 {view&&<AssistantConnectionGuide grantForm={grantForm} onRefresh={refreshView} onConfigureConnection={openConnectionSettings??undefined} connectClaude={api.claudeConnect} mcpUrl={view.mcpUrl} grants={view.grants} bridges={view.bridges} onAddGrant={()=>setForm(true)} onRouteChange={route=>{setAssistantRoute(route);if(!form)setLabel(route==="claude"?"Claude Code":route==="local"?"Codex":route==="workbuddy"?"WorkBuddy":route==="generic"?t("AI 助手"):"Dot");}} openHelp={api.openHelp??(async page=>{window.open(page==="CLAUDE_CONNECTORS"?"https://claude.ai/customize/connectors":page==="CHATGPT_PLUGINS"?"https://chatgpt.com/plugins":"https://developers.openai.com/plugins/build/auth","_blank","noopener,noreferrer");})}/>}

 {view&&!form&&(assistantRoute==="claude"?current.length>0?<details className="assistant-setup-extra"><summary>{t("管理已有授权")}</summary>{grantsList}</details>:null:<>{grantsList}{!current.length&&<p className="v4-meta">{t("先添加授权，再到你要使用的助手中连接 MCP。")}</p>}<button type="button" className="r2-action" disabled={busy} onClick={()=>setForm(true)}><Plus size={18}/>{t("添加助手")}</button></>)}
 {form&&assistantRoute!=="claude"&&grantForm}
 {error&&!form&&<div role="alert"><p>{error}</p><button type="button" disabled={busy} onClick={()=>void act(async()=>{})}>{t("重试")}</button></div>}{status&&<p role="status">{status}</p>}
 </div>;
}
