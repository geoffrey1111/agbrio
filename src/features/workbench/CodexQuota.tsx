import {useCallback,useEffect,useRef,useState} from 'react';
import * as Dialog from '@radix-ui/react-dialog';
import {ChevronRight,RefreshCw,X} from 'lucide-react';
import {invoke} from '@tauri-apps/api/core';
import {request} from '../../mobile/api';
import {getLanguage,t,useLanguage} from '../../i18n';
import '../../styles/codex-quota.css';

export type QuotaWindow={slot:string;usedPercent:number;remainingPercent:number;windowDurationMins:number|null;resetsAt:number|null;usedTokens?:number};
export type CodexQuotaSnapshot={status:'AVAILABLE'|'UNAVAILABLE';reason:string|null;observedAt:number;buckets:{id:string;windows:QuotaWindow[]}[]};
export type CodexQuotaApi={read:()=>Promise<CodexQuotaSnapshot>};
export const desktopQuotaApi:CodexQuotaApi={read:()=>invoke('codex_quota_read')};
export const mobileQuotaApi:CodexQuotaApi={read:()=>request('/codex-quota')};
export const desktopClaudeQuotaApi:CodexQuotaApi={read:()=>invoke('claude_quota_read')};
export const mobileClaudeQuotaApi:CodexQuotaApi={read:()=>request('/claude-quota')};
export function quotaPeriod(minutes:number|null){
 if(minutes===10080)return t('每周');if(minutes===300)return t('5 小时');
 if(minutes===1440)return t('每日');if(minutes&&minutes%60===0)return t('{0} 小时',minutes/60);
 return minutes?t('{0} 分钟',minutes):t('当前周期');
}
const tokens=(value:number)=>new Intl.NumberFormat(getLanguage(),{notation:'compact',maximumFractionDigits:1}).format(value);
const percent=(value:number)=>new Intl.NumberFormat(getLanguage(),{maximumFractionDigits:1}).format(value);
const timestamp=(ms:number)=>new Intl.DateTimeFormat(getLanguage(),{month:'numeric',day:'numeric',hour:'2-digit',minute:'2-digit'}).format(ms);
/** One provider's subscription windows. Codex reads its adapter; Claude Code reads its status line file. */
export function CodexQuota({api,active=true,provider='Codex'}:{api:CodexQuotaApi;active?:boolean;provider?:'Codex'|'Claude Code'}){
 useLanguage();const [data,setData]=useState<CodexQuotaSnapshot|null>(null),[offline,setOffline]=useState(false),[stale,setStale]=useState(false),[busy,setBusy]=useState(false),[open,setOpen]=useState(false);
 const pending=useRef(false),last=useRef(0),mounted=useRef(true),epoch=useRef(0);
 useEffect(()=>{if(!active)setOpen(false);},[active]);
 const refresh=useCallback(async(force=false)=>{
  if(pending.current||(!force&&Date.now()-last.current<60000)||document.visibilityState==='hidden')return;
  pending.current=true;last.current=Date.now();setBusy(true);const generation=epoch.current;
  try{const next=await api.read();if(!mounted.current||generation!==epoch.current)return;
   if(next.status==='UNAVAILABLE'&&['BACKEND_BUSY','BACKEND_OFFLINE'].includes(next.reason??'')){setStale(true);setOffline(next.reason==='BACKEND_OFFLINE');}
   else {setData(next);setStale(false);setOffline(false);}
  }catch{if(mounted.current&&generation===epoch.current)setStale(true);}
  finally{if(generation===epoch.current){pending.current=false;if(mounted.current)setBusy(false);}}
 },[api]);
 useEffect(()=>{
  mounted.current=true;pending.current=false;setData(null);setStale(false);last.current=0;void refresh();
  const foreground=()=>{if(document.visibilityState==='visible')void refresh();};
  const offline=()=>setStale(true);document.addEventListener('visibilitychange',foreground);window.addEventListener('pageshow',foreground);window.addEventListener('online',foreground);window.addEventListener('offline',offline);
  const timer=setInterval(foreground,300000);
  return()=>{mounted.current=false;epoch.current++;clearInterval(timer);document.removeEventListener('visibilitychange',foreground);window.removeEventListener('pageshow',foreground);window.removeEventListener('online',foreground);window.removeEventListener('offline',offline);};
 },[refresh]);
 const first=data?.status==='AVAILABLE'?data.buckets.find(b=>b.id==='codex'||b.id==='claude')??data.buckets[0]:null;
 const notEnabled=data?.status==='UNAVAILABLE'&&data.reason==='NOT_ENABLED';
 const firstWindow=first?.windows[0];
 const label=firstWindow?(firstWindow.usedTokens!==undefined?t('{0}已用 {1} tokens',quotaPeriod(firstWindow.windowDurationMins),tokens(firstWindow.usedTokens)):t('{0}剩余 {1}%',quotaPeriod(firstWindow.windowDurationMins),percent(firstWindow.remainingPercent))):busy?t('读取中…'):notEnabled?t('未开启'):offline?t('等待连接'):t('暂不可读取');
 return <Dialog.Root open={open} onOpenChange={value=>{setOpen(value);if(value)void refresh();}}>
  <Dialog.Trigger asChild><button type="button" className="r2-quota-row" aria-label={t('查看 {0} 额度',provider)}><span>{t('{0} 额度',provider)}</span><span>{stale&&firstWindow?t('上次读取 · {0}',label):label}</span><ChevronRight size={18}/></button></Dialog.Trigger>
  <Dialog.Portal><Dialog.Overlay className="r2-quota-overlay"/><Dialog.Content className="r2-quota-sheet r2-sheet" aria-describedby="quota-description">
   <div className="r2-quota-handle" aria-hidden="true"/><header><Dialog.Title>{t('{0} 额度',provider)}</Dialog.Title><Dialog.Close asChild><button type="button" className="r2-icon" aria-label={t('关闭')}><X size={20}/></button></Dialog.Close></header>
   <Dialog.Description id="quota-description">{provider==='Codex'?t('当前电脑连接的 Codex 账户额度。'):data?.reason==='DESKTOP_APP'?t('Claude 订阅额度，与 Claude 桌面 App「使用情况」一致；显示剩余比例。'):data?.reason==='LOCAL_ESTIMATE'?t('Claude Code 订阅用量。'):t('Claude 订阅额度，来自电脑上 Claude Code 的状态栏；显示剩余比例。')}</Dialog.Description>
   {data?.reason==='LOCAL_ESTIMATE'&&<p role="status">{t('按这台电脑上的对话记录统计已用 tokens，不代表订阅额度百分比。打开 Claude 桌面 App 的「使用情况」更新额度后，可在这里刷新读取。')}</p>}
   {notEnabled&&<p role="status">{t('还没有读到用量。请打开 Claude 桌面 App 的「使用情况」后刷新；只有本机取得真实额度时才显示百分比。')}</p>}
   {stale&&<p role="status">{firstWindow?t('连接暂不可用，以下为上次读取结果。'):offline?t('Agbrio 还没有连上 Codex（例如刚重启）。打开任意 Codex 对话或 Bridge 后会自动读取，不会为读额度单独启动 Codex。'):t('暂时无法读取，请检查电脑连接后重试。')}</p>}
   {data?.status==='AVAILABLE'?data.buckets.map(bucket=><section key={bucket.id} aria-label={bucket.id}>{data.buckets.length>1&&<h3>{bucket.id}</h3>}{bucket.windows.map(w=>w.usedTokens!==undefined?<div className="r2-quota-window" key={w.slot}><div><strong>{w.windowDurationMins===10080?t('近 7 天'):t('当前 5 小时')}</strong><span>{t('已用 {0} tokens',tokens(w.usedTokens))}</span></div><p>{w.resetsAt?t('重置于 {0}',timestamp(w.resetsAt*1000)):w.windowDurationMins===300?t('当前没有进行中的 5 小时窗口'):t('滚动统计')}</p></div>:<div className="r2-quota-window" key={w.slot}><div><strong>{quotaPeriod(w.windowDurationMins)}</strong><span>{t('剩余 {0}%',percent(w.remainingPercent))}</span></div><div className="r2-quota-track" role="meter" aria-label={quotaPeriod(w.windowDurationMins)} aria-valuemin={0} aria-valuemax={100} aria-valuenow={w.remainingPercent}><i style={{width:`${w.remainingPercent}%`}}/></div><p>{w.resetsAt?t('重置于 {0}',timestamp(w.resetsAt*1000)):t('重置时间暂不可用')}</p></div>)}</section>):!busy&&!notEnabled&&<p role="status">{provider==='Codex'?t('暂不可读取。部分登录方式不提供 Codex 额度。'):t('暂不可读取。Claude Code 订阅（Pro/Max）在回复后才会提供用量。')}</p>}
   <div className="r2-quota-freshness">{data?.status==='AVAILABLE'&&<p className="r2-quota-read-at">{t('上次读取 {0}',timestamp(data.observedAt))}</p>}
   <button type="button" className="r2-quota-refresh" aria-label={t(busy?'读取中…':'刷新额度')} title={t(busy?'读取中…':'刷新额度')} disabled={busy} onClick={()=>void refresh(true)}><RefreshCw size={20} className={busy?'r2-quota-spinning':''}/></button></div>
   <p>{t('查看和刷新额度，不会打断正在执行的任务。')}</p>
  </Dialog.Content></Dialog.Portal>
 </Dialog.Root>;
}
