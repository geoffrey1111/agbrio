import {failedAttentionKey,useDismissedAttention} from './attentionDismissals';
import {piDelegations} from './piDelegations';
import {PullToRefresh} from './PullToRefresh';
import {isQuietWatch,watchStartError,threadProviderLabel,threadAvatarKind} from './watchPresentation';
import {claudeTaskSummary} from "./ClaudeTaskStatus";
import {t as uiText,tc,useLanguage,getLanguage} from "../../i18n";
import {useEffect,useMemo,useRef,useState,type ReactNode} from "react";
import * as Dialog from "@radix-ui/react-dialog";
import {ChevronDown,ChevronLeft,ChevronRight,Link2,MessageSquare,Search,X} from "lucide-react";
import {SwipeDeleteRow} from "./SwipeDeleteRow";
import {AnimatePresence,LayoutGroup,motion,useReducedMotion} from "motion/react";
import {avatarFor} from "./avatarPool";
import {ModelBadge} from "./ModelBadge";
import {watchRuntime} from "./modelLabel";
import {messagePreview} from "./messagePreview";
import {bridgeReviewAttention,focusedBridgeActivity,type BridgeActivity} from "./bridgeActivity";
import {useWatches} from "./sharedWatches";
import {buildChildTree,childTreeSummary,type ChildEntry,type ChildTreeModel} from "./childTree";
import {ChildTreeList} from "./ChildTreeList";
import {markResultSeen,useSeenResults} from "./seenResults";
import type {CodexWatch,NotificationApi} from "./CodexNotifications";
import type {ExistingCodexThreadCandidate} from "../codex/types";
import type {WorkbenchItem} from "./models";
import {providerOwnerAttention} from "./providerAttention";

/** One list for everything the owner follows: Bridges and watched
 * conversations. Hosts stay the only routing and mutation authority; this
 * component reads their existing projections and calls their existing actions. */

type Tone="wait"|"run"|"done";
type Row={tree?:ChildTreeModel;key:string;kind:"BRIDGE"|"WATCH";title:string;meta:string;source:string;preview:string;at:number;tone:Tone;state:string;unread:boolean;newResult?:boolean;bridge?:WorkbenchItem;watch?:CodexWatch};
type Filter="ALL"|"RUN"|"BRIDGE";
type ActivityRows=Record<string,{row:BridgeActivity;receivedAt:number}>;
/** Where to land when opening a watched conversation from a tree row. */
export type WatchOpenTarget={childId?:string;parentLabel?:string};

const isClaude=(id:string)=>id.startsWith("claude_code:");
const spring={type:"spring",stiffness:520,damping:39,mass:.7} as const;
/** Persistent DiceBear avatar plus a small provider badge. */
export function ConversationAvatar({id,kind}:{id:string;kind:"bridge"|"codex"|"claude"|"pi"}){
 const reduced=useReducedMotion();
 return <motion.span className="v6-avatar" aria-hidden initial={reduced?false:{opacity:.35}} animate={{opacity:1}} transition={{duration:reduced?0:.14}}><img src={avatarFor(id)} alt="" draggable={false}/><i className={`v6-badge v6-${kind}`}>{kind==="bridge"?<Link2 size={10} strokeWidth={2.6}/>:kind==="claude"?"✳":kind==="pi"?"π":"C"}</i></motion.span>;
}
const folder=(path:string)=>path.split(/[\\/]/).filter(Boolean).at(-1)??"";
const messageTime=(value:number|null|undefined)=>typeof value==="number"&&Number.isFinite(value)&&value>0?value:0;
export const when=(value:number)=>{if(!value)return"";const minutes=(Date.now()-value)/60000;if(minutes<1)return uiText("刚刚");if(minutes<60)return tc("{0} 分钟",Math.floor(minutes));const date=new Date(value);return minutes<1440?new Intl.DateTimeFormat(getLanguage(),{hour:"2-digit",minute:"2-digit"}).format(date):new Intl.DateTimeFormat(getLanguage(),{month:"numeric",day:"numeric"}).format(date);};

