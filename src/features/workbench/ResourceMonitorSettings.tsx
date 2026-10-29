import {useState} from 'react';import {invoke} from '@tauri-apps/api/core';import {t} from '../../i18n';
type DiskRule={Drive:string;Enabled:boolean;FreePercent:number};
export type MonitorConfig={SchemaVersion:number;Enabled:boolean;SampleSeconds:number;AvailableMemoryPercent:number;CommitPercent:number;RepeatMinutes:number;Disks:DiskRule[];LegacyDiskFreeGiB?:number|null};
type Volume={drive:string;label:string;totalBytes:number;freeBytes:number;freePercent:number};
type Snapshot={config:MonitorConfig;drives:Volume[];availableBytes:number;physicalBytes:number;running:boolean;installed:boolean;legacy:boolean;startupEnabled:boolean};
export const monitorApi={read:()=>invoke<Snapshot>('resource_monitor_read'),apply:(config:MonitorConfig)=>invoke<Snapshot>('resource_monitor_apply',{config})};
const gib=(n:number)=>`${(n/1073741824).toFixed(1)} GB`;
export function ResourceMonitorSettings({api=monitorApi}:{api?:typeof monitorApi}){
 const[snapshot,setSnapshot]=useState<Snapshot|null>(null),[draft,setDraft]=useState<MonitorConfig|null>(null),[busy,setBusy]=useState(false),[error,setError]=useState(''),[saved,setSaved]=useState(false),[unconfirmed,setUnconfirmed]=useState(false);
 async function read(){if(busy)return;setBusy(true);setError('');try{const s=await api.read();setSnapshot(s);setDraft(s.config);setUnconfirmed(false);}catch{setError(t('暂时无法读取本机磁盘和内存，请重试。'));}finally{setBusy(false);}}
 async function save(){if(!draft||busy)return;setBusy(true);setError('');setSaved(false);try{const s=await api.apply(draft);setSnapshot(s);setDraft(s.config);setSaved(true);}catch(e){const code=String(e);if(code.includes('UNCONFIRMED'))setUnconfirmed(true);setError(t(code.includes('CONSENT_CANCELLED')?'已取消系统确认，预警设置未改变。':code.includes('UNCONFIRMED')?'应用结果尚未确认，请先刷新当前设置，不要重复保存。':'未能应用监测设置，请刷新状态后重试。'));}finally{setBusy(false);}}
 function disk(drive:string,change:Partial<DiskRule>){if(!draft)return;setDraft({...draft,Disks:draft.Disks.map(d=>d.Drive===drive?{...d,...change}:d)});setSaved(false);}
 const changed=!!draft&&JSON.stringify(draft)!==JSON.stringify(snapshot?.config);
 return <details className="resource-monitor-settings" onToggle={e=>{if(e.currentTarget.open&&!snapshot&&!busy)void read();}}><summary>{t('监测磁盘与预警线')}</summary>
  {busy&&!snapshot&&<p role="status">{t('正在识别本机磁盘和内存…')}</p>}
  {snapshot&&draft&&<><p>{t(snapshot.running?'资源监测正在运行':'资源监测尚未启用')}</p><label><input type="checkbox" checked={draft.Enabled} disabled={busy} onChange={e=>{setDraft({...draft,Enabled:e.target.checked});setSaved(false);}}/> {t('启用资源预警')}</label>
   <div className="resource-threshold"><label htmlFor="memory-free-alert">{t('内存剩余低于 {0}% 时提醒',draft.AvailableMemoryPercent)}</label><output htmlFor="memory-free-alert">{gib(snapshot.physicalBytes*draft.AvailableMemoryPercent/100)}</output><input id="memory-free-alert" type="range" aria-label={t('内存剩余预警百分比')} min="1" max="95" step="1" value={draft.AvailableMemoryPercent} disabled={busy} onChange={e=>{setDraft({...draft,AvailableMemoryPercent:e.target.valueAsNumber});setSaved(false);}}/><small>{t('当前可用 {0}，占总内存 {1}%',gib(snapshot.availableBytes),(100*snapshot.availableBytes/snapshot.physicalBytes).toFixed(1))}</small></div>
   <h3>{t('选择要监测的磁盘')}</h3>{snapshot.drives.map(v=>{const rule=draft.Disks.find(d=>d.Drive===v.drive);if(!rule)return null;return <div key={v.drive} className="resource-disk"><label><input type="checkbox" checked={rule.Enabled} disabled={busy} aria-label={t('监测磁盘 {0}',v.drive)} onChange={e=>disk(v.drive,{Enabled:e.target.checked})}/><strong>{v.drive} {v.label}</strong><small>{t('剩余 {0}%',v.freePercent.toFixed(1))}</small></label>{rule.Enabled&&<div className="resource-threshold"><label htmlFor={`disk-${v.drive[0]}-alert`}>{t('剩余低于 {0}% 时提醒',rule.FreePercent.toFixed(1))}</label><output>{gib(v.totalBytes*rule.FreePercent/100)}</output><input id={`disk-${v.drive[0]}-alert`} aria-label={t('磁盘 {0} 剩余预警百分比',v.drive)} type="range" min="0.1" max="95" step="0.1" value={rule.FreePercent} disabled={busy} onChange={e=>disk(v.drive,{FreePercent:e.target.valueAsNumber})}/><small>{t('当前剩余 {0} / 共 {1}',gib(v.freeBytes),gib(v.totalBytes))}</small></div>}</div>;})}
   {!snapshot.drives.length&&<p>{t('暂未发现可监测的本机磁盘。')}</p>}
   {snapshot.legacy&&<p>{t('当前仍使用旧的磁盘容量规则；保存后才应用这里的百分比设置。')}</p>}
   <p className="resource-monitor-note">{t('每 {0} 秒检查一次。另保留提交内存超过 {1}% 的保护线；只提醒，不清理或结束程序。保存时可能需要 Windows 管理员确认，不添加开机自启。',draft.SampleSeconds,draft.CommitPercent)}</p>
   <button type="button" disabled={busy||unconfirmed||(!changed&&snapshot.installed&&snapshot.running===draft.Enabled&&!snapshot.legacy)} onClick={()=>void save()}>{t(busy?'正在应用…':'保存并应用监测')}</button><button type="button" disabled={busy} onClick={()=>void read()}>{t('刷新当前状态')}</button>{saved&&<p role="status">{t('预警设置已应用。')}</p>}
  </>}{error&&<p role="alert">{error}</p>}{!snapshot&&error&&<button type="button" disabled={busy} onClick={()=>void read()}>{t('重试')}</button>}
 </details>;
}
