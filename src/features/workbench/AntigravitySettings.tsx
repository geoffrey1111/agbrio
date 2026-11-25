import {useEffect,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {t,useLanguage} from '../../i18n';
export type PermissionView={defaultMode:'ASK'|'FULL_ACCESS';runningMode:'ASK'|'FULL_ACCESS'|null;pendingNextLaunch:boolean};
export type PermissionApi={read:()=>Promise<PermissionView>;save:(mode:PermissionView['defaultMode'])=>Promise<PermissionView>};
export const antigravityPermissionApi:PermissionApi={read:()=>invoke('antigravity_permissions'),save:mode=>invoke('antigravity_permissions_save',{mode})};
export function AntigravitySettings({api=antigravityPermissionApi}:{api?:PermissionApi}){
 useLanguage();const [view,setView]=useState<PermissionView|null>(null),[mode,setMode]=useState<PermissionView['defaultMode']>('ASK'),[busy,setBusy]=useState(false),[error,setError]=useState(''),[saved,setSaved]=useState(false),[uncertain,setUncertain]=useState(false);
 useEffect(()=>{let alive=true;void api.read().then(v=>{if(alive){setView(v);setMode(v.defaultMode);}},()=>{if(alive)setError(t('暂时无法读取 AGY 权限，请刷新状态。'));});return()=>{alive=false;};},[api]);
 async function refresh(){if(busy)return;setBusy(true);setError('');try{const v=await api.read();setView(v);setMode(v.defaultMode);setUncertain(false);}catch{setError(t('暂时无法读取 AGY 权限，请刷新状态。'));}finally{setBusy(false);}}
 async function save(){if(busy||uncertain||!view)return;setBusy(true);setError('');setSaved(false);try{const v=await api.save(mode);setView(v);setMode(v.defaultMode);setSaved(true);}catch{setUncertain(true);setError(t('保存结果尚未确认，请刷新状态核实。不会自动重试或重启 AGY。'));}finally{setBusy(false);}}
 const label=(v:PermissionView['defaultMode'])=>t(v==='FULL_ACCESS'?'完全访问':'逐次询问');
 return <section className="v4-shared-codex" aria-label={t('AGY 默认权限')}>
  <p>{t('为这台电脑的 AGY 保存一次默认权限，后续对话和项目自动沿用。')}</p>
  {view?<><label><input type="checkbox" checked={mode==='FULL_ACCESS'} disabled={busy||uncertain} onChange={e=>{setMode(e.target.checked?'FULL_ACCESS':'ASK');setSaved(false);}}/>{t('默认完全访问')}</label>
   <p>{t('已保存的默认模式：')}{label(view.defaultMode)}</p>
   <p>{t('当前 AGY 进程：')}{view.runningMode?label(view.runningMode):t('尚未启动，将使用默认模式')}</p>
   {view.pendingNextLaunch&&<p role="status">{t('默认值已保存，当前进程仍使用原模式；下次启动 AGY 时生效。')}</p>}
   <p className="v4-meta">{t('完全访问使用 AGY 原生免确认模式，不会授予 Windows 管理员权限。保存设置不会停止任务或自动重启 AGY。')}</p>
   <button type="button" className="v3-primary" disabled={busy||uncertain||mode===view.defaultMode} onClick={()=>void save()}>{t(busy?'正在处理…':'保存默认权限')}</button>
  </>:!error&&<p role="status">{t('正在读取权限…')}</p>}
  <button type="button" disabled={busy} onClick={()=>void refresh()}>{t('刷新状态')}</button>
  {saved&&<p role="status">{t('默认权限已保存。')}</p>}{error&&<p role="alert">{error}</p>}
 </section>;
}
