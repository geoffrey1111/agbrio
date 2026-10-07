import {useEffect,useRef,useState} from 'react';
import {Cloud,Ticket} from 'lucide-react';
import {t,useLanguage,getLanguage} from '../../i18n';
export type HostedStatus={available:boolean;enabled:boolean;state:string;origin:string|null;expiresAt:number|null};
export type HostedApi={status:()=>Promise<HostedStatus>;redeem:(code:string)=>Promise<HostedStatus>};
export function HostedConnection({api,onActivated,onModeChange,onAvailable}:{api:HostedApi;onActivated:()=>Promise<void>;onModeChange?:(editing:boolean)=>void;onAvailable?:(available:boolean)=>void}){
 useLanguage();const [status,setStatus]=useState<HostedStatus|null>(null),[editing,setEditing]=useState(false),[code,setCode]=useState(''),[busy,setBusy]=useState(false),[error,setError]=useState('');const pending=useRef(false);
 useEffect(()=>{let live=true;const check=()=>void api.status().then(s=>{if(live){setStatus(s);onAvailable?.(s.available);}}).catch(()=>{});check();const timer=setInterval(check,5000);return()=>{live=false;clearInterval(timer);};},[api,onAvailable]);
 if(!status?.available)return null;
 async function redeem(){if(pending.current||!code.trim())return;pending.current=true;setBusy(true);setError('');try{const result=await api.redeem(code.trim());setStatus(result);setCode('');setEditing(false);onModeChange?.(false);await onActivated();}catch(e){const text=String(e);setError(t(text.includes('ALREADY_USED')?'兑换码已在另一台电脑使用。':text.includes('CODE_EXPIRED')?'兑换码已到期，请使用新的兑换码。':text.includes('CODE_UNAVAILABLE')?'兑换码已失效。':text.includes('UNCERTAIN')?'尚未确认开通结果，请用同一个码重试。':'连接未完成，原连接已保留。请用同一个码重试。'));}finally{pending.current=false;setBusy(false);}}
 return <section className="r2-hosted-connection" aria-label={t('托管连接')}><div className="r2-hosted-heading"><Cloud size={20}/><strong>{t('托管连接')}</strong>{!editing&&<button type="button" disabled={busy} onClick={()=>{setEditing(true);onModeChange?.(true);}}><Ticket size={18}/>{t(status.enabled?'续期':'兑换码')}</button>}</div>
 {status.enabled&&<p className="v4-meta" role="status">{t(status.state==='REVOKED'?'连接已停用':status.state==='EXPIRED'?'已到期':status.state==='READY'?'已连接':'连接中…')}{status.expiresAt?` · ${t('至')} ${new Date(status.expiresAt).toLocaleDateString(getLanguage())}`:''}</p>}
 {editing?<form onSubmit={e=>{e.preventDefault();void redeem();}}><label>{t('兑换码')}<input aria-label={t('兑换码')} type="text" value={code} onChange={e=>setCode(e.target.value)} disabled={busy} autoComplete="off" autoCapitalize="none" spellCheck={false}/></label><p className="v4-meta">{t('无需域名配置；开通后配对自己的手机。')}</p><div className="r2-hosted-actions"><button className="r2-action" type="submit" disabled={busy||!code.trim()}>{t(busy?'连接中…':'开通连接')}</button><button type="button" disabled={busy} onClick={()=>{setEditing(false);onModeChange?.(false);setCode('');setError('');}}>{t('取消')}</button></div></form>:!status.enabled&&<p className="v4-meta">{t('输入兑换码，即可使用托管连接。')}</p>}
 {error&&<p role="alert">{error}</p>}</section>;
}
