import {useEffect,useRef,useState} from "react";
import "../../styles/claude-code.css";
import {t,useLanguage} from "../../i18n";
import type {RoleInput} from "./RoleBridgePanel";
export type ClaudeSession={sessionId:string;title?:string|null;cwd:string;version:string;state:string;leaseUntil:number;checkedAt:number};
export type ClaudeCatalog={sessions:ClaudeSession[];complete:boolean;writesEnabled:boolean};
export type ClaudePairing={connectionId:string;hostUrl:string;pluginDirectory?:string;token:string;expiresAt:number;writesEnabled:boolean};
export type ClaudeConnection={connectionId:string;expiresAt:number;revoked:boolean;sessionCount:number};
export type ClaudeConnectionResult={phase:"CONNECTED"|"WAITING_SESSION";writesEnabled:boolean};
export type ClaudePickerApi={claudeSessions?:()=>Promise<ClaudeCatalog>;claudeConnections?:()=>Promise<ClaudeConnection[]>;claudeSetup?:()=>Promise<ClaudePairing>;claudeConnect?:()=>Promise<ClaudeConnectionResult>;claudeRevoke?:(id:string)=>Promise<void>};
export const providerLabel=(provider:string)=>provider==="CLAUDE_CODE"?"Claude Code":provider==="CODEX"?"Codex":"ChatGPT";
const unavailable=(s:ClaudeSession)=>s.leaseUntil<=Date.now()||s.state==="SESSION_END";
const stateText=(s:ClaudeSession)=>unavailable(s)?t("状态待确认"):t(({RUNNING:"执行中",WAITING_DECISION:"需要决策",COMPLETE:"回复已就绪",FAILED:"执行失败",INTERRUPTED:"已中断"} as Record<string,string>)[s.state]??"已连接");
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
  const added=next.sessions.filter(s=>!order.current.includes(s.sessionId)).sort((a,b)=>title(a).localeCompare(title(b))||a.sessionId.localeCompare(b.sessionId));
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
 return <section className="r2-claude-picker" aria-label="Claude Code">
  <p role="status">{t("Claude Code 当前仅观察；发送和中断尚未启用，不会自动消耗额度。")}</p>
  <div className="r2-claude-list" role="radiogroup" aria-label={t("选择已连接的 Claude Code 对话")}>
   {catalog?.sessions.map(s=><div className="r2-claude-row" key={s.sessionId}>
    <button type="button" role="radio" aria-checked={value.externalId===s.sessionId} aria-label={`${title(s)} · ${s.cwd}`} tabIndex={value.externalId===s.sessionId||(!available.some(s=>s.sessionId===value.externalId)&&available[0]?.sessionId===s.sessionId)?0:-1} ref={node=>{if(node)buttons.current.set(s.sessionId,node);else buttons.current.delete(s.sessionId);}} disabled={busy||unavailable(s)} onClick={()=>choose(s)} onKeyDown={event=>{if(navigate(event.key,s))event.preventDefault();}}>
     <span className="r2-claude-name">{title(s)}</span><span className="r2-claude-state">{stateText(s)}</span><span className="r2-claude-root">{s.cwd}</span>
    </button>
    <details className="r2-claude-identity"><summary>{t("身份详情")}</summary><code>{s.sessionId}</code><small>Claude Code {s.version}</small></details>
   </div>)}
  </div>
  {value.externalId&&!catalog?.sessions.some(s=>s.sessionId===value.externalId)&&<p>{value.label||t("未命名对话")} · {t("状态待确认")}</p>}
  {!catalog?.sessions.length&&<p>{t("尚无已连接对话。连接后打开原 Claude Code 对话即可，不需要发送测试消息。")}</p>}
  {catalog&&!catalog.complete&&<p role="status">{t("目录仅显示部分会话，不代表其他会话不存在。")}</p>}
  <div className="r2-claude-actions">
   {api.claudeConnect&&<button type="button" className="v3-primary" disabled={busy} onClick={()=>void connect()}>{busy?t("连接处理中…"):live?t("检查连接"):t("连接 Claude Code")}</button>}
   <button type="button" disabled={busy||!api.claudeSessions} onClick={()=>void refresh()}>{t("刷新目录")}</button>
  </div>
  {api.claudeConnect&&!live&&!connection&&<p className="r2-claude-hint">{t("连接会安装或配置 Agbrio 只读观察插件。不会发消息、重启 Claude 或修改原任务。")}</p>}
  {connection&&<p role="status">{live?t("已连接，可按名称选择对话。"):t("连接配置已就绪，等待 Claude Code 登记对话。打开原对话，在方便时重新加载观察插件；如提示费用或需要强制加载，请停止。")}</p>}
  {!api.claudeConnect&&<p>{t("请在电脑端 Agbrio 连接 Claude Code；手机端可选择已连接对话。")}</p>}
  <details className="r2-claude-manual"><summary>{t("手动连接与连接管理")}</summary>
   {connections.filter(c=>!c.revoked).map(c=><p key={c.connectionId}>{t("观察连接")} · {c.connectionId.slice(0,8)} · {new Date(c.expiresAt).toLocaleDateString()}{api.claudeRevoke&&<button type="button" disabled={busy} onClick={()=>{setBusy(true);void api.claudeRevoke!(c.connectionId).then(()=>{setConnections(v=>v.map(item=>item.connectionId===c.connectionId?{...item,revoked:true}:item));if(pairing?.connectionId===c.connectionId)setPairing(null);setConnection(null);setCatalog(null);}).catch(()=>setError(t("撤销连接失败。"))).finally(()=>setBusy(false));}}>{t("撤销此连接")}</button>}</p>)}
   <p>{t("手动配置仅用于排查。凭据只保存在本机，请勿发给 AI 助手或公开分享。")}</p>
   {api.claudeSetup?<button type="button" disabled={busy||Boolean(pairing)} onClick={()=>{setBusy(true);setError(null);void api.claudeSetup!().then(setPairing).catch(()=>setError(t("只读连接配置生成失败。"))).finally(()=>setBusy(false));}}>{t("生成只读连接配置")}</button>:<p>{t("请在电脑端 Agbrio 的绑定窗口生成连接配置。")}</p>}
   {pairing&&<>{pairing.pluginDirectory&&<label>{t("观察插件目录")}<input readOnly value={pairing.pluginDirectory}/></label>}<label>Host URL<input readOnly value={pairing.hostUrl}/></label><label>{t("私有连接凭据")}<input readOnly type="password" value={pairing.token} autoComplete="off"/></label><button type="button" disabled={busy} onClick={()=>{void navigator.clipboard.writeText(JSON.stringify({hostUrl:pairing.hostUrl,token:pairing.token},null,2)).catch(()=>setError(t("复制失败，请手动填写配置。")));}}>{t("复制私有配置")}</button></>}
  </details>
  {error&&<p role="alert">{error}</p>}
 </section>;
}