export function watchTone(w:CodexWatch):[Tone,string]{
 if(!w.enabled&&!w.threadId.startsWith("pi:"))return["done",uiText("已暂停")];
 // A classified upstream account problem (balance, quota, auth) is the owner's to fix, not a read failure.
 if(w.threadId.startsWith("pi:")&&providerOwnerAttention(w.errorCode))return["wait",uiText("需要你处理")];
 if(w.threadId.startsWith("pi:")&&w.snapshot.state==="FAILED")return["wait",uiText("执行失败")];
 if(w.errorCode)return["done",uiText("暂时无法读取")];
 if(w.snapshot.state==="ACTION_REQUIRED")return ["wait",uiText("等你确认")];
 if(w.snapshot.state==="FAILED")return ["wait",uiText("执行失败")];
 if(w.taskTree?.attention)return ["wait",claudeTaskSummary(w.taskTree)!];
 if(w.taskTree?.running)return ["run",claudeTaskSummary(w.taskTree)!];
 if(w.taskTree&&(w.taskTree.unknown||w.taskTree.limited)&&w.snapshot.state!=="RUNNING")return ["done",claudeTaskSummary(w.taskTree)!];
 switch(w.snapshot.state){
  case"RUNNING":return["run",uiText("执行中")];
  case"ACTION_REQUIRED":return["wait",uiText("等你确认")];
  case"FAILED":return["wait",uiText("执行失败")];
  case"RESULT_READY":return["done",uiText(isClaude(w.threadId)?"本轮已结束":"结果已到达")];
  case"INTERRUPTED":return["done",uiText("已停止")];
  default:return["done",""];
 }
}
/** A parent's own state, then the merged delegated-conversation tree. */
function parentTone(w:CodexWatch,tree:ChildTreeModel|undefined,failureDismissed:boolean):[Tone,string]{
 const [raw,own]=watchTone({...w,taskTree:undefined});
 const tone=w.snapshot.state==="FAILED"&&failureDismissed?"done":raw;
 if(tone==="wait"||!tree?.total)return[tone,own];
 const needs=tree.wait+tree.fail;
 if(needs)return["wait",tc("子对话等你 {0}",needs)];
 if(tone==="run")return[tone,own];
 if(tree.run)return["run",tc("子对话执行中 {0}",tree.run)];
 if(tree.unknown||tree.limited)return["done",uiText("子对话状态尚未全部确认")];
 return[tone,own];
}
export function bridgeTone(row:BridgeActivity|undefined,age:number,item:WorkbenchItem):[Tone,string]{
 // Before the first activity read, the directory attention count still tells the truth.
 if(!row)return (item.attentionCount??0)>0?["wait",uiText("有新回复")]:["done",""];
 const focus=focusedBridgeActivity(row,age),unread=bridgeReviewAttention(row,age);
 if(focus&&["RUNNING","THINKING","GOAL_ACTIVE"].includes(focus.side.phase))return["run",focus.side.role==="DECISION"?uiText("控制端执行中"):uiText("执行端执行中")];
 if(focus?.side.phase==="ACTION_REQUIRED")return["wait",uiText("等你确认")];
 if(unread>0)return["wait",uiText("有新回复")];
 return["done",""];
}

// Device-local presentation memory of which parents show their tree.
const expandedKey="agbrio.child-tree-expanded.v1";
function readExpanded(){try{const v=JSON.parse(localStorage.getItem(expandedKey)??"[]");return new Set<string>(Array.isArray(v)?v.filter((x):x is string=>typeof x==="string").slice(-200):[]);}catch{return new Set<string>();}}

