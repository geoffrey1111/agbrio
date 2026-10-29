import {useEffect,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {t,useLanguage} from '../../i18n';
import {ArrowUp,ArrowDown,X} from 'lucide-react';
import {ResourceMonitorSettings} from './ResourceMonitorSettings';
type Target={threadId:string;generation:number;cwd:string};
type Settings={recoveryEnabled:boolean;alertTarget:Target|null;fallbackTargets?:Target[]};
type Attempt={id:string;kind:string;status:string;threadId:string;updatedAt:number};
type Watch=Target&{label:string};
type Status={settings:Settings;collectorAvailable:boolean;recent:Attempt[];targets:Watch[]};
export const localSafetyApi={read:()=>invoke<Status>('local_safety_status'),save:(settings:Settings)=>invoke<Status>('local_safety_save',{settings})};
export function LocalSafetySettings({api=localSafetyApi}:{api?:typeof localSafetyApi}){
 useLanguage();const [status,setStatus]=useState<Status|null>(null),[draft,setDraft]=useState<Settings|null>(null),[watches,setWatches]=useState<Watch[]>([]),[busy,setBusy]=useState(false),[error,setError]=useState(''),[saved,setSaved]=useState(false);
 useEffect(()=>{let active=true;void api.read().then(s=>{if(active){setStatus(s);setDraft(s.settings);setWatches(s.targets);}}).catch(()=>{if(active)setError(t('暂时无法读取保护设置，请重试。'));});return()=>{active=false;};},[api]);
 async function save(){if(!draft||busy)return;setBusy(true);setError('');setSaved(false);try{const value=await api.save(draft);setStatus(value);setDraft(value.settings);setWatches(value.targets);setSaved(true);}catch{setError(t('未能保存，请确认所选对话仍在监听后重试。'));}finally{setBusy(false);}}
 async function refresh(){if(busy)return;setBusy(true);try{const value=await api.read();setStatus(value);setWatches(value.targets);setDraft(old=>old??value.settings);setError('');}catch{setError(t('暂时无法读取保护设置，请重试。'));}finally{setBusy(false);}}
 const ordered=draft?[...(draft.alertTarget?[draft.alertTarget]:[]),...(draft.fallbackTargets??[])]:[];
 function changeOrder(next:Target[]){if(!draft)return;setDraft({...draft,alertTarget:next[0]??null,fallbackTargets:next.slice(1)});setSaved(false);}
 function move(index:number,by:number){const next=[...ordered],to=index+by;if(to<0||to>=next.length)return;[next[index],next[to]]=[next[to],next[index]];changeOrder(next);}
 const available=watches.filter(w=>!ordered.some(t=>t.threadId===w.threadId));
 const changed=!!draft&&JSON.stringify(draft)!==JSON.stringify(status?.settings);
 return <section className="v4-shared-codex local-safety-settings" aria-label={t('运行保护')}><h2>{t('运行保护')}</h2>{draft&&<>
  <label><input type="checkbox" checked={draft.recoveryEnabled} disabled={busy} onChange={e=>{setDraft({...draft,recoveryEnabled:e.target.checked});setSaved(false);}}/> {t('自动恢复 Codex 模型故障')}</label>
  <p>{t('适用于所有监听的 Codex 对话。明确的临时模型故障后等待 60 秒，继续原目标；每条对话 30 分钟内最多 2 次。手动暂停、额度不足和待决策不会自动恢复。')}</p>
  <ResourceMonitorSettings/>
  <label htmlFor="resource-alert-target">{t('资源告警接收对话')}</label>
  {!!ordered.length&&<ol className="local-safety-targets">{ordered.map((target,index)=>{const w=watches.find(w=>w.threadId===target.threadId&&w.generation===target.generation&&w.cwd===target.cwd);return <li key={target.threadId} className="local-safety-target"><span><small>{t(index===0?'主接收':'备用 {0}',index)}</small><strong>{w?.label??t('原接收对话已不可用，请重新选择')}</strong><small>{target.threadId.startsWith('claude_code:')?'Claude Code':'Codex'}</small></span><div><button type="button" disabled={busy||index===0} aria-label={t('上移接收对话 {0}',index+1)} onClick={()=>move(index,-1)}><ArrowUp size={18}/></button><button type="button" disabled={busy||index===ordered.length-1} aria-label={t('下移接收对话 {0}',index+1)} onClick={()=>move(index,1)}><ArrowDown size={18}/></button><button type="button" disabled={busy} aria-label={t('移除接收对话 {0}',index+1)} onClick={()=>changeOrder(ordered.filter((_,i)=>i!==index))}><X size={18}/></button></div></li>;})}</ol>}
  <select id="resource-alert-target" value="" disabled={busy||ordered.length>=8||!available.length} onChange={e=>{const w=available.find(w=>w.threadId===e.target.value);if(w)changeOrder([...ordered,{threadId:w.threadId,generation:w.generation,cwd:w.cwd}]);}}>
   <option value="">{t(ordered.length>=8?'最多绑定 8 个接收对话':ordered.length?'添加备用对话…':'暂不发送，之后选择管理对话')}</option>
   {available.map(w=><option value={w.threadId} key={w.threadId}>{w.label} · {w.threadId.startsWith('claude_code:')?'Claude Code':'Codex'}</option>)}
  </select>
  <p>{t('按顺序优先通知可用对话。主对话离线或确认无法处理时使用备用；送达结果不确定时先核对，不重复通知。可用上下箭头调整顺序。')}</p>
  <p>{t('内存或磁盘紧张时，只向你选择的管理对话发送资源摘要，不清理文件、不结束进程。电脑、Agbrio 和接收会话需要在线；Claude Code 还需加载新版 Agbrio 插件。')}</p>
  {!status?.collectorAvailable&&<p role="status">{t('尚未发现本机资源采集器。接入采集器后才能收到资源告警。')}</p>}
  <button type="button" disabled={busy||!changed} onClick={()=>void save()}>{t(busy?'正在保存…':'保存设置')}</button>{saved&&<span role="status">{t('已保存')}</span>}
  {!!status?.recent.length&&<details><summary>{t('最近保护记录')}</summary><button type="button" disabled={busy} onClick={()=>void refresh()}>{t('刷新状态')}</button><ul>{status.recent.slice(0,5).map(r=><li key={r.id}>{t(r.kind==='RECOVERY'?'模型故障恢复':'资源告警')} · {t(({READY:'等待处理',UNKNOWN:'结果待核对，不会重复发送',APPLIED:'已恢复原目标',SENT:'已提交',DELIVERED:'已送达',UNAVAILABLE:'接收方本轮失败，使用备用',REJECTED:'未执行，请查看对话状态',HISTORICAL:'历史告警，未推送',SUPERSEDED:'已由较新告警替代'} as Record<string,string>)[r.status]??'结果待核对，不会重复发送')} · <time>{new Date(r.updatedAt).toLocaleTimeString([],{hour:'2-digit',minute:'2-digit'})}</time></li>)}</ul></details>}
 </>}{error&&<><p role="alert">{error}</p><button type="button" disabled={busy} onClick={()=>void refresh()}>{t('重试')}</button></>}</section>;
}
