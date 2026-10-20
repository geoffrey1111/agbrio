import {useEffect,useRef,useState} from "react";
import "../../styles/claude-code.css";
import {t,useLanguage} from "../../i18n";
import type {RoleInput} from "./RoleBridgePanel";
import {RefreshCw} from "lucide-react";
import {ConversationAvatar} from "./ConversationHome";
export type ClaudeSession={sessionId:string;title?:string|null;cwd:string;version:string;state:string;leaseUntil:number;checkedAt:number};
export type ClaudeCatalog={sessions:ClaudeSession[];complete:boolean;writesEnabled:boolean};
export type ClaudePairing={connectionId:string;hostUrl:string;pluginDirectory?:string;token:string;expiresAt:number;writesEnabled:boolean};
export type ClaudeConnection={connectionId:string;expiresAt:number;revoked:boolean;sessionCount:number};
export type ClaudeConnectionResult={phase:"CONNECTED"|"WAITING_SESSION";writesEnabled:boolean};
export type ClaudePickerApi={claudeSessions?:()=>Promise<ClaudeCatalog>;claudeConnections?:()=>Promise<ClaudeConnection[]>;claudeSetup?:()=>Promise<ClaudePairing>;claudeConnect?:()=>Promise<ClaudeConnectionResult>;claudeRevoke?:(id:string)=>Promise<void>};
export const providerLabel=(provider:string)=>provider==="CLAUDE_CODE"?"Claude Code":provider==="CODEX"?"Codex":"ChatGPT";
// DESKTOP: listed by the Claude desktop app but not reported yet; selectable by exact identity.
const unavailable=(s:ClaudeSession)=>s.state!=="DESKTOP"&&(s.leaseUntil<=Date.now()||s.state==="SESSION_END");
const stateText=(s:ClaudeSession)=>unavailable(s)?t("暂时离线"):t(({RUNNING:"执行中",WAITING_DECISION:"需要决策",COMPLETE:"回复已就绪",FAILED:"执行失败",INTERRUPTED:"已中断",DESKTOP:"未同步 · 在 Claude 打开后开始同步"} as Record<string,string>)[s.state]??"已连接");
const folder=(cwd:string)=>cwd.split(/[\\/]/).filter(Boolean).at(-1)??cwd;
const clock=(at:number)=>{const d=new Date(at);return d.toDateString()===new Date().toDateString()?d.toLocaleTimeString([],{hour:"2-digit",minute:"2-digit"}):d.toLocaleDateString([],{month:"numeric",day:"numeric"});};
const title=(s:ClaudeSession)=>s.title?.trim()||t("未命名对话");
const connectionError=(error:unknown)=>t(({
 CLAUDE_DESKTOP_ENGINE_NOT_FOUND:"未找到支持观察插件的 Claude Code，请先安装或更新 Claude 桌面应用。",
 CLAUDE_PLUGIN_DIFFERENT_HOST:"这个插件已连接另一个 Agbrio 地址，请先核对原连接。",
 CLAUDE_CONNECTION_BUSY:"连接正在配置，请稍候。",
 CLAUDE_PLUGIN_ADMIN_TIMEOUT:"配置超时，未确认连接成功。请稍后检查连接。",
 CLAUDE_PLUGIN_ADMIN_REFUSED:"Claude 拒绝了插件配置；原任务未启动或重启，请检查插件权限。",
} as Record<string,string>)[String(error)]??"连接配置未完成，请检查 Claude 是否已安装；也可以展开手动连接。");
export function ClaudeSessionPicker({api,value,onChange}:{api:ClaudePickerApi;value:RoleInput;onChange:(input:RoleInput)=>void}){
 useLanguage();
 const[catalog,setCatalog]=useState<ClaudeCatalog|null>(null),[error,setError]=useState<string|null>(null),[busy,setBusy]=useState(false),[pairing,setPairing]=useState<ClaudePairing|null>(null);
 const[connections,setConnections]=useState<ClaudeConnection[]>([]),[connection,setConnection]=useState<ClaudeConnectionResult|null>(null);
 const order=useRef<string[]>([]),buttons=useRef(new Map<string,HTMLButtonElement>());
 function accept(next:ClaudeCatalog){
  const ids=new Set(next.sessions.map(s=>s.sessionId));order.current=order.current.filter(id=>ids.has(id));
  const added=next.sessions.filter(s=>!order.current.includes(s.sessionId)).sort((a,b)=>(b.checkedAt??0)-(a.checkedAt??0)||title(a).localeCompare(title(b))||a.sessionId.localeCompare(b.sessionId));
  order.current.push(...added.map(s=>s.sessionId));next.sessions.sort((a,b)=>order.current.indexOf(a.sessionId)-order.current.indexOf(b.sessionId));setCatalog(next);
 }
 useEffect(()=>{let alive=true;if(api.claudeConnections)void api.claudeConnections().then(v=>{if(alive)setConnections(v);}).catch(()=>{});return()=>{alive=false;};},[api.claudeConnections]);
 useEffect(()=>{let alive=true;let reading=false;async function load(){if(!api.claudeSessions||reading)return;reading=true;try{const next=await api.claudeSessions();if(alive)accept({...next,sessions:[...next.sessions]});}catch{if(alive)setError(t("Claude Code 目录暂不可用，请确认 Host 已更新。"));}finally{reading=false;}}
  void load();const timer=setInterval(()=>void load(),15000);return()=>{alive=false;clearInterval(timer);};
 },[api.claudeSessions]);
 async function refresh(){if(!api.claudeSessions)return;setBusy(true);setError(null);try{const next=await api.claudeSessions();accept({...next,sessions:[...next.sessions]});}catch{setError(t("Claude Code 目录暂不可用，请确认 Host 已更新。"));}finally{setBusy(false);}}
 async function connect(){if(!api.claudeConnect)return;setBusy(true);setError(null);try{setConnection(await api.claudeConnect());if(api.claudeSessions){const next=await api.claudeSessions();accept({...next,sessions:[...next.sessions]});}if(api.claudeConnections)setConnections(await api.claudeConnections());}catch(e){setError(connectionError(e));}finally{setBusy(false);}}
 const live=catalog?.sessions.some(s=>!unavailable(s));
 const available=catalog?.sessions.filter(s=>!unavailable(s))??[];
 function choose(s:ClaudeSession){onChange({provider:"CLAUDE_CODE",externalId:s.sessionId,label:title(s),cwd:s.cwd});}
 function navigate(key:string,current:ClaudeSession){
  if(busy||!available.length||!["ArrowUp","ArrowDown","ArrowLeft","ArrowRight","Home","End"].includes(key))return false;
  const index=available.findIndex(s=>s.sessionId===current.sessionId);
  const next=key==="Home"?available[0]:key==="End"?available.at(-1)!:available[(index+(["ArrowUp","ArrowLeft"].includes(key)?-1:1)+available.length)%available.length];
  choose(next);buttons.current.get(next.sessionId)?.focus();return true;
 }
 const stale=catalog?.sessions.filter(unavailable)??[];
 const groups=[...available.reduce((map,s)=>map.set(folder(s.cwd),[...(map.get(folder(s.cwd))??[]),s]),new Map<string,ClaudeSession[]>())];
 const row=(s:ClaudeSession)=><button key={s.sessionId} type="button" role="radio" className="v6-row" data-chosen={value.externalId===s.sessionId} aria-checked={value.externalId===s.sessionId} aria-label={`${title(s)} · ${s.cwd}`} tabIndex={value.externalId===s.sessionId||(!available.some(s=>s.sessionId===value.externalId)&&available[0]?.sessionId===s.sessionId)?0:-1} ref={node=>{if(node)buttons.current.set(s.sessionId,node);else buttons.current.delete(s.sessionId);}} disabled={busy||unavailable(s)} onClick={()=>choose(s)} onKeyDown={event=>{if(navigate(event.key,s))event.preventDefault();}}>
  <ConversationAvatar id={`c:${s.sessionId}`} kind="claude"/><span className="v6-row-body"><span className="v6-row-top"><strong>{title(s)}</strong>{!unavailable(s)&&s.checkedAt?<time>{clock(s.checkedAt)}</time>:null}</span><span className="v6-row-meta">{stateText(s)}{value.externalId===s.sessionId?` · ${t("当前")}`:""}{unavailable(s)?` · ${s.cwd}`:""}</span></span>
 </button>;
 return <section className="r2-claude-picker v6-claude-pick" aria-label="Claude Code">
  <p role="status" className="v6-muted">{t("Bridge 转给 Claude Code 前都由你审阅；Agbrio 不会自动发起模型请求。")}</p>
  {available.length>0?<div className="v6-claude-list" role="radiogroup" aria-label={t("选择已连接的 Claude Code 对话")}>
   {groups.map(([name,rows])=><div key={name} className="v6-claude-group"><h4>{name}</h4>{rows.map(row)}</div>)}
  </div>:<div className="v6-claude-empty">
   <strong>{t("还没有可选的 Claude Code 对话")}</strong>
   <p>{api.claudeConnect?t("在 Claude 桌面 App 的插件设置里开启 agbrio-bridge，然后在要绑定的对话里发一条消息，再点「刷新目录」。"):t("请在电脑端 Agbrio 连接 Claude Code；手机端可选择已连接对话。")}</p>
   {api.claudeConnect&&<button type="button" className="v6-primary" disabled={busy} onClick={()=>void connect()}>{busy?t("连接处理中…"):t("连接 Claude Code")}</button>}
  </div>}
  {stale.length>0&&<div className="v6-claude-group v6-claude-stale" role="group" aria-label={t("暂不可选")}><h4>{t("暂不可选 · 在 Claude 里打开并发一条消息后可选")}</h4>{stale.map(row)}</div>}
  {value.externalId&&!catalog?.sessions.some(s=>s.sessionId===value.externalId)&&<p className="v6-muted">{value.label||t("未命名对话")} · {t("状态待确认")}</p>}
  {catalog&&!catalog.complete&&<p role="status" className="v6-muted">{t("目录仅显示部分会话，不代表其他会话不存在。")}</p>}
  {connection&&<p role="status" className="v6-muted">{live?t("已连接，可按名称选择对话。"):t("连接配置已就绪，等待 Claude Code 登记对话。打开原对话，在方便时重新加载观察插件；如提示费用或需要强制加载，请停止。")}</p>}
  {error&&<p role="alert">{error}</p>}
  <button type="button" className="v6-swap" disabled={busy||!api.claudeSessions} onClick={()=>void refresh()}><RefreshCw size={14}/>{t("刷新目录")}</button>
  <details className="v6-claude-settings"><summary>{t("连接设置")}</summary>
   {api.claudeConnect&&live&&<button type="button" className="v6-swap" disabled={busy} onClick={()=>void connect()}>{busy?t("连接处理中…"):t("检查连接")}</button>}
   {connections.filter(c=>!c.revoked).map(c=><div className="v6-claude-setting" key={c.connectionId}><span>{t("观察连接")} · {c.connectionId.slice(0,8)} · {new Date(c.expiresAt).toLocaleDateString()}</span>{api.claudeRevoke&&<button type="button" className="v6-link-danger" disabled={busy} onClick={()=>{setBusy(true);void api.claudeRevoke!(c.connectionId).then(()=>{setConnections(v=>v.map(item=>item.connectionId===c.connectionId?{...item,revoked:true}:item));if(pairing?.connectionId===c.connectionId)setPairing(null);setConnection(null);setCatalog(null);}).catch(()=>setError(t("撤销连接失败。"))).finally(()=>setBusy(false));}}>{t("撤销此连接")}</button>}</div>)}
   {catalog?.sessions.length?<div className="v6-claude-ids">{catalog.sessions.map(s=><p key={s.sessionId}><span>{title(s)} · {s.cwd}</span><code>{s.sessionId}</code><small>Claude Code {s.version}</small></p>)}</div>:null}
   <p className="v6-muted">{t("手动配置仅用于排查。凭据只保存在本机，请勿发给 AI 助手或公开分享。")}</p>
   {api.claudeSetup&&<button type="button" className="v6-swap" disabled={busy||Boolean(pairing)} onClick={()=>{setBusy(true);setError(null);void api.claudeSetup!().then(setPairing).catch(()=>setError(t("只读连接配置生成失败。"))).finally(()=>setBusy(false));}}>{t("生成只读连接配置")}</button>}
   {pairing&&<>{pairing.pluginDirectory&&<label className="v6-field">{t("观察插件目录")}<input readOnly value={pairing.pluginDirectory}/></label>}<label className="v6-field">Host URL<input readOnly value={pairing.hostUrl}/></label><label className="v6-field">{t("私有连接凭据")}<input readOnly type="password" value={pairing.token} autoComplete="off"/></label><button type="button" className="v6-swap" disabled={busy} onClick={()=>{void navigator.clipboard.writeText(JSON.stringify({hostUrl:pairing.hostUrl,token:pairing.token},null,2)).catch(()=>setError(t("复制失败，请手动填写配置。")));}}>{t("复制私有配置")}</button></>}
  </details>
 </section>;
}