export function ConversationHome({selectedKey,bridges,activity,activityClock,watchApi,quotaPanel,addRequest,onOpenBridge,onOpenWatch,onCreateBridge,onDeleteBridge,onRefreshList,scrollContainer}:{
 onRefreshList?:()=>Promise<unknown>;scrollContainer?:import("react").RefObject<HTMLElement|null>;
 /** Row key (`w:<threadId>` / `b:<bridgeId>`) open beside the list on wide desktops. */
 selectedKey?:string;
 bridges:WorkbenchItem[];activity:ActivityRows;activityClock:number;watchApi?:NotificationApi;quotaPanel?:ReactNode;addRequest:number;
 onOpenBridge:(id:string)=>void;onOpenWatch:(watch:CodexWatch,target?:WatchOpenTarget)=>void;onCreateBridge?:(name:string)=>Promise<void>;onDeleteBridge?:(item:WorkbenchItem)=>void;
}){
 useLanguage();
 const reduced=useReducedMotion();
 const isDismissed=useDismissedAttention();
 const seen=useSeenResults();
 const [hiddenWatches,setHiddenWatches]=useState<string[]>([]);
 const [watched,refreshWatches]=useWatches(watchApi);
 const watches=watched.filter(w=>!hiddenWatches.includes(w.threadId));
 const [undo,setUndo]=useState<CodexWatch|null>(null);
 useEffect(()=>{if(!undo)return;const timer=setTimeout(()=>setUndo(null),6000);return()=>clearTimeout(timer);},[undo]);
 // Swiping a watched conversation only stops Agbrio's tracking; the chat itself is untouched and can be restored.
 async function stopWatching(w:CodexWatch){if(!watchApi?.remove)return;setHiddenWatches(old=>[...old,w.threadId]);try{await watchApi.remove("WATCH",w.threadId,true);setUndo(w);}catch{setHiddenWatches(old=>old.filter(id=>id!==w.threadId));}}
 async function restoreWatch(w:CodexWatch){setUndo(null);try{await watchApi?.remove?.("WATCH",w.threadId,false);}finally{setHiddenWatches(old=>old.filter(id=>id!==w.threadId));}}
 const [query,setQuery]=useState(""),[filter,setFilter]=useState<Filter>("ALL");
 const [expanded,setExpanded]=useState<Set<string>>(readExpanded);
 const toggleTree=(key:string)=>setExpanded(old=>{const next=new Set(old);if(next.has(key))next.delete(key);else next.add(key);try{localStorage.setItem(expandedKey,JSON.stringify([...next].slice(-200)));}catch{/* presentation only */}return next;});
 const [sheet,setSheet]=useState<"CHOOSE"|"WATCH"|"BRIDGE"|null>(null);
 const seenAdd=useRef(addRequest);
 useEffect(()=>{if(addRequest!==seenAdd.current){seenAdd.current=addRequest;setSheet("CHOOSE");}},[addRequest]);
 const now=Date.now();

 const rows=useMemo<Row[]>(()=>{
  const b=bridges.map<Row>(item=>{const entry=activity[item.id];const [tone,state]=bridgeTone(entry?.row,entry?Math.max(0,activityClock-entry.receivedAt):Infinity,item);
   const meta=`Bridge · ${item.sourceLabel||item.projectName||uiText("待绑定")}`;
   return{key:`b:${item.id}`,kind:"BRIDGE",title:item.name,meta,source:meta,preview:"",at:messageTime(entry?.row.lastMessageAt??item.lastMessageAt),tone,state,unread:tone==="wait",bridge:item};});
  const delegation=piDelegations(watches);
  const w=watches.filter(watch=>!delegation.nested.has(watch.threadId)).map<Row>(watch=>{
   const pi=delegation.children.get(watch.threadId)??[];
   const tree=watch.taskTree?.nodes.length||watch.taskTree?.limited||pi.length?buildChildTree({parentThreadId:watch.threadId,tree:watch.taskTree,pi,now,isDismissed,seen}):undefined;
   const [tone,state]=parentTone(watch,tree,isDismissed(failedAttentionKey(watch)));
   const at=Math.max(messageTime(watch.lastMessageAt),...pi.map(c=>messageTime(c.lastMessageAt)));
   return{key:`w:${watch.threadId}`,kind:"WATCH",title:watch.label,meta:`${threadProviderLabel(watch.threadId)}${watch.cwd?` · ${folder(watch.cwd)}`:""}`,source:threadProviderLabel(watch.threadId),preview:messagePreview(watch.snapshot.text),at,tone,state,unread:false,newResult:watch.unreadResult===true&&!seen(watch.threadId,watch.lastMessageAt),tree,watch};});
  const rank={run:0,wait:1,done:2};
  return [...b,...w].sort((x,y)=>rank[x.tone]-rank[y.tone]||y.at-x.at||(x.key<y.key?-1:x.key>y.key?1:0));
 },[bridges,activity,activityClock,watches,isDismissed,seen]);
 const counts={run:rows.filter(r=>r.tone==="run").length};
 const q=query.trim().toLocaleLowerCase();
 const childHit=(r:Row)=>Boolean(q&&r.tree&&[...r.tree.active,...r.tree.ended].some(e=>e.title.toLocaleLowerCase().includes(q)));
 const visible=rows.filter(r=>(filter==="ALL"||filter==="RUN"&&r.tone==="run"||filter===r.kind)&&(!q||`${r.title} ${r.meta}`.toLocaleLowerCase().includes(q)||childHit(r)));
 const quietIds=new Set(watches.filter(w=>isQuietWatch(w)).map(w=>w.threadId));
 const quiet=!q?visible.filter(r=>r.tone==="done"&&r.watch&&quietIds.has(r.watch.threadId)):[];
 const collapsed=new Set(quiet.map(r=>r.key));
 const active=visible.filter(r=>r.tone!=="done"),rest=visible.filter(r=>r.tone==="done"&&!collapsed.has(r.key));
 const [quietError,setQuietError]=useState<string|null>(null);
 async function showQuiet(w:CodexWatch){if(!watchApi?.show)return;setQuietError(null);try{await watchApi.show(w.threadId);}catch(e){setQuietError(watchStartError(e));}}
 const openWatch=(w:CodexWatch,target?:WatchOpenTarget)=>{if(!target?.childId){markResultSeen(w.threadId,w.lastMessageAt);
  // T5: clear this conversation's captured result notices; an older Host lacks it and the local seen mark stands.
  if(w.unreadResult===true)void watchApi?.markThreadRead?.(w.threadId).catch(()=>undefined);}if(target)onOpenWatch(w,target);else onOpenWatch(w);};
 const open=(r:Row)=>r.bridge?onOpenBridge(r.bridge.id):r.watch&&openWatch(r.watch);
 const openChild=(r:Row,e:ChildEntry)=>{if(!r.watch)return;markResultSeen(e.key,e.at);if(e.kind==="pi"&&e.watch)openWatch(e.watch,{parentLabel:r.title});else if(e.node)openWatch(r.watch,{childId:e.node.sessionId});};
 // data-bridge-id lets useBridgeActivity read status only for rows on screen.
 const item=(r:Row):ReactNode=>{const body=<motion.button type="button" whileTap={reduced?undefined:{scale:.97}} transition={spring} className="v6-row" data-tone={r.tone} data-bridge-id={r.bridge?.id} aria-current={selectedKey===r.key?"true":undefined} onClick={()=>open(r)}>
   <ConversationAvatar id={r.key} kind={r.kind==="BRIDGE"?"bridge":threadAvatarKind(r.watch!.threadId)}/>
   <span className="v6-row-body"><span className="v6-row-top"><strong>{r.title}</strong>{r.newResult&&<i className="v7-result-dot" role="img" aria-label={uiText("有新结果")}/>}{r.at?<time>{when(r.at)}</time>:null}</span><span className="v6-row-meta"><span className="v7-meta-text" title={r.meta}>{r.source}</span>{r.watch&&<ModelBadge runtime={watchRuntime(r.watch)}/>}</span>{r.preview&&<span className="v6-row-preview">{r.preview}</span>}{r.state&&<motion.span key={r.state} className="v6-state" data-tone={r.tone} initial={reduced?false:{opacity:.5}} animate={{opacity:1}} transition={{duration:reduced?0:.12}}>{r.tone==="run"&&<i aria-hidden/>}{r.state}</motion.span>}</span>
   <AnimatePresence>{r.unread&&<motion.i className="v6-unread" aria-label={uiText("需要你处理")} initial={{scale:0}} animate={{scale:1}} exit={{scale:0}} transition={{type:"spring",stiffness:600,damping:14}}/>}</AnimatePresence>
  </motion.button>;
  const inner=r.bridge&&onDeleteBridge?<SwipeDeleteRow label={r.title} onDelete={()=>onDeleteBridge(r.bridge!)}>{body}</SwipeDeleteRow>:r.watch&&!r.watch.threadId.startsWith("pi:")&&watchApi?.remove?<SwipeDeleteRow label={r.title} onDelete={()=>void stopWatching(r.watch!)}>{body}</SwipeDeleteRow>:body;
  const showTree=Boolean(r.tree&&(expanded.has(r.key)||childHit(r)));
  const selectedChild=selectedKey?.startsWith("w:")?selectedKey.slice(2):undefined;
  const grouped=r.tree?.total?<div className="v7-tree-parent">{inner}<button type="button" className="v7-tree-toggle" aria-expanded={showTree} onClick={()=>toggleTree(r.key)}>{showTree?<ChevronDown size={14} aria-hidden/>:<ChevronRight size={14} aria-hidden/>}<span>{childTreeSummary(r.tree)}</span></button>{showTree&&<div className="v7-tree-list"><ChildTreeList model={r.tree} selectedKey={selectedChild} label={uiText("{0} 的子对话",r.title)} onOpen={e=>openChild(r,e)}/></div>}</div>:inner;
 return <motion.div key={r.key} layout="position" initial={reduced?false:{opacity:.35}} animate={{opacity:1}} exit={{opacity:0,transition:{duration:reduced?0:.12}}} transition={reduced?{duration:0}:spring}>{grouped}</motion.div>;};
 // One row that fits a 375px phone: all, running and the Bridge type.
 const chips:[Filter,string,number?][]=[["ALL",uiText("全部")],["RUN",uiText("执行中"),counts.run],["BRIDGE","Bridge"]];
 async function refreshList(){const results=await Promise.allSettled([refreshWatches(),onRefreshList?.()]);if(results.some(r=>r.status==="rejected"))throw Error("LIST_REFRESH_INCOMPLETE");}
 return <PullToRefresh enabled={Boolean(onRefreshList)} onRefresh={refreshList} scrollContainer={scrollContainer}><section className="v6-home" data-mode="ALL" aria-label={uiText("对话")}>
  <label className="v6-search"><Search size={16} aria-hidden/><input type="search" aria-label={uiText("搜索对话、子对话或项目")} placeholder={uiText("搜索对话、子对话或项目")} value={query} onChange={e=>setQuery(e.target.value)}/>{query&&<button type="button" aria-label={uiText("清除搜索")} onClick={()=>setQuery("")}><X size={16}/></button>}</label>
  <LayoutGroup id="v6-chips"><div className="v6-chips" role="group" aria-label={uiText("筛选")}>{chips.map(([id,label,n])=><motion.button key={id} type="button" whileTap={{scale:.92}} aria-pressed={filter===id} onClick={()=>setFilter(id)}>{filter===id&&<motion.span className="v6-chip-fill" layoutId="v6-chip-fill" transition={spring}/>}<span>{label}{n?<b>{n}</b>:null}</span></motion.button>)}</div></LayoutGroup>
  {visible.length>0&&<div className="v6-list"><AnimatePresence>{[...active.map(item),...(rest.length?[<motion.small layout="position" key="section" className="v6-section">{active.length?uiText("其他"):uiText("最近")}</motion.small>]:[]),...rest.map(item)]}</AnimatePresence></div>}
  {quiet.length>0&&<details className="v6-quiet-watches"><summary>{uiText("较早的对话")} <span>{quiet.length}</span></summary><p>{uiText("三天未更新的对话收在这里，仍会继续监听。有新进展或手动移回最近后重新显示。")}</p><div className="v6-list">{quiet.map(r=><div className="v6-quiet-item" key={r.key}>{item(r)}{watchApi?.show&&<button type="button" className="v6-quiet-restore" aria-label={uiText("移回最近：{0}",r.title)} onClick={()=>void showQuiet(r.watch!)}>{uiText("移回最近")}</button>}</div>)}</div>{quietError&&<p role="alert">{quietError}</p>}</details>}
  {!visible.length&&<div className="v6-empty">{rows.length?<p>{uiText("没有符合条件的对话")}</p>:<><MessageSquare size={30} aria-hidden/><h2>{uiText("还没有关注的对话")}</h2><p>{uiText("监听电脑上的 Codex 或 Claude Code 对话，或者把两个对话连成 Bridge。")}</p><button type="button" className="v6-primary" onClick={()=>setSheet("CHOOSE")}>{uiText("开始")}</button></>}</div>}
  <AnimatePresence>{undo&&<motion.div className="v6-toast" role="status" initial={{y:24,opacity:0}} animate={{y:0,opacity:1}} exit={{y:24,opacity:0}} transition={spring}>{uiText("已停止监听")}<button type="button" onClick={()=>void restoreWatch(undo)}>{uiText("撤销")}</button></motion.div>}</AnimatePresence>
  <AddSheet mode={sheet} setMode={setSheet} watchApi={watchApi} watched={watches} quietIds={quietIds} onCreateBridge={onCreateBridge} onWatched={w=>{setSheet(null);onOpenWatch(w);}}/>
 </section></PullToRefresh>;
}

