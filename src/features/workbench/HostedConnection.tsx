import {useEffect,useRef,useState} from 'react';
import {Cloud,Ticket} from 'lucide-react';
import {t,useLanguage,getLanguage} from '../../i18n';
export type HostedStatus={available:boolean;enabled:boolean;state:string;origin:string|null;expiresAt:number|null;kind?:string|null;serverNow?:number;trialAvailable?:boolean};
export type HostedApi={status:()=>Promise<HostedStatus>;redeem:(code:string)=>Promise<HostedStatus>;trial?:()=>Promise<HostedStatus>};
export function HostedConnection({api,onActivated,onModeChange,onAvailable,onBusyChange,hidden=false}:{api:HostedApi;onActivated:()=>Promise<void>;onModeChange?:(editing:boolean)=>void;onAvailable?:(available:boolean)=>void;onBusyChange?:(busy:boolean)=>void;hidden?:boolean}){
 useLanguage();const [status,setStatus]=useState<HostedStatus|null>(null),[claimBlocked,setClaimBlocked]=useState(false),[editing,setEditing]=useState(false),[code,setCode]=useState(''),[busy,setBusy]=useState(false),[error,setError]=useState(''),[now,setNow]=useState(Date.now());const pending=useRef(false),offset=useRef(0);
 const accept=(s:HostedStatus)=>{offset.current=typeof s.serverNow==='number'?s.serverNow-Date.now():0;setNow(Date.now()+offset.current);setStatus(s);};
 useEffect(()=>{let live=true;const check=()=>void api.status().then(s=>{if(live){accept(s);onAvailable?.(s.available);}}).catch(()=>{});check();const timer=setInterval(()=>{setNow(Date.now()+offset.current);if(!pending.current)check();},5000);return()=>{live=false;clearInterval(timer);};},[api,onAvailable]);
 if(!status?.available)return null;
 const isTrial=status.kind==='TRIAL',expired=status.state==='EXPIRED'||Boolean(status.expiresAt&&status.expiresAt<=now),hasLease=Boolean(status.origin),canTrial=Boolean(api.trial)&&status.trialAvailable!==false&&!hasLease&&!claimBlocked;
 const hours=status.expiresAt?Math.max(0,Math.ceil((status.expiresAt-now)/3600000)):0;
 const trialTime=hours<1?t('试用已结束'):t('试用剩余 {0} 天 {1} 小时',Math.floor(hours/24),hours%24);
 async function activate(trial:boolean){
  if(pending.current||!trial&&!code.trim())return;pending.current=true;setBusy(true);onBusyChange?.(true);setError('');
  try{const result=await (trial?api.trial!():api.redeem(code.trim()));accept(result);setCode('');setEditing(false);onModeChange?.(false);try{await onActivated();}catch{setError(t("连接已开通，请重新打开设备设置查看配对入口。"));}}
  catch(e){const text=String(e);if(trial&&text.includes("ALREADY_USED"))setClaimBlocked(true);setError(t(trial?(text.includes('ALREADY_USED')?'这台电脑已领取过试用，请使用激活码继续。':text.includes('TRIAL_EXPIRED')?'试用已结束，请联系作者获取激活码。':text.includes('CAPACITY')?'试用名额暂时已满，请稍后重试或联系作者。':text.includes('DEVICE_UNAVAILABLE')?'无法识别这台电脑，请使用激活码或联系作者。':'试用开通尚未完成，请重试。七天试用将在连接验证成功后开始。'):(text.includes('ALREADY_USED')?'激活码已在另一台电脑使用。':text.includes('CODE_EXPIRED')?'激活码已到期，请使用新的激活码。':text.includes('CODE_UNAVAILABLE')?'激活码已失效。':text.includes('UNCERTAIN')?'尚未确认开通结果，请用同一个码重试。':'连接未完成，原连接已保留。请用同一个码重试。')));}
  finally{pending.current=false;setBusy(false);onBusyChange?.(false);}
 }
 const edit=()=>{setEditing(true);onModeChange?.(true);setError('');};
 return <section hidden={hidden} className="r2-hosted-connection" aria-label={t('托管连接')} aria-busy={busy}>
  <div className="r2-hosted-heading"><Cloud size={20}/><strong>{t('使用作者服务器')}</strong>{hasLease&&!editing&&<button type="button" disabled={busy} onClick={edit}><Ticket size={18}/>{t('续期')}</button>}</div>
  {!editing&&<>
   {hasLease?<><p className="r2-hosted-status" role="status">{status.state==='REVOKED'?t('连接已停用'):expired?t(isTrial?'试用已结束':'已到期'):isTrial?trialTime:t(status.enabled?(status.state==='READY'?'已连接':'连接中…'):'正在使用自建连接')}</p>{status.expiresAt&&<p className="v4-meta">{t('至')} {new Date(status.expiresAt).toLocaleString(getLanguage(),{year:'numeric',month:'numeric',day:'numeric',hour:'2-digit',minute:'2-digit'})}</p>}{(expired||status.state==='REVOKED')&&<p className="v4-meta">{t('输入激活码即可继续；本地功能和数据不受影响。')}</p>}{isTrial&&!expired&&!status.enabled&&api.trial&&<button type="button" className="r2-action" disabled={busy} onClick={()=>void activate(true)}>{t(busy?'连接中…':'恢复托管连接')}</button>}</>:
    <><p className="v4-meta">{t('无需配置域名，开通后即可配对手机。')}</p>{canTrial&&<><button type="button" className="r2-action" disabled={busy} onClick={()=>void activate(true)}>{t(busy?'连接中…':'免费试用 7 天')}</button><p className="v4-meta">{t('每台电脑一次，连接成功后开始计时。')}</p></>}<button type="button" className={`r2-hosted-code${canTrial?"":" r2-action"}`} disabled={busy} onClick={edit}><Ticket size={16}/>{t('已有激活码')}</button></>}
   {(expired||status.state==='REVOKED'||Boolean(error))&&<a className="r2-hosted-contact" href="mailto:geoffreyzjx@qq.com">{t('联系作者获取激活码')}</a>}
  </>}
  {editing&&<form onSubmit={e=>{e.preventDefault();void activate(false);}}><label>{t('激活码')}<input aria-label={t('激活码')} type="text" value={code} onChange={e=>setCode(e.target.value)} disabled={busy} autoComplete="off" autoCapitalize="none" spellCheck={false}/></label><div className="r2-hosted-actions"><button className="r2-action" type="submit" disabled={busy||!code.trim()}>{t(busy?'连接中…':'开通连接')}</button><button type="button" disabled={busy} onClick={()=>{setEditing(false);onModeChange?.(false);setCode('');setError('');}}>{t('取消')}</button></div></form>}
  {error&&<p role="alert">{error}</p>}
 </section>;
}
