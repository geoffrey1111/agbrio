import {useCallback,useEffect,useRef,useState} from 'react';
import * as Dialog from '@radix-ui/react-dialog';
import {ChevronRight,RefreshCw,X} from 'lucide-react';
import {invoke} from '@tauri-apps/api/core';
import {request} from '../../mobile/api';
import {getLanguage,t,useLanguage} from '../../i18n';
import '../../styles/codex-quota.css';

export type QuotaWindow={slot:string;usedPercent:number;remainingPercent:number;windowDurationMins:number|null;resetsAt:number|null};
export type CodexQuotaSnapshot={status:'AVAILABLE'|'UNAVAILABLE';reason:string|null;observedAt:number;buckets:{id:string;windows:QuotaWindow[]}[]};
export type CodexQuotaApi={read:()=>Promise<CodexQuotaSnapshot>};
export const desktopQuotaApi:CodexQuotaApi={read:()=>invoke('codex_quota_read')};
export const mobileQuotaApi:CodexQuotaApi={read:()=>request('/codex-quota')};
export function quotaPeriod(minutes:number|null){
 if(minutes===10080)return t('每周');if(minutes===300)return t('5 小时');
 if(minutes===1440)return t('每日');if(minutes&&minutes%60===0)return t('{0} 小时',minutes/60);
 return minutes?t('{0} 分钟',minutes):t('当前周期');
}
const percent=(value:number)=>new Intl.NumberFormat(getLanguage(),{maximumFractionDigits:1}).format(value);
const timestamp=(ms:number)=>new Intl.DateTimeFormat(getLanguage(),{month:'numeric',day:'numeric',hour:'2-digit',minute:'2-digit'}).format(ms);
export function CodexQuota({api}:{api:CodexQuotaApi}){
 useLanguage();const [data,setData]=useState<CodexQuotaSnapshot|null>(null),[stale,setStale]=useState(false),[busy,setBusy]=useState(false),[open,setOpen]=useState(false);
 const pending=useRef(false),last=useRef(0),mounted=useRef(true),epoch=useRef(0);
 const refresh=useCallback(async(force=false)=>{
  if(pending.current||(!force&&Date.now()-last.current<60000)||document.visibilityState==='hidden')return;
  pending.current=true;last.current=Date.now();setBusy(true);const generation=epoch.current;
  try{const next=await api.read();if(!mounted.current||generation!==epoch.current)return;
   if(next.status==='UNAVAILABLE'&&['BACKEND_BUSY','BACKEND_OFFLINE'].includes(next.reason??'')){setStale(true);}
   else {setData(next);setStale(false);}
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
 const first=data?.status==='AVAILABLE'?data.buckets.find(b=>b.id==='codex')??data.buckets[0]:null;
 const firstWindow=first?.windows[0];
 const label=firstWindow?t('{0}剩余 {1}%',quotaPeriod(firstWindow.windowDurationMins),percent(firstWindow.remainingPercent)):busy?t('读取中…'):t('暂不可读取');
 return <Dialog.Root open={open} onOpenChange={value=>{setOpen(value);if(value)void refresh();}}>
  <Dialog.Trigger asChild><button type="button" className="r2-quota-row" aria-label={t('查看 Codex 额度')}><span>{t('Codex 额度')}</span><span>{stale&&firstWindow?t('上次读取 · {0}',label):label}</span><ChevronRight size={18}/></button></Dialog.Trigger>
  <Dialog.Portal><Dialog.Overlay className="r2-quota-overlay"/><Dialog.Content className="r2-quota-sheet r2-sheet" aria-describedby="quota-description">
   <div className="r2-quota-handle" aria-hidden="true"/><header><Dialog.Title>{t('Codex 额度')}</Dialog.Title><Dialog.Close asChild><button type="button" className="r2-icon" aria-label={t('关闭')}><X size={20}/></button></Dialog.Close></header>
   <Dialog.Description id="quota-description">{t('当前电脑连接的 Codex 账户额度。')}</Dialog.Description>
   {stale&&<p role="status">{t(firstWindow?'连接暂不可用，以下为上次读取结果。':'暂时无法读取，请检查电脑连接后重试。')}</p>}
   {data?.status==='AVAILABLE'?data.buckets.map(bucket=><section key={bucket.id} aria-label={bucket.id}>{data.buckets.length>1&&<h3>{bucket.id}</h3>}{bucket.windows.map(w=><div className="r2-quota-window" key={w.slot}><div><strong>{quotaPeriod(w.windowDurationMins)}</strong><span>{t('剩余 {0}%',percent(w.remainingPercent))}</span></div><div className="r2-quota-track" role="meter" aria-label={quotaPeriod(w.windowDurationMins)} aria-valuemin={0} aria-valuemax={100} aria-valuenow={w.remainingPercent}><i style={{width:`${w.remainingPercent}%`}}/></div><p>{w.resetsAt?t('重置于 {0}',timestamp(w.resetsAt*1000)):t('重置时间暂不可用')}</p></div>)}</section>):!busy&&<p role="status">{t('暂不可读取。部分登录方式不提供 Codex 额度。')}</p>}
   <div className="r2-quota-freshness">{data?.status==='AVAILABLE'&&<p className="r2-quota-read-at">{t('上次读取 {0}',timestamp(data.observedAt))}</p>}
   <button type="button" className="r2-quota-refresh" aria-label={t(busy?'读取中…':'刷新额度')} title={t(busy?'读取中…':'刷新额度')} disabled={busy} onClick={()=>void refresh(true)}><RefreshCw size={20} className={busy?'r2-quota-spinning':''}/></button></div>
   <p>{t('查看和刷新额度，不会打断正在执行的任务。')}</p>
  </Dialog.Content></Dialog.Portal>
 </Dialog.Root>;
}
