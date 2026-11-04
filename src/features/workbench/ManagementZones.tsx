import {useCallback,useEffect,useRef,useState,type FormEvent} from 'react';
import {ArrowRightLeft,Check,ChevronRight,GitBranch,Plus,RefreshCw,Search,UserRoundCog,X} from 'lucide-react';
import {ConversationAvatar} from './ConversationHome';
import type {CodexWatch,NotificationApi} from './CodexNotifications';
import {managementZoneApi,type ManagementApi,type ManagementChange,type ManagementSnapshot,type ManagementZone,type ManagedConversation} from './managementZoneApi';
import {t} from '../../i18n';
import {readModels} from './readModelCache';
import '../../styles/management-zones.css';

type Editor={kind:ManagementChange['action'];zone?:ManagementZone;source?:ManagedConversation;adopt?:{managerId:string;threadIds:string[]}};
const pendingKey=()=>`agbrio.management.pending:${readModels.presentationScope()??location.origin}`;
function savedPending():ManagementChange|null{try{const value=JSON.parse(sessionStorage.getItem(pendingKey())??'null');return value&&typeof value.requestId==='string'&&/^[0-9a-f-]{36}$/.test(value.requestId)&&['CREATE','ADOPT','APPOINT','ADD','MOVE','REMOVE','RENAME','DELETE'].includes(value.action)?value:null;}catch{return null;}}
function savePending(input:ManagementChange|null){try{if(input)sessionStorage.setItem(pendingKey(),JSON.stringify(input));else sessionStorage.removeItem(pendingKey());}catch{/* Memory intent and server idempotency remain available. */}}
const rejected=new Set(['MANAGEMENT_CHANGED_REFRESH','MANAGEMENT_SOURCE_CHANGED_REFRESH','MANAGEMENT_ALREADY_ASSIGNED_USE_TRANSFER','MANAGEMENT_ALREADY_ASSIGNED','MANAGEMENT_MANAGER_IS_EXECUTOR','MANAGEMENT_EXECUTOR_IS_MANAGER','MANAGEMENT_ENABLE_WATCH_FIRST','MANAGEMENT_ZONE_NOT_EMPTY','MANAGEMENT_CONVERSATION_UNAVAILABLE','MANAGEMENT_ADOPTION_CHANGED_REFRESH','MANAGEMENT_REQUEST_INVALID','MANAGEMENT_REQUEST_CONFLICT','MANAGEMENT_NAME_INVALID','MANAGEMENT_MANAGER_UNSUPPORTED','MANAGEMENT_ZONE_NOT_FOUND','MANAGEMENT_ACTION_INVALID','MANAGEMENT_MEMBERSHIP_NOT_FOUND','MANAGEMENT_ZONE_LIMIT','MANAGEMENT_MANAGER_CAPACITY','MANAGEMENT_ADOPTION_INVALID']);
const provider=(row:ManagedConversation)=>row.provider==='CLAUDE_CODE'?'Claude Code':row.provider==='CODEX'?'Codex':'Pi';
const stateLabel=(row:ManagedConversation)=>!row.available?t('暂时无法读取'):['RUNNING','THINKING'].includes(row.state)?t('执行中'):['ACTION_REQUIRED','WAITING_DECISION'].includes(row.state)?t('等你确认'):['FAILED','INTERRUPTED','CANCELLED'].includes(row.state)?t('需要处理'):['RESULT_READY','COMPLETE'].includes(row.state)?t('本轮已结束'):t('空闲');
const errorLabel=(error:unknown)=>{
 const code=String(error).match(/\bMANAGEMENT_[A-Z_]+\b/)?.[0]??'';
 const messages:Record<string,string>={
  MANAGEMENT_CHANGED_REFRESH:'分工已发生变化，已为你重新读取，请核对后再操作。',
  MANAGEMENT_SOURCE_CHANGED_REFRESH:'这条对话的状态已变化，请重新选择。',
  MANAGEMENT_ALREADY_ASSIGNED_USE_TRANSFER:'这条执行对话已有管理区，请使用“转移”。',
  MANAGEMENT_MANAGER_IS_EXECUTOR:'这条对话目前是执行对话，请先从原管理区移出，再任命为管理。',
  MANAGEMENT_EXECUTOR_IS_MANAGER:'这条对话已经是管理对话，不能同时作为执行对话。',
  MANAGEMENT_ENABLE_WATCH_FIRST:'请先在对话列表监听这条对话，再加入管理区。',
  MANAGEMENT_ZONE_NOT_EMPTY:'请先转移或移出执行对话，再删除这个管理区。',
  MANAGEMENT_CONVERSATION_UNAVAILABLE:'暂时找不到这条原对话，请打开原对话后重试。',
  MANAGEMENT_ADOPTION_CHANGED_REFRESH:'已有订阅发生变化，请刷新后重新接管。',
 };
 return t(messages[code]??'操作结果暂未确认。请检查并完成这次操作，系统会沿用同一请求，不会重复创建。');
};
function Avatar({row}:{row:ManagedConversation}){return <ConversationAvatar id={row.threadId} kind={row.provider==='PI'?'pi':row.provider==='CLAUDE_CODE'?'claude':'codex'}/>;}
function Node({row,role,onOpen}:{row:ManagedConversation;role:string;onOpen:()=>void}){
 return <button type="button" className="management-node" onClick={onOpen}><Avatar row={row}/><span className="management-node-copy"><small>{role} · {provider(row)}</small><strong>{row.label}</strong><span data-running={['RUNNING','THINKING'].includes(row.state)}>{stateLabel(row)}</span></span><ChevronRight size={16} aria-hidden/></button>;
}
export function ManagementZones({api=managementZoneApi,watchApi,onOpenWatch}:{api?:ManagementApi;watchApi?:NotificationApi;onOpenWatch:(watch:CodexWatch)=>void}){
 const [data,setData]=useState<ManagementSnapshot|null>(null),[selected,setSelected]=useState(''),[error,setError]=useState(''),[busy,setBusy]=useState(false),[loading,setLoading]=useState(true);
 const [editor,setEditor]=useState<Editor|null>(null),[name,setName]=useState(''),[choice,setChoice]=useState(''),[query,setQuery]=useState('');
 const [pending,setPending]=useState<ManagementChange|null>(savedPending),[notice,setNotice]=useState('');
 const epoch=useRef(0),alive=useRef(true);
 const read=useCallback(async()=>{const version=++epoch.current;try{const view=await api.read();if(alive.current&&version===epoch.current){setData(view);setSelected(old=>view.zones.some(z=>z.id===old)?old:view.zones[0]?.id??'');}}finally{if(alive.current&&version===epoch.current)setLoading(false);}},[api]);
 useEffect(()=>{alive.current=true;if(savedPending())setError(errorLabel('UNCONFIRMED'));void read().catch(()=>setError(t('暂时无法读取管理区，请重试。')));const refresh=()=>{if(!document.hidden)void read().catch(()=>{});};const timer=setInterval(refresh,5000);window.addEventListener('focus',refresh);return()=>{alive.current=false;epoch.current++;clearInterval(timer);window.removeEventListener('focus',refresh);};},[read]);
 const zone=data?.zones.find(z=>z.id===selected);
 const membership=new Map(data?.zones.flatMap(z=>z.members.map(m=>[m.threadId,z] as const))??[]);
 const managers=new Set(data?.zones.flatMap(z=>z.managerId?[z.managerId]:[])??[]);
 function edit(next:Editor){setEditor(next);setName(next.zone?.name??'');setChoice(next.adopt?.managerId??'');setQuery('');setError('');setNotice('');}
 async function apply(input:ManagementChange){
  setBusy(true);setError('');setPending(input);savePending(input);
  try{await api.change(input);setPending(null);savePending(null);setEditor(null);setNotice(t('分工已更新，通知将按新的归属交接。'));await read().catch(()=>setError(t('分工已保存，暂时无法刷新列表，请重试。')));}
  catch(e){setError(errorLabel(e));if(rejected.has(String(e).match(/\bMANAGEMENT_[A-Z_]+\b/)?.[0]??'')){setPending(null);savePending(null);setEditor(null);await read().catch(()=>{});}}
  finally{setBusy(false);}
 }
 async function submit(event:FormEvent){
  event.preventDefault();if(!editor||busy||pending)return;
  const input:ManagementChange={requestId:crypto.randomUUID(),action:editor.kind};
  if(editor.zone){input.zoneId=editor.zone.id;input.revision=editor.zone.revision;}
  if(['CREATE','RENAME','ADOPT'].includes(editor.kind))input.name=name.trim();
  if(['CREATE','APPOINT'].includes(editor.kind))input.managerId=choice;
  if(editor.kind==='ADOPT'){input.managerId=editor.adopt!.managerId;input.threadIds=editor.adopt!.threadIds;}
  if(editor.kind==='ADD'){
   const row=data?.candidates.find(c=>c.threadId===choice);if(!row)return;
   if(!row.watchEnabled&&watchApi){setBusy(true);try{await watchApi.enable(row.threadId);await read();const watches=await watchApi.watches();const actual=watches.find(w=>w.threadId===row.threadId);if(!actual)throw Error('MANAGEMENT_ENABLE_WATCH_FIRST');input.generation=actual.generation;}catch(e){setBusy(false);setError(errorLabel(e));return;}}
   input.threadId=row.threadId;input.generation??=row.generation;
  }
  if(editor.source){input.threadId=editor.source.threadId;input.generation=editor.source.generation;}
  if(editor.kind==='MOVE'){
   const target=data?.zones.find(z=>z.id===choice);if(!target||!editor.zone)return;
   input.zoneId=target.id;input.revision=target.revision;input.fromZoneId=editor.zone.id;input.fromRevision=editor.zone.revision;
  }
  await apply(input);
 }
 async function open(row:ManagedConversation){
  if(!watchApi)return;setError('');
  try{let watches=await watchApi.watches();let watch=watches.find(w=>w.threadId===row.threadId);if(!watch){await watchApi.enable(row.threadId);watches=await watchApi.watches();watch=watches.find(w=>w.threadId===row.threadId);}if(watch)onOpenWatch(watch);else throw Error('MANAGEMENT_CONVERSATION_UNAVAILABLE');}
  catch(e){setError(errorLabel(e));}
 }
 const pickingManager=editor?.kind==='CREATE'||editor?.kind==='APPOINT';
 const candidates=data?.candidates.filter(row=>(pickingManager?row.provider!=='PI'&&!membership.has(row.threadId):!membership.has(row.threadId)&&!managers.has(row.threadId))&&`${row.label} ${provider(row)} ${row.cwd}`.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()))??[];
 const title=editor?.kind==='CREATE'?t('新建管理区'):editor?.kind==='APPOINT'?t('更换管理对话'):editor?.kind==='MOVE'?t('转移执行对话'):editor?.kind==='ADD'?t('添加执行对话'):editor?.kind==='ADOPT'?t('接管现有分工'):editor?.kind==='REMOVE'?t('移出管理区'):editor?.kind==='DELETE'?t('删除管理区'):t('重命名管理区');
 const valid=editor&&(['CREATE','APPOINT','ADD','MOVE'].includes(editor.kind)?Boolean(choice):true)&&(!['CREATE','ADOPT','RENAME'].includes(editor.kind)||Boolean(name.trim()));
 const legacy=new Map<string,string[]>();for(const link of data?.legacyAssignments??[]){if(!membership.has(link.threadId))legacy.set(link.managerId,[...new Set([...(legacy.get(link.managerId)??[]),link.threadId])]);}
 return <section className="management-page" aria-label={t('管理区拓扑')}>
  <div className="management-heading"><p>{t('按业务线分工。每条执行对话只归属一个管理区，换负责人和转移都在这里完成。')}</p><button type="button" className="management-primary" disabled={busy||Boolean(pending)} onClick={()=>edit({kind:'CREATE'})}><Plus size={16} aria-hidden/>{t('新建管理区')}</button><button type="button" className="management-icon" aria-label={t('刷新管理区')} disabled={busy} onClick={()=>void read().catch(()=>setError(t('暂时无法读取管理区，请重试。')))}><RefreshCw size={17} aria-hidden/></button></div>
  {loading&&<p role="status">{t('正在读取分工…')}</p>}
  {error&&<div className="management-error" role="alert"><p>{error}</p>{pending&&<button type="button" disabled={busy} onClick={()=>void apply(pending)}>{t('检查并完成这次操作')}</button>}</div>}
  {notice&&<p className="management-notice" role="status"><Check size={16} aria-hidden/>{notice}</p>}
  {data&&<div className="management-tabs" role="group" aria-label={t('选择管理区')}>{data.zones.map(z=><button type="button" key={z.id} aria-pressed={z.id===selected} onClick={()=>{if(!pending){setSelected(z.id);setEditor(null);}}}>{z.name}<span>{z.members.length}</span></button>)}</div>}
  {editor&&<form className="management-editor" onSubmit={e=>void submit(e)} aria-label={title}><div className="management-editor-title"><h2>{title}</h2><button type="button" className="management-icon" disabled={busy||Boolean(pending)} aria-label={t('关闭')} onClick={()=>setEditor(null)}><X size={18}/></button></div>
   {['CREATE','RENAME','ADOPT'].includes(editor.kind)&&<label className="management-field">{t('管理区名称')}<input autoFocus maxLength={80} value={name} disabled={busy||Boolean(pending)} onChange={e=>setName(e.target.value)} placeholder={t('例如：游戏开发、美术制作')}/></label>}
   {(pickingManager||editor.kind==='ADD')&&<><label className="management-search"><Search size={17} aria-hidden/><input type="search" value={query} disabled={busy||Boolean(pending)} onChange={e=>setQuery(e.target.value)} placeholder={t('搜索对话或项目')}/></label><div className="management-picker" role="radiogroup" aria-label={pickingManager?t('选择管理对话'):t('选择执行对话')}>{candidates.map(row=><button key={row.threadId} type="button" role="radio" aria-checked={choice===row.threadId} disabled={busy||Boolean(pending)} onClick={()=>setChoice(row.threadId)}><Avatar row={row}/><span><strong>{row.label}</strong><small>{provider(row)} · {row.cwd.split(/[\\/]/).filter(Boolean).at(-1)}</small></span>{choice===row.threadId&&<Check size={18} aria-hidden/>}</button>)}{!candidates.length&&<p>{t('没有可选对话。请先在“对话”中添加监听，再回到这里。')}</p>}</div></>}
   {editor.kind==='MOVE'&&<><p>{t('当前任务继续运行。后续提问和结果将交给目标管理区，不需要手动重新订阅。')}</p><label className="management-field">{t('转移到')}<select value={choice} disabled={busy||Boolean(pending)} onChange={e=>setChoice(e.target.value)}><option value="">{t('选择目标管理区')}</option>{data?.zones.filter(z=>z.id!==editor.zone?.id).map(z=><option key={z.id} value={z.id}>{z.name}</option>)}</select></label></>}
   {editor.kind==='APPOINT'&&<p>{t('原任务和权限保留。尚未发送的通知交给新管理对话；送达未确认的通知先核对，不会重复发送。')}</p>}
   {editor.kind==='ADOPT'&&<p>{t('将现有订阅整理为一个管理区，执行对话保留原身份和进度，不会重发任务。')} {editor.adopt?.threadIds.length} {t('条执行对话')}</p>}
   {editor.kind==='REMOVE'&&<p>{t('只移除管理归属，不停止任务、不删除原对话。')}</p>}
   {editor.kind==='DELETE'&&<p>{t('只删除空管理区，原对话不会删除。')}</p>}
   <footer><button type="submit" className="management-primary" disabled={!valid||busy||Boolean(pending)}>{busy?t('正在处理…'):editor.kind==='CREATE'?t('创建管理区'):editor.kind==='APPOINT'?t('确认更换'):editor.kind==='MOVE'?t('确认转移'):t('确认')}</button><button type="button" disabled={busy||Boolean(pending)} onClick={()=>setEditor(null)}>{t('取消')}</button></footer>
  </form>}
  {zone&&!editor&&<><div className="management-zone-heading"><h2>{zone.name}</h2><button type="button" disabled={Boolean(pending)} onClick={()=>edit({kind:'APPOINT',zone})}><UserRoundCog size={16} aria-hidden/>{zone.manager?t('更换管理对话'):t('选择管理对话')}</button><details className="management-more"><summary>{t('更多')}</summary><button type="button" onClick={()=>edit({kind:'RENAME',zone})}>{t('重命名')}</button><button type="button" disabled={zone.members.length>0} onClick={()=>edit({kind:'DELETE',zone})}>{t('删除空管理区')}</button></details></div>
   <div className="management-tree"><div className="management-root">{zone.manager?<Node row={zone.manager} role={t('管理对话')} onOpen={()=>void open(zone.manager!)}/>:<p>{t('还没有管理对话，先选择负责人。')}</p>}</div>
    <ol className="management-children">{zone.members.map(member=><li key={member.threadId}>{member.conversation?<><Node row={member.conversation} role={t('执行对话')} onOpen={()=>void open(member.conversation!)}/><div className="management-child-actions"><button type="button" onClick={()=>edit({kind:'MOVE',zone,source:member.conversation!})}><ArrowRightLeft size={14} aria-hidden/>{t('转移')}</button><button type="button" onClick={()=>edit({kind:'REMOVE',zone,source:member.conversation!})}>{t('移出')}</button></div></>:<p>{t('原对话暂时不可用')}<code>{member.threadId}</code></p>}</li>)}</ol>
    {!zone.members.length&&<div className="management-empty"><GitBranch size={30} aria-hidden/><p>{t('把执行对话加入这里，提问与结果交给本区管理对话。')}</p></div>}
   </div><button type="button" className="management-add" onClick={()=>edit({kind:'ADD',zone})}><Plus size={17} aria-hidden/>{t('添加执行对话')}</button>
   <p className="management-footnote">{t('分工不改变原对话权限。管理对话仍须能使用 Agbrio MCP；通知插件与 MCP 工具是两条独立连接。')}</p></>}
  {!loading&&data&&!zone&&!editor&&<div className="management-empty"><GitBranch size={36} aria-hidden/><h2>{t('先建立你的管理区')}</h2><p>{t('选择管理对话，再添加它负责的执行对话。你可以为不同业务建立多个管理区。')}</p></div>}
  {legacy.size>0&&!editor&&<section className="management-legacy"><h2>{t('已有分工可以直接接管')}</h2><p>{t('这些对话已经登记通知，不用逐条重新添加。')}</p>{[...legacy].map(([managerId,threadIds])=>{const row=data?.candidates.find(c=>c.threadId===managerId);return <button type="button" key={managerId} disabled={!row||Boolean(pending)} onClick={()=>{edit({kind:'ADOPT',adopt:{managerId,threadIds}});setName(row?.label??t('管理区'));}}><span>{row?.label??managerId}<small>{threadIds.length} {t('条执行对话')}</small></span><ChevronRight size={17} aria-hidden/></button>;})}</section>}
 </section>;
}
