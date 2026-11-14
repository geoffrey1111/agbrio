import {useCallback,useEffect,useMemo,useRef,useState,type CSSProperties,type FormEvent,type ReactNode} from 'react';
import * as Dialog from '@radix-ui/react-dialog';
import {ArrowRightLeft,Check,ChevronDown,ExternalLink,GitBranch,LayoutList,MoreHorizontal,Orbit,Plus,RefreshCw,Search,UserRoundCog,X} from 'lucide-react';
import {ConversationAvatar} from './ConversationHome';
import type {CodexWatch,NotificationApi} from './CodexNotifications';
import {managementZoneApi,type ManagementApi,type ManagementChange,type ManagementSnapshot,type ManagementZone,type ManagedConversation} from './managementZoneApi';
import {t} from '../../i18n';
import {useBackLayer} from './navigationHistory';
import {readModels} from './readModelCache';
import '../../styles/management-zones.css';

type Editor={kind:ManagementChange['action'];zone?:ManagementZone;source?:ManagedConversation;adopt?:{managerId:string;threadIds:string[]}};
type Focus={zoneId:string;threadId:string;role:'MANAGER'|'MEMBER'};
type Tier='ATTENTION'|'RUNNING'|'QUIET';
export type ZoneView='NORMAL'|'MAP';
const VIEW_KEY='agbrio.management.view';
/** A per-device display preference only. */
function initialView():ZoneView{try{const saved=localStorage.getItem(VIEW_KEY);if(saved==='NORMAL'||saved==='MAP')return saved;}catch{/* Presentation default below. */}return typeof window!=='undefined'&&typeof window.matchMedia==='function'&&window.matchMedia('(max-width: 650px)').matches?'NORMAL':'MAP';}
const pendingKey=()=>`agbrio.management.pending:${readModels.presentationScope()??location.origin}`;
function savedPending():ManagementChange|null{try{const value=JSON.parse(sessionStorage.getItem(pendingKey())??'null');return value&&typeof value.requestId==='string'&&/^[0-9a-f-]{36}$/.test(value.requestId)&&['CREATE','ADOPT','APPOINT','ADD','MOVE','REMOVE','RENAME','DELETE'].includes(value.action)?value:null;}catch{return null;}}
function savePending(input:ManagementChange|null){try{if(input)sessionStorage.setItem(pendingKey(),JSON.stringify(input));else sessionStorage.removeItem(pendingKey());}catch{/* Memory intent and server idempotency remain available. */}}
const rejected=new Set(['MANAGEMENT_CHANGED_REFRESH','MANAGEMENT_SOURCE_CHANGED_REFRESH','MANAGEMENT_ALREADY_ASSIGNED_USE_TRANSFER','MANAGEMENT_ALREADY_ASSIGNED','MANAGEMENT_MANAGER_IS_EXECUTOR','MANAGEMENT_EXECUTOR_IS_MANAGER','MANAGEMENT_ENABLE_WATCH_FIRST','MANAGEMENT_ZONE_NOT_EMPTY','MANAGEMENT_CONVERSATION_UNAVAILABLE','MANAGEMENT_ADOPTION_CHANGED_REFRESH','MANAGEMENT_REQUEST_INVALID','MANAGEMENT_REQUEST_CONFLICT','MANAGEMENT_NAME_INVALID','MANAGEMENT_MANAGER_UNSUPPORTED','MANAGEMENT_ZONE_NOT_FOUND','MANAGEMENT_ACTION_INVALID','MANAGEMENT_MEMBERSHIP_NOT_FOUND','MANAGEMENT_ZONE_LIMIT','MANAGEMENT_MANAGER_CAPACITY','MANAGEMENT_ADOPTION_INVALID']);
const provider=(row:ManagedConversation)=>row.provider==='CLAUDE_CODE'?'Claude Code':row.provider==='CODEX'?'Codex':'Pi';
const folder=(path:string)=>path.split(/[\\/]/).filter(Boolean).at(-1)??'';
const INACTIVE_AFTER_MS=3*24*60*60*1000;
const SETTLED=['IDLE','RESULT_READY','COMPLETE','EMPTY','SESSION_END','TURN_ENDED','DESKTOP'];
function isInactive(row:ManagedConversation|null,now:number){
 const at=row?.lastActivityAt;
 return Boolean(row&&SETTLED.includes(row.state)&&typeof at==='number'&&Number.isFinite(at)&&at>0&&now-at>=INACTIVE_AFTER_MS);
}
const ATTENTION=['ACTION_REQUIRED','WAITING_DECISION','FAILED','INTERRUPTED','CANCELLED'];
/** Presentation emphasis only; never routing or permission authority. */
export const tierOf=(row:ManagedConversation):Tier=>!row.available?'QUIET':ATTENTION.includes(row.state)?'ATTENTION':['RUNNING','THINKING'].includes(row.state)?'RUNNING':'QUIET';
const stateLabel=(row:ManagedConversation)=>!row.available?t('暂时无法读取'):['RUNNING','THINKING'].includes(row.state)?t('执行中'):['ACTION_REQUIRED','WAITING_DECISION'].includes(row.state)?t('等你确认'):['FAILED','INTERRUPTED','CANCELLED'].includes(row.state)?t('需要处理'):['RESULT_READY','COMPLETE'].includes(row.state)?t('本轮已结束'):t('空闲');
const errorLabel=(error:unknown,uncertain=true)=>{
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
 return t(messages[code]??(uncertain?'操作结果暂未确认。请检查并完成这次操作，系统会沿用同一请求，不会重复创建。':'操作未完成，请刷新后重试。'));
};
function useNarrow(){
 const query='(max-width: 650px)';
 const read=()=>typeof window!=='undefined'&&typeof window.matchMedia==='function'&&window.matchMedia(query).matches;
 const [narrow,setNarrow]=useState(read);
 useEffect(()=>{if(typeof window.matchMedia!=='function')return;const media=window.matchMedia(query);const update=()=>setNarrow(media.matches);update();media.addEventListener?.('change',update);return()=>media.removeEventListener?.('change',update);},[]);
 return narrow;
}
function Avatar({row}:{row:ManagedConversation}){return <ConversationAvatar id={row.threadId} kind={row.provider==='PI'?'pi':row.provider==='CLAUDE_CODE'?'claude':'codex'}/>;}
function Status({row}:{row:ManagedConversation}){return <span className="mz-status" data-tier={tierOf(row)}><i aria-hidden/>{stateLabel(row)}</span>;}

