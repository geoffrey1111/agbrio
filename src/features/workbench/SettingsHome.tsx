import {useEffect,useRef,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {Bot,ChevronRight,Monitor,Smartphone,Download} from 'lucide-react';
import {t,tc,useLanguage} from '../../i18n';
import {FirstRunLauncher} from '../onboarding/DesktopFirstRun';
import type {CodexConnectionStatus} from './CodexConnection';
import type {UpdateView} from './DesktopUpdateSettings';
import type {RuntimePresentation} from './models';

/** Settings › 概览 reads only existing interfaces, in parallel, and shows the
 * backend's actual readiness, never a saved preference. */
export type OverviewApi={
 shared:()=>Promise<CodexConnectionStatus>;
 pi:()=>Promise<{installed:boolean;administratorBackend?:{ready:boolean;preparing?:boolean}}>;
 prepareAdministrator?:()=>Promise<{installed:boolean;administratorBackend?:{ready:boolean;preparing?:boolean}}>;
 devices:()=>Promise<unknown[]>;
 update:()=>Promise<UpdateView>;
};
export const desktopOverviewApi:OverviewApi={shared:()=>invoke('codex_connection_status'),pi:()=>invoke('pi_settings'),prepareAdministrator:()=>invoke('pi_prepare_administrator'),devices:()=>invoke('web_paired_devices'),update:()=>invoke('desktop_update_status')};
type Read<T>={value?:T;failed?:boolean};
type Snapshot={shared:Read<CodexConnectionStatus>;pi:Read<Awaited<ReturnType<OverviewApi['pi']>>>;devices:Read<unknown[]>;update:Read<UpdateView>};
const settle=<T,>(r:PromiseSettledResult<T>):Read<T>=>r.status==='fulfilled'?{value:r.value}:{failed:true};
export type OverviewTarget='CODEX'|'PI_SETTINGS'|'DEVICES'|'UPDATE';

function Status({tone,children}:{tone:'ok'|'need'|'muted';children:string}){return <span className="v7-set-status" data-tone={tone}>{children}</span>;}
export function DesktopOverview({api,onNavigate,onPair}:{api:OverviewApi;onNavigate:(target:OverviewTarget)=>void;onPair?:()=>void}){
 useLanguage();
 const [snap,setSnap]=useState<Snapshot|null>(null),[busy,setBusy]=useState(false),[notice,setNotice]=useState(''),[error,setError]=useState('');
 const alive=useRef(true),pending=useRef(false);
 async function load(){const [shared,pi,devices,update]=await Promise.allSettled([api.shared(),api.pi(),api.devices(),api.update()]);if(alive.current)setSnap({shared:settle(shared),pi:settle(pi),devices:settle(devices),update:settle(update)});}
 useEffect(()=>{alive.current=true;void load();const timer=setInterval(()=>{if(document.visibilityState!=='hidden')void load();},15000);return()=>{alive.current=false;clearInterval(timer);};},[api]);
 async function prepare(){if(!api.prepareAdministrator||pending.current)return;pending.current=true;setBusy(true);setNotice('');setError('');try{await api.prepareAdministrator();if(alive.current)setNotice(t('管理员后台已就绪。'));}catch(reason){if(alive.current)setError(t(String(reason).includes('UAC_CANCELLED')?'已取消 Windows 授权，未启动管理员后台。':'管理员后台没有准备好。请到 Pi 子任务页查看后重试。'));}finally{pending.current=false;if(alive.current){setBusy(false);void load();}}}
 const shared=snap?.shared.value,pi=snap?.pi.value,devices=snap?.devices.value,update=snap?.update.value;
 const sharedText=!snap?t('正在读取…'):snap.shared.failed?t('暂时无法读取'):shared?.state==='UNSUPPORTED'?t('这个平台不需要设置'):shared?.state==='SHARED_RETIRING'||shared?.state==='SHARED_RETIREMENT_BLOCKED'?t('旧的共享后台未停用'):shared?.ready&&shared.desktopConnected?t('已接入'):shared?.ready?t('等待打开 Codex'):t('未就绪');
 const sharedTone=shared?.ready&&shared.desktopConnected?'ok':'muted';
 const piReady=Boolean(pi?.administratorBackend?.ready);
 const piText=!snap?t('正在读取…'):snap.pi.failed?t('暂时无法读取'):!pi?.installed?t('本机尚未安装 Pi'):piReady?t('已就绪'):pi.administratorBackend?.preparing?t('正在准备'):t('未准备');
 const deviceText=!snap?t('正在读取…'):snap.devices.failed?t('暂时无法读取'):devices?.length?tc('{0} 台已登录',devices.length):t('还没有配对的手机');
 const updateText=!snap?t('正在读取…'):snap.update.failed?t('暂时无法读取'):update?.state==='UP_TO_DATE'?t('已是最新'):update?.state==='AVAILABLE'||update?.state==='MANUAL_AVAILABLE'?t('有新版本 {0}',update.version??''):update?.state==='READY'?t('更新已下载并验证签名'):t('可以检查新版本');
 return <section className="v7-overview" aria-label={t('概览')}>
  <FirstRunLauncher/>
  <h3 className="v7-set-cap">{t('这台电脑')}</h3>
  <div className="v7-set-group">
   <div className="v7-set-item"><span className="v7-set-icon"><Monitor size={18} aria-hidden/></span><button type="button" className="v7-set-main" onClick={()=>onNavigate('CODEX')}><strong>{t('Codex 连接')}</strong><small><Status tone={sharedTone}>{sharedText}</Status></small></button><button type="button" className="v7-set-btn" onClick={()=>onNavigate('CODEX')}>{t('管理')}</button></div>
   <div className="v7-set-item"><span className="v7-set-icon"><Bot size={18} aria-hidden/></span><button type="button" className="v7-set-main" onClick={()=>onNavigate('PI_SETTINGS')}><strong>{t('Pi 管理员后台')}</strong><small><Status tone={piReady?'ok':pi?.installed?'need':'muted'}>{piText}</Status>{pi?.installed&&!piReady?` · ${t('新 Pi 子任务需要它；重启 Agbrio 后要重新准备')}`:''}</small></button>{pi?.installed&&!piReady&&api.prepareAdministrator&&<button type="button" className="v7-set-btn" data-primary disabled={busy} onClick={()=>void prepare()}>{t(busy?'请在 Windows 弹窗中确认…':'准备')}</button>}</div>
   <div className="v7-set-item"><span className="v7-set-icon"><Smartphone size={18} aria-hidden/></span><button type="button" className="v7-set-main" onClick={()=>onPair?onPair():onNavigate('DEVICES')}><strong>{t('手机')}</strong><small><Status tone={devices?.length?'ok':'muted'}>{deviceText}</Status></small></button>{onPair&&<button type="button" className="v7-set-btn" onClick={onPair}>{t('配对新手机')}</button>}</div>
   <div className="v7-set-item"><span className="v7-set-icon"><Download size={18} aria-hidden/></span><button type="button" className="v7-set-main" onClick={()=>onNavigate('UPDATE')}><strong>{t('Agbrio 版本')} {update?.currentVersion??''}</strong><small><Status tone={update?.state==='UP_TO_DATE'?'ok':update?.state==='READY'||update?.state==='AVAILABLE'?'need':'muted'}>{updateText}</Status></small></button><button type="button" className="v7-set-btn" onClick={()=>onNavigate('UPDATE')}>{t('检查更新')}</button></div>
  </div>
  {notice&&<p role="status">{notice}</p>}{error&&<p role="alert">{error}</p>}
 </section>;
}
/** Phone: the computer this phone is paired with, from existing runtime checks. */
export function ComputerStatusCard({runtime,identity,onOpen}:{runtime?:RuntimePresentation|null;identity?:()=>Promise<{hostName:string|null}>;onOpen?:()=>void}){
 useLanguage();
 // Transport T7: an older Host has no identity read; the card then says "电脑".
 const [hostName,setHostName]=useState<string|null>(null);
 useEffect(()=>{let alive=true;void identity?.().then(r=>{if(alive&&typeof r?.hostName==='string')setHostName(r.hostName.slice(0,64));},()=>undefined);return()=>{alive=false;};},[identity]);
 const check=(id:string)=>runtime?.checks.find(c=>c.id===id)?.state;
 // The phone reports router-core. Missing/checking fields are not proof of a disconnect.
 const host=check('router-core')??check('router-host'),codex=check('codex');
 const hostUnavailable=host==='UNAVAILABLE',codexUnavailable=codex==='UNAVAILABLE';
 if(!hostUnavailable&&!codexUnavailable)return null;
 const text=hostUnavailable?t('暂时连不上'):`${t('Codex 连接')} · ${t('未就绪')}`;
 const body=<><span className="v7-set-icon"><Monitor size={18} aria-hidden/></span><span className="v7-set-main"><strong>{hostName||t('电脑')}</strong><small><span className="v7-set-status" data-tone="muted">{text}</span></small></span>{onOpen&&<ChevronRight size={18} aria-hidden/>}</>;
 return onOpen?<button type="button" className="v7-computer-card" onClick={onOpen} aria-label={t('电脑连接状态')}>{body}</button>:<div className="v7-computer-card">{body}</div>;
}
