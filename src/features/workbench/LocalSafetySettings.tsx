import {useEffect,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {t,useLanguage} from '../../i18n';
import {ArrowUp,ArrowDown,X,Plus,Search,RefreshCw} from 'lucide-react';
import {ResourceMonitorSettings,monitorApi} from './ResourceMonitorSettings';
import {ConversationAvatar} from './ConversationHome';
import {desktopNotificationApi} from './notificationApi';
import type {ExistingCodexThreadCatalog,ExistingCodexThreadCandidate} from '../codex/types';
import '../../styles/local-safety.css';
type Target={threadId:string;generation:number;cwd:string};
type Settings={recoveryEnabled:boolean;alertTarget:Target|null;fallbackTargets?:Target[]};
type Attempt={id:string;kind:string;status:string;threadId:string;updatedAt:number};
type Watch=Target&{label:string};
type Status={settings:Settings;collectorAvailable:boolean;recent:Attempt[];targets:Watch[]};
type SafetyApi={read:()=>Promise<Status>;save:(settings:Settings)=>Promise<Status>;catalog?:()=>Promise<ExistingCodexThreadCatalog>;select?:(id:string)=>Promise<Watch>};
export const localSafetyApi:SafetyApi={read:()=>invoke<Status>('local_safety_status'),save:(settings:Settings)=>invoke<Status>('local_safety_save',{settings}),catalog:()=>invoke('codex_watch_candidates'),select:async id=>{
 let rows=await desktopNotificationApi.watches();let row=rows.find(w=>w.threadId===id&&w.enabled);
 if(!row){await desktopNotificationApi.enable(id);rows=await(desktopNotificationApi.refreshWatches?.()??desktopNotificationApi.watches());row=rows.find(w=>w.threadId===id&&w.enabled);}
 if(!row)throw Error('SAFETY_TARGET_UNCONFIRMED');return {threadId:row.threadId,generation:row.generation,cwd:row.cwd,label:row.label};
}};
export function LocalSafetySettings({api=localSafetyApi,monitor}:{api?:typeof localSafetyApi;monitor?:typeof monitorApi}){
 useLanguage();const [status,setStatus]=useState<Status|null>(null),[draft,setDraft]=useState<Settings|null>(null),[watches,setWatches]=useState<Watch[]>([]),[busy,setBusy]=useState(false),[error,setError]=useState(''),[saved,setSaved]=useState(false);
 const [picker,setPicker]=useState(false),[catalog,setCatalog]=useState<ExistingCodexThreadCatalog|null>(null),[loading,setLoading]=useState(false),[query,setQuery]=useState(''),[provider,setProvider]=useState('ALL'),[pickerError,setPickerError]=useState('');
 const [collectorRunning,setCollectorRunning]=useState<boolean|null>(null);
 useEffect(()=>{let active=true;void api.read().then(s=>{if(active){setStatus(s);setDraft(s.settings);setWatches(s.targets);}}).catch(()=>{if(active)setError(t('暂时无法读取保护设置，请重试。'));});return()=>{active=false;};},[api]);
 async function save(){if(!draft||busy)return;setBusy(true);setError('');setSaved(false);try{const value=await api.save(draft);setStatus(value);setDraft(value.settings);setWatches(value.targets);setSaved(true);}catch{setError(t('未能保存，请确认所选对话仍在监听后重试。'));}finally{setBusy(false);}}
 async function refresh(){if(busy)return;setBusy(true);try{const value=await api.read();setStatus(value);setWatches(value.targets);setDraft(old=>old??value.settings);setError('');}catch{setError(t('暂时无法读取保护设置，请重试。'));}finally{setBusy(false);}}
 const ordered=draft?[...(draft.alertTarget?[draft.alertTarget]:[]),...(draft.fallbackTargets??[])]:[];
 function changeOrder(next:Target[]){if(!draft)return;setDraft({...draft,alertTarget:next[0]??null,fallbackTargets:next.slice(1)});setSaved(false);}
 function move(index:number,by:number){const next=[...ordered],to=index+by;if(to<0||to>=next.length)return;[next[index],next[to]]=[next[to],next[index]];changeOrder(next);}
 async function discover(){if(loading)return;setLoading(true);setPickerError('');try{if(api.catalog)setCatalog(await api.catalog());else setCatalog({threads:watches.map(w=>({id:w.threadId,label:w.label,projectProvenance:w.cwd})),complete:true});}catch{setPickerError(t('暂时读不到当前对话，请确认对应应用已打开后刷新。'));}finally{setLoading(false);}}
 async function choose(id:string,label:string){if(busy||ordered.length>=8)return;setBusy(true);setPickerError('');try{const existing=watches.find(w=>w.threadId===id);const selected=existing&&existing.generation<0?existing:api.select?await api.select(id):existing;if(!selected)throw Error('SAFETY_TARGET_UNCONFIRMED');setWatches(old=>[...old.filter(w=>w.threadId!==id),{...selected,label:selected.label||label}]);changeOrder([...ordered,{threadId:selected.threadId,generation:selected.generation,cwd:selected.cwd}]);setPicker(false);setQuery('');}catch{setPickerError(t('加入结果尚未确认，请刷新后核对；未保存新的告警接收设置。'));}finally{setBusy(false);}}
 const providerOf=(id:string)=>id.startsWith('claude_code:')?'Claude Code':'Codex';
 const pool:ExistingCodexThreadCandidate[]=[...(catalog?.threads??[]),...watches.filter(w=>w.generation<0&&!(catalog?.threads??[]).some(c=>c.id===w.threadId)).map(w=>({id:w.threadId,label:w.label,projectProvenance:w.cwd,recencyAt:undefined}))];
 const available=pool.filter(w=>!ordered.some(v=>v.threadId===w.id)&&!w.id.startsWith('pi:')&&(provider==='ALL'||providerOf(w.id)===provider)&&`${w.label} ${w.projectLabel??''} ${w.projectProvenance??''}`.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase())).sort((a,b)=>(b.recencyAt??0)-(a.recencyAt??0)||a.label.localeCompare(b.label));
 const changed=!!draft&&JSON.stringify(draft)!==JSON.stringify(status?.settings);
 return <section className="v4-shared-codex local-safety-settings" aria-label={t('运行保护')}><h2>{t('运行保护')}</h2>{draft&&<>
  <label><input type="checkbox" checked={draft.recoveryEnabled} disabled={busy} onChange={e=>{setDraft({...draft,recoveryEnabled:e.target.checked});setSaved(false);}}/> {t('自动恢复 Codex 模型故障')}</label>
  <p>{t('轻量连接不支持 Goal，这一项在 Windows 上不生效。')}</p>
  <p>{t('仅自动恢复已有原生目标（Goal）的监听对话。目标因明确的临时模型故障阻塞后等待 60 秒，30 分钟内最多恢复 2 次。没有 Goal 的普通对话仍需在 Codex 中重试；手动暂停、额度不足和待决策不会自动恢复。')}</p>
  <ResourceMonitorSettings api={monitor} autoRead onObservation={s=>setCollectorRunning(s.running)}/>
  {collectorRunning!==null&&<p className="local-safety-runtime" data-running={collectorRunning} role="status">{t(collectorRunning?'资源采集正在运行':'资源采集已停止：即使已绑定对话，也不会产生新的告警。请展开上方监测设置恢复。')}</p>}
  <h3>{t('资源告警接收对话')}</h3>
  {!!ordered.length&&<ol className="local-safety-targets">{ordered.map((target,index)=>{const w=watches.find(w=>w.threadId===target.threadId&&w.generation===target.generation&&w.cwd===target.cwd);const listed=!catalog||catalog.threads.some(row=>row.id===target.threadId)||target.generation<0;return <li key={target.threadId} className="local-safety-target"><span><small>{t(index===0?'主接收':'备用 {0}',index)}</small><strong>{catalog?.threads.find(row=>row.id===target.threadId)?.label??w?.label??t('原接收对话已不可用，请重新选择')}</strong>{(!w||!listed)&&<small className="local-safety-unavailable">{t('当前目录中不可用，请更换接收对话')}</small>}<small>{target.threadId.startsWith('claude_code:')?'Claude Code':'Codex'}</small></span><div><button type="button" disabled={busy||index===0} aria-label={t('上移接收对话 {0}',index+1)} onClick={()=>move(index,-1)}><ArrowUp size={18}/></button><button type="button" disabled={busy||index===ordered.length-1} aria-label={t('下移接收对话 {0}',index+1)} onClick={()=>move(index,1)}><ArrowDown size={18}/></button><button type="button" disabled={busy} aria-label={t('移除接收对话 {0}',index+1)} onClick={()=>changeOrder(ordered.filter((_,i)=>i!==index))}><X size={18}/></button></div></li>;})}</ol>}
  <button type="button" disabled={busy||ordered.length>=8} onClick={()=>{setPicker(!picker);if(!picker)void discover();}}><Plus size={17} aria-hidden/>{t(ordered.length>=8?'最多绑定 8 个接收对话':ordered.length?'添加备用对话':'选择接收对话')}</button>
  {picker&&<section className="local-safety-picker" aria-label={t('选择资源告警接收对话')}>
   <div className="local-safety-picker-heading"><strong>{t('选择当前对话')}</strong><button type="button" aria-label={t('关闭选择对话')} onClick={()=>setPicker(false)} disabled={busy}><X size={18}/></button></div>
   <label className="local-safety-search"><Search size={17} aria-hidden/><input type="search" aria-label={t('搜索对话或项目')} placeholder={t('搜索对话或项目')} value={query} onChange={e=>setQuery(e.target.value)}/></label>
   <div className="local-safety-picker-tools"><div role="group" aria-label={t('筛选对话平台')}>{['ALL','Codex','Claude Code'].map(p=><button type="button" key={p} aria-pressed={provider===p} onClick={()=>setProvider(p)}>{p==='ALL'?t('全部'):p}</button>)}</div><button type="button" disabled={loading||busy} onClick={()=>void discover()}><RefreshCw size={15} aria-hidden/>{t('刷新对话')}</button></div>
   <p className="local-safety-picker-note">{t('可选择未监听的当前对话；点击后会开启监听并加入草稿，再保存设置。已归档对话不作为新接收方。')}</p>
   {loading?<p role="status">{t('正在读取当前对话…')}</p>:<ul className="local-safety-candidates">{available.slice(0,100).map(row=><li key={row.id}><button type="button" disabled={busy} aria-label={t('选择 {0}',row.label)} onClick={()=>void choose(row.id,row.label)}><ConversationAvatar id={`w:${row.id}`} kind={row.id.startsWith('claude_code:')?'claude':'codex'}/><span><strong>{row.label}</strong><small>{providerOf(row.id)} · {row.projectLabel??row.projectProvenance?.split(/[\\/]/).filter(Boolean).at(-1)??t('无项目')}</small><small>{t(watches.some(w=>w.threadId===row.id&&w.generation<0)?'已绑定 Bridge':watches.some(w=>w.threadId===row.id)?'已监听':'将开启监听')}{row.recencyAt?` · ${new Date(row.recencyAt*1000).toLocaleDateString()}`:''}</small></span></button></li>)}</ul>}
   {!loading&&!available.length&&!pickerError&&<p>{t('没有找到可选对话。请在 Codex 或 Claude Code 中打开当前对话，再刷新列表。')}</p>}
   {available.length>100&&<p>{t('仅显示前 100 条，请搜索对话名称或项目缩小范围。')}</p>}
   {catalog?.complete===false&&<p>{t('当前目录尚未完整读取，可稍后刷新。')}</p>}{pickerError&&<p role="alert">{pickerError}</p>}
  </section>}
  <p>{t('按顺序优先通知可用对话。主对话离线或确认无法处理时使用备用；送达结果不确定时先核对，不重复通知。可用上下箭头调整顺序。')}</p>
  <p>{t('内存或磁盘紧张时，只向你选择的管理对话发送资源摘要，不清理文件、不结束进程。电脑、Agbrio 和接收会话需要在线；Claude Code 还需加载新版 Agbrio 插件。')}</p>
  {!status?.collectorAvailable&&<p role="status">{t('尚未发现本机资源采集器。接入采集器后才能收到资源告警。')}</p>}
  <button type="button" disabled={busy||!changed} onClick={()=>void save()}>{t(busy?'正在保存…':'保存设置')}</button><button type="button" disabled={busy} onClick={()=>void refresh()}>{t('刷新接收状态')}</button>{saved&&<span role="status">{t('已保存')}</span>}
  {!!status?.recent.length&&<details><summary>{t('最近保护记录')}</summary><button type="button" disabled={busy} onClick={()=>void refresh()}>{t('刷新状态')}</button><ul>{status.recent.slice(0,5).map(r=><li key={r.id}>{t(r.kind==='RECOVERY'?'模型故障恢复':'资源告警')} · {t(({READY:'等待处理',UNKNOWN:'结果待核对，不会重复发送',APPLIED:'已恢复原目标',SENT:'已提交',DELIVERED:'已送达',UNAVAILABLE:'接收方本轮失败，使用备用',REJECTED:'未执行，请查看对话状态',HISTORICAL:'历史告警，未推送',SUPERSEDED:'已由较新告警替代'} as Record<string,string>)[r.status]??'结果待核对，不会重复发送')} · <time>{new Date(r.updatedAt).toLocaleTimeString([],{hour:'2-digit',minute:'2-digit'})}</time></li>)}</ul></details>}
 </>}{error&&<><p role="alert">{error}</p><button type="button" disabled={busy} onClick={()=>void refresh()}>{t('重试')}</button></>}</section>;
}
