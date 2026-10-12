import {useEffect,useState} from "react";
import "../../styles/claude-code.css";
import {t,useLanguage} from "../../i18n";
import type {RoleInput} from "./RoleBridgePanel";
export type ClaudeSession={sessionId:string;cwd:string;version:string;state:string;leaseUntil:number;checkedAt:number};
export type ClaudeCatalog={sessions:ClaudeSession[];complete:boolean;writesEnabled:boolean};
export type ClaudePairing={connectionId:string;hostUrl:string;pluginDirectory?:string;token:string;expiresAt:number;writesEnabled:boolean};
export type ClaudeConnection={connectionId:string;expiresAt:number;revoked:boolean;sessionCount:number};
export type ClaudePickerApi={claudeSessions?:()=>Promise<ClaudeCatalog>;claudeConnections?:()=>Promise<ClaudeConnection[]>;claudeSetup?:()=>Promise<ClaudePairing>;claudeRevoke?:(id:string)=>Promise<void>};
export const providerLabel=(provider:string)=>provider==="CLAUDE_CODE"?"Claude Code":provider==="CODEX"?"Codex":"ChatGPT";
export function ClaudeSessionPicker({api,value,onChange}:{api:ClaudePickerApi;value:RoleInput;onChange:(input:RoleInput)=>void}){
 useLanguage();
 const[catalog,setCatalog]=useState<ClaudeCatalog|null>(null),[error,setError]=useState<string|null>(null),[busy,setBusy]=useState(false),[pairing,setPairing]=useState<ClaudePairing|null>(null);
 const[connections,setConnections]=useState<ClaudeConnection[]>([]);
 useEffect(()=>{let alive=true;if(api.claudeConnections)void api.claudeConnections().then(v=>{if(alive)setConnections(v);}).catch(()=>{});return()=>{alive=false;};},[api.claudeConnections]);
 async function refresh(){if(!api.claudeSessions)return;setBusy(true);setError(null);try{setCatalog(await api.claudeSessions());}catch{setError(t("Claude Code 目录暂不可用，请确认 Host 已更新。"));}finally{setBusy(false);}}
 useEffect(()=>{let alive=true;if(api.claudeSessions)void api.claudeSessions().then(v=>{if(alive)setCatalog(v);}).catch(()=>{if(alive)setError(t("Claude Code 目录暂不可用，请确认 Host 已更新。"));});return()=>{alive=false;};},[api.claudeSessions]);
 return <section className="r2-claude-picker" aria-label="Claude Code">
  <p role="status">{t("Claude Code 当前仅观察；发送和中断尚未启用，不会自动消耗额度。")}</p>
  <label>{t("选择已连接的 Claude Code 对话")}<select value={value.externalId} disabled={busy} onChange={event=>{const s=catalog?.sessions.find(s=>s.sessionId===event.target.value);if(s)onChange({provider:"CLAUDE_CODE",externalId:s.sessionId,label:`Claude Code · ${s.cwd.split(/[\\/]/).filter(Boolean).at(-1)||s.sessionId.slice(0,8)}`,cwd:s.cwd});}}>
   <option value="">{t("选择对话")}</option>
   {value.externalId&&!catalog?.sessions.some(s=>s.sessionId===value.externalId)&&<option value={value.externalId} disabled>{value.label||value.externalId} · {t("状态待确认")}</option>}
   {catalog?.sessions.map(s=><option key={s.sessionId} value={s.sessionId} disabled={s.leaseUntil<=Date.now()||s.state==="SESSION_END"}>{s.cwd} · {s.sessionId.slice(0,8)}{s.leaseUntil<=Date.now()?` · ${t("状态待确认")}`:""}</option>)}
  </select></label>
  {!catalog?.sessions.length&&<p>{t("尚无已连接对话。需在 Claude 桌面 Code 中安装 Agbrio 观察插件；不需要发送测试消息。")}</p>}
  {catalog&&!catalog.complete&&<p role="status">{t("目录仅显示部分会话，不代表其他会话不存在。")}</p>}
  <button type="button" disabled={busy||!api.claudeSessions} onClick={()=>void refresh()}>{t("刷新目录")}</button>
  <details><summary>{t("Claude Code 连接设置")}</summary>
   {connections.filter(c=>!c.revoked).map(c=><p key={c.connectionId}>{t("观察连接")} · {c.connectionId.slice(0,8)} · {new Date(c.expiresAt).toLocaleDateString()}{api.claudeRevoke&&<button type="button" disabled={busy} onClick={()=>{setBusy(true);void api.claudeRevoke!(c.connectionId).then(()=>{setConnections(v=>v.map(item=>item.connectionId===c.connectionId?{...item,revoked:true}:item));if(pairing?.connectionId===c.connectionId)setPairing(null);setCatalog(null);}).catch(()=>setError(t("撤销连接失败。"))).finally(()=>setBusy(false));}}>{t("撤销此连接")}</button>}</p>)}
   <p>{t("需要支持 Mods 的 Claude Code。此操作只生成只读连接配置，不安装插件、不修改 Claude 设置。")}</p>
   {api.claudeSetup?<button type="button" disabled={busy||Boolean(pairing)} onClick={()=>{setBusy(true);setError(null);void api.claudeSetup!().then(setPairing).catch(()=>setError(t("只读连接配置生成失败。"))).finally(()=>setBusy(false));}}>{t("生成只读连接配置")}</button>:<p>{t("请在电脑端 Agbrio 的绑定窗口生成连接配置。")}</p>}
   {pairing&&<>{pairing.pluginDirectory&&<label>{t("观察插件目录")}<input readOnly value={pairing.pluginDirectory}/></label>}<label>Host URL<input readOnly value={pairing.hostUrl}/></label><label>{t("私有连接凭据")}<input readOnly type="password" value={pairing.token} autoComplete="off"/></label><p>{t("将本机地址和凭据填入观察插件的配置；不要发送给 AI 助手或公开分享。")}</p><button type="button" disabled={busy} onClick={()=>{void navigator.clipboard.writeText(JSON.stringify({hostUrl:pairing.hostUrl,token:pairing.token},null,2)).catch(()=>setError(t("复制失败，请手动填写配置。")));}}>{t("复制私有配置")}</button>{api.claudeRevoke&&<button type="button" disabled={busy} onClick={()=>{setBusy(true);void api.claudeRevoke!(pairing.connectionId).then(()=>{setPairing(null);setCatalog(null);}).catch(()=>setError(t("撤销连接失败。"))).finally(()=>setBusy(false));}}>{t("撤销此连接")}</button>}</>}
  </details>
  {error&&<p role="alert">{error}</p>}
 </section>;
}