function AddSheet({mode,setMode,watchApi,watched,quietIds,onCreateBridge,onWatched}:{mode:"CHOOSE"|"WATCH"|"BRIDGE"|null;setMode:(m:"CHOOSE"|"WATCH"|"BRIDGE"|null)=>void;watchApi?:NotificationApi;watched:CodexWatch[];quietIds:Set<string>;onCreateBridge?:(name:string)=>Promise<void>;onWatched:(w:CodexWatch)=>void}){
 useLanguage();
 const [threads,setThreads]=useState<ExistingCodexThreadCandidate[]|null>(null),[query,setQuery]=useState(""),[kind,setKind]=useState<"ALL"|"CODEX"|"CLAUDE">("ALL");
 const [name,setName]=useState(""),[busy,setBusy]=useState(false),[error,setError]=useState<string|null>(null);
 const [reload,setReload]=useState(0);
 useEffect(()=>{if(!mode){setQuery("");setKind("ALL");setError(null);setName("");}},[mode]);
 useEffect(()=>{
  if(mode!=="WATCH"||!watchApi)return;let alive=true;setThreads(null);setError(null);
  void (async()=>{try{if(reload>0){try{await watchApi.refreshWatches?.();}catch{/* retain cached acknowledgements */}}try{await watchApi.connect();}catch{/* Claude catalog remains usable without a Codex transport. */}const next=await watchApi.threads();if(alive)setThreads(next.threads);}catch(e){if(alive){setThreads([]);setError(String(e).includes("ROUTER_READ_TIMEOUT")?uiText("读取超时，请稍后重试。"):uiText("暂时读不到电脑上的对话。请确认电脑上的 Router 正在运行。"));}}})();
  return()=>{alive=false;};
 },[mode,watchApi,reload]);
 const enabled=new Set(watched.filter(w=>w.enabled&&!(watchApi?.show&&quietIds.has(w.threadId))).map(w=>w.threadId));
 const q=query.trim().toLocaleLowerCase();
 const list=(threads??[]).filter(t=>!enabled.has(t.id)&&(kind==="ALL"||(kind==="CLAUDE")===isClaude(t.id))&&(!q||`${t.label} ${t.projectProvenance??""} ${t.projectLabel??""}`.toLocaleLowerCase().includes(q)))
  .sort((a,b)=>(b.recencyAt??0)-(a.recencyAt??0));
 async function watch(t:ExistingCodexThreadCandidate){
  if(!watchApi||busy)return;setBusy(true);setError(null);
  let fallback:CodexWatch|undefined;
  try{
   const current=watched.find(w=>w.threadId===t.id&&w.enabled);
   if(current&&quietIds.has(t.id)&&watchApi.show){const result=await watchApi.show(t.id);fallback={...current,shownAt:result.shownAt};}
   else await watchApi.enable(t.id);
  }catch(e){setError(watchStartError(e));setBusy(false);return;}
  // The write was acknowledged. A later failed read must not turn it into a
  // fake failure or invite another enable operation.
  try{const next=await (watchApi.refreshWatches?.()??watchApi.watches());fallback=next.find(x=>x.threadId===t.id)??fallback;}catch{/* normal observation refresh continues */}
  if(fallback)onWatched(fallback);else setMode(null);
  setBusy(false);
 }
 async function create(){
  if(!onCreateBridge||busy)return;setBusy(true);setError(null);
  try{await onCreateBridge(name.trim()||uiText("新 Bridge"));setMode(null);}catch{setError(uiText("没有创建成功，请重试。"));}finally{setBusy(false);}
 }
 const title=mode==="WATCH"?uiText("监听对话"):mode==="BRIDGE"?uiText("新建 Bridge"):uiText("新建");
 return <Dialog.Root open={Boolean(mode)} onOpenChange={open=>{if(!open&&!busy)setMode(null);}}><Dialog.Portal><Dialog.Overlay asChild><motion.div className="v6-scrim" initial={{opacity:0}} animate={{opacity:1}}/></Dialog.Overlay><Dialog.Content asChild aria-describedby={undefined}><motion.div className="v6-sheet" initial={{y:"100%"}} animate={{y:0}} transition={{type:"spring",stiffness:380,damping:34,mass:.9}}>
  <span className="v6-grab" aria-hidden/>
  <header className="v6-sheet-head">{mode!=="CHOOSE"&&<button type="button" className="v6-icon" aria-label={uiText("返回")} disabled={busy} onClick={()=>setMode("CHOOSE")}><ChevronLeft size={22}/></button>}<Dialog.Title>{title}</Dialog.Title><Dialog.Close asChild><button type="button" className="v6-icon" aria-label={uiText("关闭")} disabled={busy}><X size={20}/></button></Dialog.Close></header>
  <AnimatePresence mode="wait" initial={false}><motion.div key={mode??"none"} className="v6-step" initial={{opacity:0,x:mode==="CHOOSE"?-24:24}} animate={{opacity:1,x:0}} exit={{opacity:0,x:mode==="CHOOSE"?24:-24}} transition={{type:"spring",stiffness:500,damping:38}}>
  {mode==="CHOOSE"&&<div className="v6-choices">
   <motion.button type="button" initial={{opacity:0,y:12}} animate={{opacity:1,y:0}} transition={{...spring,delay:.04}} whileTap={{scale:.97}} className="v6-choice" disabled={!watchApi} onClick={()=>setMode("WATCH")}><span className="v6-glyph"><MessageSquare size={22}/></span><span><strong>{uiText("监听一个对话")}</strong><small>{uiText("选电脑上的 Codex 或 Claude Code 对话。回复到达时通知你，可以直接回复。")}</small></span></motion.button>
   <motion.button type="button" initial={{opacity:0,y:12}} animate={{opacity:1,y:0}} transition={{...spring,delay:.1}} whileTap={{scale:.97}} className="v6-choice" disabled={!onCreateBridge} onClick={()=>setMode("BRIDGE")}><span className="v6-glyph"><Link2 size={22}/></span><span><strong>{uiText("新建 Bridge")}</strong><small>{uiText("把两个对话连起来，一个做决策，一个做执行。每次转发都由你审阅和批准。")}</small></span></motion.button>
  </div>}
  {mode==="WATCH"&&<p className="v7-sheet-note">{uiText("Pi 子任务会出现在派单的主对话下面，可以用搜索找到。")}</p>}{mode==="WATCH"&&<>
   <label className="v6-search"><Search size={16} aria-hidden/><input type="search" aria-label={uiText("搜索标题或项目")} placeholder={uiText("搜索标题或项目")} value={query} onChange={e=>setQuery(e.target.value)}/></label>
   <div className="v6-chips">{([["ALL",uiText("全部")],["CODEX","Codex"],["CLAUDE","Claude Code"]] as const).map(([id,label])=><button key={id} type="button" aria-pressed={kind===id} onClick={()=>setKind(id)}>{label}</button>)}</div>
   {error&&<div className="v6-watch-error"><p role="alert" className="v6-error">{error}</p><button type="button" disabled={busy||threads===null} onClick={()=>setReload(n=>n+1)}>{uiText("刷新列表")}</button></div>}
   <div className="v6-sheet-scroll">{threads===null?<p className="v6-muted" role="status">{uiText("正在读取电脑上的对话…")}</p>:list.length?<><small className="v6-section">{uiText("电脑上最近活跃")}</small><div className="v6-list">{list.map((t,i)=><motion.button key={t.id} type="button" className="v6-row" disabled={busy} onClick={()=>void watch(t)} whileTap={{scale:.97}} initial={{opacity:0,y:10}} animate={{opacity:1,y:0}} transition={{...spring,delay:Math.min(i,12)*.025}}><ConversationAvatar id={`w:${t.id}`} kind={isClaude(t.id)?"claude":"codex"}/><span className="v6-row-body"><span className="v6-row-top"><strong>{t.label}</strong>{t.recencyAt?<time>{when(t.recencyAt*1000)}</time>:null}</span><span className="v6-row-meta">{isClaude(t.id)?"Claude Code":"Codex"}{t.projectLabel||t.projectProvenance?` · ${t.projectLabel||folder(t.projectProvenance!)}`:""}</span></span></motion.button>)}</div></>:<p className="v6-muted">{uiText("没有找到。这里只列出电脑上还没监听的 Codex 和 Claude Code 对话。")}</p>}</div>
  </>}
  {mode==="BRIDGE"&&<form className="v6-bridge-form" onSubmit={e=>{e.preventDefault();void create();}}>
   <label>{uiText("名称")}<input autoFocus value={name} maxLength={100} disabled={busy} placeholder={uiText("例如：工具开发")} onChange={e=>setName(e.target.value)}/></label>
   <p className="v6-muted">{uiText("下一步选择控制端和执行端。创建后不会自动转发，转发前由你审阅和批准。")}</p>
   {error&&<p role="alert" className="v6-error">{error}</p>}
   <button type="submit" className="v6-primary" disabled={busy}>{busy?uiText("正在创建…"):uiText("下一步：选择两端")}</button>
  </form>}
  </motion.div></AnimatePresence>
 </motion.div></Dialog.Content></Dialog.Portal></Dialog.Root>;
}
