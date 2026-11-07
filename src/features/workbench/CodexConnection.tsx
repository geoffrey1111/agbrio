import {useEffect,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {SharedCodexConnection} from './SharedCodexConnection';
import {t} from '../../i18n';

const nativeIdentity=()=>invoke<{platform?:string}>('computer_identity');
type Status={connected:boolean;connecting?:boolean;detail?:string};
const nativeConnect=()=>invoke<Status>('connect_codex');
const nativeStatus=()=>invoke<Status>('codex_status');
/** The host OS, never the phone/browser OS, selects the connection explanation. */
export function CodexConnection({identity=nativeIdentity,connect=nativeConnect,status=nativeStatus}:{identity?:()=>Promise<{platform?:string}>;connect?:()=>Promise<Status>;status?:()=>Promise<Status>}){
 const [platform,setPlatform]=useState<string>(),[failed,setFailed]=useState(false),[busy,setBusy]=useState(false),[connected,setConnected]=useState<boolean|null>(null),[connecting,setConnecting]=useState(false);
 useEffect(()=>{let live=true;void identity().then(value=>{if(live)setPlatform(value.platform);},()=>{if(live)setFailed(true);});return()=>{live=false;};},[identity]);
 useEffect(()=>{if(platform!=='macos')return;let live=true;const check=()=>void status().then(value=>{if(live){setConnected(value.connected);setConnecting(value.connecting===true);}},()=>{if(live){setConnecting(false);setFailed(true);}});check();const timer=setInterval(check,3000);return()=>{live=false;clearInterval(timer);};},[platform,status]);
 if(platform==='windows')return <SharedCodexConnection/>;
 async function open(){if(busy)return;setBusy(true);setFailed(false);try{const value=await connect();setConnected(value.connected);setConnecting(value.connecting===true);setFailed(!value.connected&&!value.connecting);}catch{setFailed(true);}finally{setBusy(false);}}
 return <section className="v4-shared-codex" aria-label={t('Codex 连接')}><h2>{t('Codex 连接')}</h2>
  {platform==='macos'?<><p>{t('Mac 使用独立 app-server，不提供 Windows 轻量连接或共享启动。Goal 控制和模型失败后的自动恢复仍可使用。')}</p><p role="status">{t(connected===null?'正在检查独立后台…':connected?'独立后台已连接':connecting?'连接中…':'独立后台尚未连接')}</p><button className="v3-primary" type="button" disabled={busy||connecting} onClick={()=>void open()}>{t(busy||connecting?'连接中…':connected?'重新检查连接':'连接 Codex')}</button></>:<p role="status">{t('正在读取电脑平台…')}</p>}
  {failed&&<p role="alert">{t('连接状态暂时无法读取。')}</p>}
 </section>;
}