/** Members ordered by emphasis, then placed on concentric ellipses (positions in %). */
export function constellation(members:ManagedConversation[],compact=false){
 const rank={ATTENTION:0,RUNNING:1,QUIET:2} as const;
 const sorted=[...members].sort((a,b)=>rank[tierOf(a)]-rank[tierOf(b)]);
 const n=sorted.length,rings=n<=7?1:n<=20?2:3;
 const radii=compact?(rings===1?[[36,36]]:rings===2?[[27,24],[40,44]]:[[22,17],[32,31],[41,45]]):rings===1?[[36,35]]:rings===2?[[25,28],[45,44]]:[[19,20],[33,32],[46,45]];
 const weight=radii.reduce((sum,[x])=>sum+x,0);
 const counts=radii.map(([x])=>Math.round(n*x/weight));
 if(rings>1)counts[0]=Math.max(Math.min(n,5),counts[0]);
 counts[rings-1]+=n-counts.reduce((a,b)=>a+b,0);
 const placed:{row:ManagedConversation;x:number;y:number;ring:number}[]=[];let index=0;
 counts.forEach((count,ring)=>{const [rx,ry]=radii[ring];const offset=-Math.PI/2+(ring%2?Math.PI/Math.max(count,1):Math.PI/Math.max(count,1)/2);for(let i=0;i<count&&index<n;i++,index++){const angle=offset+i*2*Math.PI/count;placed.push({row:sorted[index],x:50+rx*Math.cos(angle),y:50+ry*Math.sin(angle),ring});}});
 return {placed,rings,orbits:radii.filter((_,ring)=>counts[ring]>0)};
}

function Graph({zone,view,narrow,focus,onFocus,onAdd,disabled}:{zone:ManagementZone;view:ZoneView;narrow:boolean;focus:Focus|null;onFocus:(f:Focus)=>void;onAdd:()=>void;disabled:boolean}){
 const members=zone.members.map(m=>m.conversation).filter((row):row is ManagedConversation=>Boolean(row));
 const missing=zone.members.filter(m=>!m.conversation);
 const managerButton=(compact=false)=>zone.manager?<button type="button" className="mz-manager" data-compact={compact||undefined} aria-pressed={focus?.threadId===zone.manager.threadId} title={zone.manager.label} onClick={()=>onFocus({zoneId:zone.id,threadId:zone.manager!.threadId,role:'MANAGER'})}><Avatar row={zone.manager}/><span><small>{t('管理对话')} · {provider(zone.manager)}</small><strong>{zone.manager.label}</strong><Status row={zone.manager}/></span></button>:<div className="mz-manager mz-manager-empty"><p>{t('还没有管理对话')}</p></div>;
 const memberButton=(row:ManagedConversation,style?:CSSProperties)=><button key={row.threadId} type="button" className="mz-member" data-tier={tierOf(row)} style={style} aria-pressed={focus?.threadId===row.threadId} title={`${row.label} · ${provider(row)}`} aria-label={`${t('执行对话')} · ${provider(row)} · ${row.label} · ${stateLabel(row)}`} onClick={()=>onFocus({zoneId:zone.id,threadId:row.threadId,role:'MEMBER'})}><Avatar row={row}/><strong>{row.label}</strong><Status row={row}/></button>;
 const missingNote=missing.length>0&&<p className="mz-missing">{t('{0} 条原对话暂时不可用',missing.length)}</p>;
 if(view==='NORMAL'&&(narrow||members.length>4)){
  const groups:[Tier,string][]=[['ATTENTION','需要你'],['RUNNING','执行中'],['QUIET','其他']];
  return <div className="mz-list">{managerButton(true)}
   {groups.map(([tier,label])=>{const rows=members.filter(r=>tierOf(r)===tier);return rows.length?<section key={tier} className="mz-group" data-tier={tier}><h3>{t(label)}<span>{rows.length}</span></h3><ul>{rows.map(row=><li key={row.threadId}>{memberButton(row)}</li>)}</ul></section>:null;})}
   {missingNote}
   <button type="button" className="mz-add-row" disabled={disabled} onClick={onAdd}><Plus size={17} aria-hidden/>{t('添加执行对话')}</button>
  </div>;
 }
 if(view==='NORMAL')return <div className="mz-tree" style={{'--mz-cols':Math.max(1,members.length+1)} as CSSProperties}>
  <div className="mz-tree-root">{managerButton()}</div>
  <ol className="mz-tree-children">{members.map(row=><li key={row.threadId}>{memberButton(row)}</li>)}<li className="mz-tree-add"><button type="button" className="mz-add-node" disabled={disabled} onClick={onAdd}><Plus size={20} aria-hidden/><span>{t('添加执行对话')}</span></button></li></ol>
  {missingNote}
 </div>;
 const {placed,rings,orbits}=constellation(members,narrow);
 return <div className="mz-map" data-rings={rings}>
  <svg className="mz-map-lines" viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden>{orbits.map(([rx,ry])=><ellipse key={rx} cx="50" cy="50" rx={rx} ry={ry} className="mz-orbit"/>)}{placed.map(p=><line key={p.row.threadId} x1="50" y1="50" x2={p.x} y2={p.y} data-tier={tierOf(p.row)}/>)}</svg>
  <div className="mz-map-center">{managerButton(true)}</div>
  <ol className="mz-map-nodes">{placed.map(p=><li key={p.row.threadId} style={{left:`${p.x}%`,top:`${p.y}%`}}>{memberButton(p.row)}</li>)}</ol>
  <button type="button" className="mz-map-add" disabled={disabled} onClick={onAdd}><Plus size={17} aria-hidden/>{t('添加执行对话')}</button>
  {missingNote}
 </div>;
}

function Menu({label,children}:{label:string;children:(close:()=>void)=>ReactNode}){
 const [open,setOpen]=useState(false),root=useRef<HTMLDivElement>(null),trigger=useRef<HTMLButtonElement>(null),popup=useRef<HTMLDivElement>(null),startAtEnd=useRef(false);
 const items=()=>Array.from(popup.current?.querySelectorAll<HTMLButtonElement>('button[role=menuitem]:not(:disabled)')??[]);
 const close=()=>{setOpen(false);trigger.current?.focus({preventScroll:true});};
 useEffect(()=>{if(!open)return;const options=items();(startAtEnd.current?options.at(-1):options[0])?.focus();startAtEnd.current=false;
  const away=(e:PointerEvent)=>{if(!root.current?.contains(e.target as Node))setOpen(false);};
  document.addEventListener('pointerdown',away);return()=>document.removeEventListener('pointerdown',away);
 },[open]);
 return <div className="mz-menu" ref={root} onBlur={event=>{if(!event.currentTarget.contains(event.relatedTarget))setOpen(false);}} onKeyDown={event=>{
  if(!open&&['ArrowDown','ArrowUp'].includes(event.key)){event.preventDefault();startAtEnd.current=event.key==='ArrowUp';setOpen(true);return;}
  if(!open)return;
  if(event.key==='Escape'){event.preventDefault();event.stopPropagation();close();return;}
  if(event.key==='Tab'){close();return;}
  if(!['ArrowDown','ArrowUp','Home','End'].includes(event.key))return;
  event.preventDefault();const options=items();if(!options.length)return;
  const index=options.indexOf(document.activeElement as HTMLButtonElement);
  const next=event.key==='Home'?0:event.key==='End'?options.length-1:index<0?(event.key==='ArrowUp'?options.length-1:0):(index+options.length+(event.key==='ArrowDown'?1:-1))%options.length;
  options[next]?.focus();
 }}><button ref={trigger} type="button" className="management-icon" aria-label={label} aria-expanded={open} aria-haspopup="menu" onClick={()=>setOpen(v=>!v)}><MoreHorizontal size={19} aria-hidden/></button>{open&&<div ref={popup} className="mz-menu-pop" role="menu" aria-label={label}>{children(close)}</div>}</div>;
}

export function ManagementZones({api=managementZoneApi,watchApi,onOpenWatch}:{api?:ManagementApi;watchApi?:NotificationApi;onOpenWatch:(watch:CodexWatch)=>void}){
 const [data,setData]=useState<ManagementSnapshot|null>(null),[selected,setSelected]=useState(''),[error,setError]=useState(''),[busy,setBusy]=useState(false),[loading,setLoading]=useState(true);
 const [editor,setEditor]=useState<Editor|null>(null),[name,setName]=useState(''),[nameTouched,setNameTouched]=useState(false),[choice,setChoice]=useState(''),[query,setQuery]=useState('');
 const [pending,setPending]=useState<ManagementChange|null>(savedPending),[notice,setNotice]=useState(''),[focus,setFocus]=useState<Focus|null>(null);
 const [refreshing,setRefreshing]=useState(false),[opening,setOpening]=useState(''),[openError,setOpenError]=useState('');
 const openLock=useRef(false),changeLock=useRef(false),refreshLock=useRef(false);
 const [revealedZone,setRevealedZone]=useState<string|null>(null); const epoch=useRef(0),alive=useRef(true),narrow=useNarrow(),[view,setViewState]=useState<ZoneView>(initialView);
 const setView=(next:ZoneView)=>{setViewState(next);try{localStorage.setItem(VIEW_KEY,next);}catch{/* Keeps working for this page. */}};
 const read=useCallback(async(preferred?:string)=>{const version=++epoch.current;try{const view=await api.read();if(alive.current&&version===epoch.current){setData(view);setSelected(old=>view.zones.some(z=>z.id===preferred)?preferred!:view.zones.some(z=>z.id===old)?old:view.zones[0]?.id??'');}}finally{if(alive.current&&version===epoch.current)setLoading(false);}},[api]);
 useEffect(()=>{alive.current=true;if(savedPending())setError(errorLabel('UNCONFIRMED'));void read().catch(()=>setError(t('暂时无法读取管理区，请重试。')));const refresh=()=>{if(!document.hidden&&!refreshLock.current&&!changeLock.current)void read().catch(()=>{});};const timer=setInterval(refresh,5000);window.addEventListener('focus',refresh);document.addEventListener('visibilitychange',refresh);return()=>{alive.current=false;epoch.current++;clearInterval(timer);window.removeEventListener('focus',refresh);document.removeEventListener('visibilitychange',refresh);};},[read]);
 useEffect(()=>{if(!notice)return;const timer=setTimeout(()=>setNotice(''),4000);return()=>clearTimeout(timer);},[notice]);
 const zone=data?.zones.find(z=>z.id===selected);
 const now=Date.now();
 const activeMembers=zone?.members.filter(member=>!isInactive(member.conversation,now));
 const hiddenCount=(zone?.members.length??0)-(activeMembers?.length??0);
 const showInactive=Boolean(zone&&revealedZone===zone.id);
 const graphZone=zone&&activeMembers&&!showInactive?{...zone,members:activeMembers}:zone;
 const membership=useMemo(()=>new Map(data?.zones.flatMap(z=>z.members.map(m=>[m.threadId,z] as const))??[]),[data]);
 const managers=useMemo(()=>new Set(data?.zones.flatMap(z=>z.managerId?[z.managerId]:[])??[]),[data]);
 const focusZone=focus?data?.zones.find(z=>z.id===focus.zoneId):undefined;
 const focused=focus&&focusZone?(focus.role==='MANAGER'?focusZone.manager:focusZone.members.find(m=>m.threadId===focus.threadId)?.conversation)??null:null;
 const zonesBar=useRef<HTMLDivElement>(null);
 const inspectorOpener=useRef<HTMLElement|null>(null),editorOpener=useRef<HTMLElement|null>(null),inspectorClose=useRef<HTMLButtonElement>(null);
 const closeInspector=()=>{setFocus(null);setOpenError('');inspectorOpener.current?.focus({preventScroll:true});};
 const focusedId=focused?.threadId;
 useEffect(()=>{if(narrow&&focusedId)inspectorClose.current?.focus({preventScroll:true});},[narrow,focusedId]);
 useEffect(()=>{if(focus&&(!focused||focus.zoneId!==selected))setFocus(null);},[focus,focused,selected]);
 const locked=busy||Boolean(pending);
 useBackLayer(Boolean(focused&&narrow),closeInspector);
 useBackLayer(Boolean(editor),()=>{if(!locked)setEditor(null);});
 function edit(next:Editor){editorOpener.current=document.activeElement instanceof HTMLElement?document.activeElement:null;setEditor(next);setName(next.zone?.name??'');setNameTouched(Boolean(next.zone));setChoice(next.adopt?.managerId??'');setQuery('');setError('');setNotice('');if(next.kind==='APPOINT'||next.kind==='CREATE')void read().catch(()=>{});}
 async function apply(input:ManagementChange){
  if(changeLock.current)return;
  changeLock.current=true;setBusy(true);setError('');setPending(input);savePending(input);
  try{
   const receipt=await api.change(input);
   const target=typeof receipt==='object'&&receipt&&'zoneId' in receipt&&typeof receipt.zoneId==='string'?receipt.zoneId:input.zoneId;
   setPending(null);savePending(null);setEditor(null);setFocus(null);setNotice(t('分工已更新，通知将按新的归属交接。'));
   await read(['CREATE','ADOPT','MOVE'].includes(input.action)?target:undefined).catch(()=>setError(t('分工已保存，暂时无法刷新列表，请重试。')));
  }
  catch(e){
   const denied=rejected.has(String(e).match(/\bMANAGEMENT_[A-Z_]+\b/)?.[0]??'');
   setError(errorLabel(e,!denied));
   if(denied){setPending(null);savePending(null);setEditor(null);await read().catch(()=>{});}
  }
  finally{changeLock.current=false;setBusy(false);}
 }
 async function submit(event:FormEvent){
  event.preventDefault();if(!editor||locked)return;
  const input:ManagementChange={requestId:crypto.randomUUID(),action:editor.kind};
  if(editor.zone){input.zoneId=editor.zone.id;input.revision=editor.zone.revision;}
  if(['CREATE','RENAME','ADOPT'].includes(editor.kind))input.name=name.trim();
  if(['CREATE','APPOINT'].includes(editor.kind))input.managerId=choice;
  if(editor.kind==='ADOPT'){input.managerId=editor.adopt!.managerId;input.threadIds=editor.adopt!.threadIds;}
  if(editor.kind==='ADD'){
   const row=data?.candidates.find(c=>c.threadId===choice);if(!row)return;
   if(!row.watchEnabled&&watchApi){setBusy(true);try{await watchApi.enable(row.threadId);await read();const watches=await watchApi.watches();const actual=watches.find(w=>w.threadId===row.threadId);if(!actual)throw Error('MANAGEMENT_ENABLE_WATCH_FIRST');input.generation=actual.generation;}catch(e){setBusy(false);setError(errorLabel(e,false));return;}}
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
  if(!watchApi||openLock.current)return;openLock.current=true;setOpening(row.threadId);setOpenError('');
  try{let watches=await watchApi.watches();let watch=watches.find(w=>w.threadId===row.threadId);if(!watch){await watchApi.enable(row.threadId);watches=await watchApi.watches();watch=watches.find(w=>w.threadId===row.threadId);}if(watch)onOpenWatch(watch);else throw Error('MANAGEMENT_CONVERSATION_UNAVAILABLE');}
  catch(e){setOpenError(errorLabel(e,false));}
  finally{openLock.current=false;setOpening('');}
 }
 const pickingManager=editor?.kind==='CREATE'||editor?.kind==='APPOINT';
 const candidates=data?.candidates.filter(row=>(pickingManager?row.provider!=='PI'&&!membership.has(row.threadId)&&row.threadId!==editor?.zone?.managerId:!membership.has(row.threadId)&&!managers.has(row.threadId))&&`${row.label} ${provider(row)} ${row.cwd} ${row.threadId} ${row.localSessionId??""}`.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()))??[];
 const title=editor?.kind==='CREATE'?t('新建管理区'):editor?.kind==='APPOINT'?t('更换管理对话'):editor?.kind==='MOVE'?t('转移执行对话'):editor?.kind==='ADD'?t('添加执行对话'):editor?.kind==='ADOPT'?t('接管现有分工'):editor?.kind==='REMOVE'?t('移出管理区'):editor?.kind==='DELETE'?t('删除管理区'):t('重命名管理区');
 const valid=editor&&(['CREATE','APPOINT','ADD','MOVE'].includes(editor.kind)?Boolean(choice):true)&&(!['CREATE','ADOPT','RENAME'].includes(editor.kind)||Boolean(name.trim()));
 const legacy=new Map<string,string[]>();for(const link of data?.legacyAssignments??[]){if(!membership.has(link.threadId))legacy.set(link.managerId,[...new Set([...(legacy.get(link.managerId)??[]),link.threadId])]);}
 const attention=(z:ManagementZone)=>z.members.filter(m=>m.conversation&&tierOf(m.conversation)==='ATTENTION').length;
 const refresh=async()=>{if(refreshLock.current)return;refreshLock.current=true;setRefreshing(true);if(!pending)setError('');setNotice('');try{await read();setNotice(t('管理区已刷新。'));}catch{if(!pending)setError(t('暂时无法读取管理区，请重试。'));}finally{refreshLock.current=false;setRefreshing(false);}};
 const pick=(row:ManagedConversation)=>{setChoice(row.threadId);if(editor?.kind==='CREATE'&&!nameTouched)setName(row.label);};

 return <section className="management-page" aria-label={t('管理区拓扑')} data-narrow={narrow||undefined}>
  {data&&data.zones.length>0&&<div className="mz-bar">
   <div ref={zonesBar} className="mz-zones" role="group" aria-label={t('选择管理区')}>{data.zones.map(z=><button type="button" key={z.id} aria-pressed={z.id===selected} disabled={Boolean(pending)&&z.id!==selected} onClick={()=>{if(z.id===selected)return;setSelected(z.id);setRevealedZone(null);setFocus(null);setOpenError('');}}>{z.manager&&<Avatar row={z.manager}/>}<span className="mz-zone-name">{z.name}</span><span className="mz-zone-count" aria-label={t('{0} 条执行对话',z.members.length)}>{z.members.length}</span>{attention(z)>0&&<i className="mz-zone-alert" aria-label={t('{0} 条需要你',attention(z))}/>}</button>)}
   </div>
   <button type="button" className="mz-zone-new" aria-label={t('新建管理区')} disabled={locked} onClick={()=>edit({kind:'CREATE'})}><Plus size={17} aria-hidden/><span>{t('新建')}</span></button>
   {zone&&<div className="mz-view" role="group" aria-label={t('显示方式')}><button type="button" aria-pressed={view==='NORMAL'} aria-label={t('普通视图')} title={t('普通视图')} onClick={()=>setView('NORMAL')}><LayoutList size={17} aria-hidden/><span>{t('普通')}</span></button><button type="button" aria-pressed={view==='MAP'} aria-label={t('星图视图')} title={t('星图视图')} onClick={()=>setView('MAP')}><Orbit size={17} aria-hidden/><span>{t('星图')}</span></button></div>}
   {zone&&<Menu label={t('管理区操作')}>{close=><>
    <button type="button" role="menuitem" disabled={locked} onClick={()=>{close();edit({kind:'APPOINT',zone});}}><UserRoundCog size={16} aria-hidden/>{zone.manager?t('更换管理对话'):t('选择管理对话')}</button>
    <button type="button" role="menuitem" disabled={locked} onClick={()=>{close();edit({kind:'RENAME',zone});}}>{t('重命名')}</button>
    <button type="button" role="menuitem" disabled={busy||refreshing} onClick={()=>{close();void refresh();}}><RefreshCw size={16} aria-hidden/>{t('刷新管理区')}</button>
    <hr/>
    <button type="button" role="menuitem" className="mz-danger" disabled={locked} onClick={()=>{close();edit({kind:'DELETE',zone});}}>{t('删除管理区')}</button>
   </>}</Menu>}
  </div>}
  {(loading||refreshing)&&<p role="status" className="mz-loading">{t('正在读取分工…')}</p>}
  {error&&!editor&&<div className="management-error" role="alert"><p>{error}</p>{pending&&<button type="button" className="management-primary" disabled={busy} onClick={()=>void apply(pending)}>{t('检查并完成这次操作')}</button>}{!pending&&<button type="button" disabled={busy||refreshing} onClick={()=>void refresh()}>{t('重新读取')}</button>}</div>}
  {notice&&<p className="management-notice" role="status"><Check size={16} aria-hidden/>{notice}</p>}

  {legacy.size>0&&<section className="management-legacy"><div><h2>{t('已有分工可以直接接管')}</h2><p>{t('这些对话已经登记通知，不用逐条重新添加。')}</p></div>{[...legacy].map(([managerId,threadIds])=>{const row=data?.candidates.find(c=>c.threadId===managerId);return <button type="button" key={managerId} disabled={!row||locked} onClick={()=>{const target=data?.zones.find(z=>z.id===selected&&z.managerId===managerId)??data?.zones.find(z=>z.managerId===managerId);edit({kind:'ADOPT',zone:target,adopt:{managerId,threadIds}});setName(target?.name??row?.label??t('管理区'));}}>{row&&<Avatar row={row}/>}<span>{row?.label??managerId}<small>{threadIds.length} {t('条执行对话')}</small></span><span className="mz-legacy-go">{t('接管')}</span></button>;})}</section>}

  {zone&&hiddenCount>0&&<button type="button" className="mz-inactive-toggle" aria-expanded={showInactive} onClick={()=>{
   setRevealedZone(showInactive?null:zone.id);
   if(showInactive&&focus?.role==='MEMBER'&&isInactive(focused,now))setFocus(null);
  }}><ChevronDown size={16} aria-hidden/>{showInactive?t('收起 {0} 条不活跃对话',hiddenCount):t('显示 {0} 条不活跃对话',hiddenCount)}</button>}
  {zone&&graphZone&&<div className="mz-stage" data-inspecting={Boolean(focused)||undefined}>
   <Graph zone={graphZone} view={view} narrow={narrow} focus={focus} disabled={locked} onFocus={f=>{if(focus?.threadId===f.threadId){closeInspector();return;}inspectorOpener.current=document.activeElement instanceof HTMLElement?document.activeElement:null;setOpenError('');setFocus(f);}} onAdd={()=>edit({kind:'ADD',zone})}/>
   {focused&&focus&&focusZone&&<aside className="mz-inspector" aria-label={t('对话详情')} onKeyDown={e=>{if(e.key==='Escape'){e.stopPropagation();closeInspector();}}}>
    <div className="mz-inspector-head"><Avatar row={focused}/><div><small>{focus.role==='MANAGER'?t('管理对话'):t('执行对话')} · {provider(focused)}</small><h3>{focused.label}</h3><Status row={focused}/></div><button ref={inspectorClose} type="button" className="management-icon" aria-label={t('关闭详情')} onClick={closeInspector}><X size={18} aria-hidden/></button></div>
    {focused.cwd&&<p className="mz-inspector-meta">{t('项目')} · {folder(focused.cwd)}</p>}
    <div className="mz-inspector-actions">
     {watchApi&&<button type="button" className="management-primary" disabled={Boolean(opening)} onClick={()=>void open(focused)}><ExternalLink size={16} aria-hidden/>{t(opening===focused.threadId?'正在打开对话…':'打开对话')}</button>}
     {focus.role==='MANAGER'?<button type="button" disabled={locked} onClick={()=>edit({kind:'APPOINT',zone:focusZone})}><UserRoundCog size={16} aria-hidden/>{t('更换管理对话')}</button>:<>
      <button type="button" disabled={locked||data!.zones.length<2} title={data!.zones.length<2?t('还没有其他管理区'):undefined} onClick={()=>edit({kind:'MOVE',zone:focusZone,source:focused})}><ArrowRightLeft size={16} aria-hidden/>{t('转移')}</button>
      <button type="button" className="mz-quiet" disabled={locked} onClick={()=>edit({kind:'REMOVE',zone:focusZone,source:focused})}>{t('移出')}</button></>}
    </div>
    {openError&&<p className="management-error" role="alert">{openError}</p>}
    {focus.role==='MANAGER'&&<p className="mz-inspector-note">{t('分工不改变原对话权限。管理对话仍须能使用 Agbrio MCP；通知插件与 MCP 工具是两条独立连接。')}</p>}
   </aside>}
   {focused&&narrow&&<div className="mz-sheet-backdrop" aria-hidden onClick={closeInspector}/>}
  </div>}

  {!loading&&data&&!data.zones.length&&<div className="management-empty"><GitBranch size={36} aria-hidden/><h2>{t('先建立你的管理区')}</h2><p>{t('按业务线分工：选一条 Claude Code 或 Codex 对话当管理，再把它负责的执行对话加进来。每条执行对话只归属一个管理区。')}</p><button type="button" className="management-primary" disabled={locked} onClick={()=>edit({kind:'CREATE'})}><Plus size={16} aria-hidden/>{t('新建管理区')}</button></div>}

  <Dialog.Root open={Boolean(editor)} onOpenChange={value=>{if(!value&&!locked)setEditor(null);}}>
   <Dialog.Portal><Dialog.Overlay className="mz-overlay"/><Dialog.Content className="mz-dialog" aria-describedby={undefined} onCloseAutoFocus={event=>{const opener=editorOpener.current;const target=opener?.isConnected?opener:zonesBar.current?.querySelector<HTMLButtonElement>('[aria-pressed=true]');if(target){event.preventDefault();target.focus({preventScroll:true});}}} onPointerDownOutside={e=>{if(locked)e.preventDefault();}} onEscapeKeyDown={e=>{if(locked)e.preventDefault();}}>
    {editor&&<form className="management-editor" onSubmit={e=>void submit(e)} aria-label={title} aria-busy={busy}>
     <div className="management-editor-title"><Dialog.Title asChild><h2>{title}</h2></Dialog.Title><button type="button" className="management-icon" disabled={locked} aria-label={t('关闭')} onClick={()=>setEditor(null)}><X size={18}/></button></div>
     {editor.source&&<div className="mz-editor-source"><Avatar row={editor.source}/><span><strong>{editor.source.label}</strong><small>{t('当前在')} {editor.zone?.name}</small></span></div>}
     {(pickingManager||editor.kind==='ADD')&&<><label className="management-search"><Search size={17} aria-hidden/><input type="search" value={query} disabled={locked} onChange={e=>setQuery(e.target.value)} placeholder={t('搜索对话或项目')} aria-label={t('搜索对话或项目')}/></label><div className="management-picker" role="radiogroup" aria-label={pickingManager?t('选择管理对话'):t('选择执行对话')}>{candidates.map(row=><button key={row.threadId} type="button" role="radio" aria-checked={choice===row.threadId} disabled={locked} onClick={()=>pick(row)}><Avatar row={row}/><span><strong>{row.label}</strong><small>{provider(row)} · {folder(row.cwd)}{!row.watchEnabled?` · ${t('将自动开启监听')}`:''}</small></span>{choice===row.threadId&&<Check size={18} aria-hidden/>}</button>)}{!candidates.length&&<p className="mz-picker-empty">{query.trim()?t('没有匹配的对话，换个关键词试试。'):t('还没有可选对话。先在 Codex 或 Claude Code 里打开要管理的对话，它会出现在这里。')}</p>}</div></>}
     {(['CREATE','RENAME'].includes(editor.kind)||(editor.kind==='ADOPT'&&!editor.zone))&&<label className="management-field">{t('管理区名称')}<input autoFocus={editor.kind!=='CREATE'} maxLength={80} value={name} disabled={locked} onChange={e=>{setName(e.target.value);setNameTouched(true);}} placeholder={t('例如：游戏开发、美术制作')}/></label>}
     {editor.kind==='ADOPT'&&<label className="management-field">{t('选择管理区')}<select aria-label={t('选择管理区')} disabled={locked} value={editor.zone?.id??''} onChange={e=>{const target=data?.zones.find(z=>z.id===e.target.value);setEditor({...editor,zone:target});setName(target?.name??data?.candidates.find(c=>c.threadId===editor.adopt?.managerId)?.label??t('管理区'));}}><option value="">{t('新建管理区')}</option>{data?.zones.filter(z=>z.managerId===editor.adopt?.managerId).map(z=><option key={z.id} value={z.id}>{z.name}</option>)}</select></label>}
     {editor.kind==='MOVE'&&<div className="mz-targets" role="radiogroup" aria-label={t('转移到')}>{data?.zones.filter(z=>z.id!==editor.zone?.id).map(z=><button type="button" role="radio" key={z.id} aria-checked={choice===z.id} disabled={locked} onClick={()=>setChoice(z.id)}>{z.manager?<Avatar row={z.manager}/>:<span className="v6-avatar v6-avatar-empty" aria-hidden/>}<span><strong>{z.name}</strong><small>{z.manager?`${t('管理')} · ${z.manager.label}`:t('还没有管理对话')} · {t('{0} 条执行对话',z.members.length)}</small></span>{choice===z.id&&<Check size={18} aria-hidden/>}</button>)}</div>}
     <p className="mz-editor-note">{editor.kind==='MOVE'?t('一次完成转移，不会先移出再加入。当前任务继续运行，后续提问和结果交给目标管理区，无需重新订阅。'):editor.kind==='APPOINT'?t('原任务和权限保留。尚未发送的通知交给新管理对话；送达未确认的通知先核对，不会重复发送。'):editor.kind==='ADOPT'?`${(editor.zone?t('将这些执行对话并入所选管理区，保留原身份和进度，不会重发任务。'):t('将现有订阅整理为一个管理区，执行对话保留原身份和进度，不会重发任务。'))} ${editor.adopt?.threadIds.length} ${t('条执行对话')}`:editor.kind==='REMOVE'?t('只移除管理归属，不停止任务、不删除原对话。'):editor.kind==='DELETE'?t('删除管理区并移出全部执行对话，取消本区通知订阅。原对话和正在运行的任务保留。'):editor.kind==='CREATE'?t('管理对话负责接收本区执行对话的提问和结果。任命不会扩大它的权限。'):editor.kind==='ADD'?t('加入后，这条对话的提问和结果交给本区管理对话。'):''}</p>
     {error&&<div className="management-error" role="alert"><p>{error}</p>{pending&&<button type="button" className="management-primary" disabled={busy} onClick={()=>void apply(pending)}>{t('检查并完成这次操作')}</button>}</div>}
     <footer><button type="button" disabled={locked} onClick={()=>setEditor(null)}>{t('取消')}</button><button type="submit" className={editor.kind==='DELETE'||editor.kind==='REMOVE'?'management-primary mz-confirm-danger':'management-primary'} disabled={!valid||locked}>{busy?t('正在处理…'):editor.kind==='CREATE'?t('创建管理区'):editor.kind==='APPOINT'?t('确认更换'):editor.kind==='MOVE'?t('确认转移'):editor.kind==='ADD'?t('加入管理区'):editor.kind==='DELETE'?t('删除管理区'):t('确认')}</button></footer>
    </form>}
   </Dialog.Content></Dialog.Portal>
  </Dialog.Root>
 </section>;
}
