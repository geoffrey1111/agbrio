import {claudeTaskSummary} from "./ClaudeTaskStatus";
import {t as uiText,tc,useLanguage,getLanguage} from "../../i18n";
import {useEffect,useMemo,useRef,useState,type ReactNode} from "react";
import * as Dialog from "@radix-ui/react-dialog";
import {CircleCheck,ChevronLeft,Link2,MessageSquare,Search,X} from "lucide-react";
import {SwipeDeleteRow} from "./SwipeDeleteRow";
import {AnimatePresence,LayoutGroup,motion} from "motion/react";
import {avatarFor} from "./avatarPool";
import {messagePreview} from "./messagePreview";
import {bridgeReviewAttention,focusedBridgeActivity,type BridgeActivity} from "./bridgeActivity";
import type {CodexWatch,NotificationApi} from "./CodexNotifications";
import type {ExistingCodexThreadCandidate} from "../codex/types";
import type {WorkbenchItem} from "./models";

/** One list for everything the owner follows: Bridges and watched
 * conversations. Hosts stay the only routing and mutation authority; this
 * component reads their existing projections and calls their existing actions. */

type Tone="wait"|"run"|"done";
type Row={key:string;kind:"BRIDGE"|"WATCH";title:string;meta:string;preview:string;at:number;tone:Tone;state:string;unread:boolean;bridge?:WorkbenchItem;watch?:CodexWatch};
type Filter="ALL"|"RUN"|"WAIT"|"BRIDGE"|"WATCH";
type ActivityRows=Record<string,{row:BridgeActivity;receivedAt:number}>;

const isClaude=(id:string)=>id.startsWith("claude_code:");
const spring={type:"spring",stiffness:520,damping:32,mass:.7} as const;
/** Persistent DiceBear avatar plus a small provider badge. */
export function ConversationAvatar({id,kind}:{id:string;kind:"bridge"|"codex"|"claude"}){
 return <motion.span className="v6-avatar" aria-hidden initial={{scale:.6,rotate:-8}} animate={{scale:1,rotate:0}} transition={{type:"spring",stiffness:420,damping:16}}><img src={avatarFor(id)} alt="" draggable={false}/><i className={`v6-badge v6-${kind}`}>{kind==="bridge"?<Link2 size={10} strokeWidth={2.6}/>:kind==="claude"?"✳":"C"}</i></motion.span>;
}
const folder=(path:string)=>path.split(/[\\/]/).filter(Boolean).at(-1)??"";
const messageTime=(value:number|null|undefined)=>typeof value==="number"&&Number.isFinite(value)&&value>0?value:0;
const when=(value:number)=>{if(!value)return"";const minutes=(Date.now()-value)/60000;if(minutes<1)return uiText("刚刚");if(minutes<60)return tc("{0} 分钟",Math.floor(minutes));const date=new Date(value);return minutes<1440?new Intl.DateTimeFormat(getLanguage(),{hour:"2-digit",minute:"2-digit"}).format(date):new Intl.DateTimeFormat(getLanguage(),{month:"numeric",day:"numeric"}).format(date);};

export function watchTone(w:CodexWatch):[Tone,string]{
 if(!w.enabled)return["done",uiText("已暂停")];
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
function bridgeTone(row:BridgeActivity|undefined,age:number,item:WorkbenchItem):[Tone,string]{
 // Before the first activity read, the directory attention count still tells the truth.
 if(!row)return (item.attentionCount??0)>0?["wait",uiText("有新回复")]:["done",""];
 const focus=focusedBridgeActivity(row,age),unread=bridgeReviewAttention(row,age);
 if(focus&&["RUNNING","THINKING","GOAL_ACTIVE"].includes(focus.side.phase))return["run",focus.side.role==="DECISION"?uiText("控制端执行中"):uiText("执行端执行中")];
 if(focus?.side.phase==="ACTION_REQUIRED")return["wait",uiText("等你确认")];
 if(unread>0)return["wait",uiText("有新回复")];
 return["done",""];
}

export function ConversationHome({onOpenBoard,mode="ALL",bridges,activity,activityClock,watchApi,quotaPanel,addRequest,onOpenBridge,onOpenWatch,onCreateBridge,onDeleteBridge}:{
 mode?:"ALL"|"TODO";onOpenBoard?:()=>void;
 bridges:WorkbenchItem[];activity:ActivityRows;activityClock:number;watchApi?:NotificationApi;quotaPanel?:ReactNode;addRequest:number;
 onOpenBridge:(id:string)=>void;onOpenWatch:(watch:CodexWatch)=>void;onCreateBridge?:(name:string)=>Promise<void>;onDeleteBridge?:(item:WorkbenchItem)=>void;
}){
 useLanguage();
 const [hiddenWatches,setHiddenWatches]=useState<string[]>([]);
 const watches=useWatches(watchApi).filter(w=>!hiddenWatches.includes(w.threadId));
 const [undo,setUndo]=useState<CodexWatch|null>(null);
 useEffect(()=>{if(!undo)return;const timer=setTimeout(()=>setUndo(null),6000);return()=>clearTimeout(timer);},[undo]);
 // Swiping a watched conversation only stops Agbrio's tracking; the chat itself is untouched and can be restored.
 async function stopWatching(w:CodexWatch){if(!watchApi?.remove)return;setHiddenWatches(old=>[...old,w.threadId]);try{await watchApi.remove("WATCH",w.threadId,true);setUndo(w);}catch{setHiddenWatches(old=>old.filter(id=>id!==w.threadId));}}
 async function restoreWatch(w:CodexWatch){setUndo(null);try{await watchApi?.remove?.("WATCH",w.threadId,false);}finally{setHiddenWatches(old=>old.filter(id=>id!==w.threadId));}}
 const [query,setQuery]=useState(""),[filter,setFilter]=useState<Filter>("ALL");
 const [sheet,setSheet]=useState<"CHOOSE"|"WATCH"|"BRIDGE"|null>(null);
 const seenAdd=useRef(addRequest);
 useEffect(()=>{if(addRequest!==seenAdd.current){seenAdd.current=addRequest;setSheet("CHOOSE");}},[addRequest]);

 const rows=useMemo<Row[]>(()=>{
  const b=bridges.map<Row>(item=>{const entry=activity[item.id];const [tone,state]=bridgeTone(entry?.row,entry?Math.max(0,activityClock-entry.receivedAt):Infinity,item);
   return{key:`b:${item.id}`,kind:"BRIDGE",title:item.name,meta:`Bridge · ${item.sourceLabel||item.projectName||uiText("待绑定")}`,preview:"",at:messageTime(entry?.row.lastMessageAt??item.lastMessageAt),tone,state,unread:tone==="wait",bridge:item};});
  const w=watches.map<Row>(watch=>{const [tone,state]=watchTone(watch);
   return{key:`w:${watch.threadId}`,kind:"WATCH",title:watch.label,meta:`${isClaude(watch.threadId)?"Claude Code":"Codex"}${watch.cwd?` · ${folder(watch.cwd)}`:""}`,preview:messagePreview(watch.snapshot.text),at:messageTime(watch.lastMessageAt),tone,state,unread:false,watch};});
  const rank={run:0,wait:1,done:2};
  return[...b,...w].sort((x,y)=>rank[x.tone]-rank[y.tone]||y.at-x.at||(x.key<y.key?-1:x.key>y.key?1:0));
 },[bridges,activity,activityClock,watches]);
 const counts={run:rows.filter(r=>r.tone==="run").length,wait:rows.filter(r=>r.tone==="wait").length};
 const q=query.trim().toLocaleLowerCase();
 const todo=mode==="TODO";
 const visible=rows.filter(r=>(!todo||r.tone==="wait")&&(filter==="ALL"||filter==="RUN"&&r.tone==="run"||filter==="WAIT"&&r.tone==="wait"||filter===r.kind)&&(!q||`${r.title} ${r.meta}`.toLocaleLowerCase().includes(q)));
 const active=visible.filter(r=>r.tone!=="done"),rest=visible.filter(r=>r.tone==="done");
 const open=(r:Row)=>r.bridge?onOpenBridge(r.bridge.id):r.watch&&onOpenWatch(r.watch);
 // data-bridge-id lets useBridgeActivity read status only for rows on screen.
 const item=(r:Row,index:number)=>{const body=<motion.button type="button" whileTap={{scale:.97}} transition={spring} className="v6-row" data-tone={r.tone} data-bridge-id={r.bridge?.id} onClick={()=>open(r)}>
   <ConversationAvatar id={r.key} kind={r.kind==="BRIDGE"?"bridge":isClaude(r.watch!.threadId)?"claude":"codex"}/>
   <span className="v6-row-body"><span className="v6-row-top"><strong>{r.title}</strong>{r.at?<time>{when(r.at)}</time>:null}</span><span className="v6-row-meta">{r.meta}</span>{r.preview&&<span className="v6-row-preview">{r.preview}</span>}{r.state&&<motion.span key={r.state} className="v6-state" data-tone={r.tone} initial={{scale:.7,opacity:.4}} animate={{scale:1,opacity:1}} transition={spring}>{r.tone==="run"&&<i aria-hidden/>}{r.state}</motion.span>}</span>
   <AnimatePresence>{r.unread&&<motion.i className="v6-unread" aria-label={uiText("需要你处理")} initial={{scale:0}} animate={{scale:1}} exit={{scale:0}} transition={{type:"spring",stiffness:600,damping:14}}/>}</AnimatePresence>
  </motion.button>;
  const inner=r.bridge&&onDeleteBridge?<SwipeDeleteRow label={r.title} onDelete={()=>onDeleteBridge(r.bridge!)}>{body}</SwipeDeleteRow>:r.watch&&watchApi?.remove?<SwipeDeleteRow label={r.title} onDelete={()=>void stopWatching(r.watch!)}>{body}</SwipeDeleteRow>:body;
  return <motion.div key={r.key} layout="position" initial={{opacity:.35,y:14,scale:.98}} animate={{opacity:1,y:0,scale:1}} exit={{opacity:0,scale:.96,transition:{duration:.15}}} transition={{...spring,delay:Math.min(index,10)*.03}}>{inner}</motion.div>;};
 const chips:[Filter,string,number?][]=[["ALL",uiText("全部")],["RUN",uiText("执行中"),counts.run],["WAIT",uiText("等你"),counts.wait],["BRIDGE","Bridge"],["WATCH",uiText("监听")]];
 return <section className="v6-home" data-mode={mode} aria-label={todo?uiText("需要你处理"):uiText("对话")}>
  {!todo&&<><label className="v6-search"><Search size={16} aria-hidden/><input type="search" aria-label={uiText("搜索对话或项目")} placeholder={uiText("搜索对话或项目")} value={query} onChange={e=>setQuery(e.target.value)}/>{query&&<button type="button" aria-label={uiText("清除搜索")} onClick={()=>setQuery("")}><X size={16}/></button>}</label>
  <LayoutGroup id="v6-chips"><div className="v6-chips" role="group" aria-label={uiText("筛选")}>{chips.map(([id,label,n])=><motion.button key={id} type="button" whileTap={{scale:.92}} aria-pressed={filter===id} onClick={()=>setFilter(id)}>{filter===id&&<motion.span className="v6-chip-fill" layoutId="v6-chip-fill" transition={spring}/>}<span>{label}{n?<b>{n}</b>:null}</span></motion.button>)}</div></LayoutGroup></>}
  {todo&&visible.length>0&&<div className="v6-todo-stack">{visible.map((r,i)=>{const kind=r.kind==="BRIDGE"?(r.state===uiText("等你确认")?uiText("等你确认"):uiText("待转发")):r.watch?.snapshot.state==="FAILED"?uiText("执行失败"):uiText("等你确认");const tone=r.watch?.snapshot.state==="FAILED"?"danger":r.kind==="BRIDGE"?"accent":"wait";
   return <motion.button key={r.key} type="button" className="v6-todo-card" data-bridge-id={r.bridge?.id} onClick={()=>open(r)} whileTap={{scale:.97}} layout="position" initial={{opacity:.35,y:16,scale:.97}} animate={{opacity:1,y:0,scale:1}} transition={{...spring,delay:Math.min(i,8)*.04}}>
    <span className="v6-todo-kind" data-tone={tone}>{kind}</span>
    <span className="v6-todo-title"><ConversationAvatar id={r.key} kind={r.kind==="BRIDGE"?"bridge":isClaude(r.watch!.threadId)?"claude":"codex"}/><strong>{r.title}</strong>{r.at?<time>{when(r.at)}</time>:null}</span>
    <span className="v6-todo-detail">{r.preview||r.meta}</span>
   </motion.button>;})}</div>}
  {todo&&onOpenBoard&&<motion.button type="button" className="v6-todo-card v6-todo-board" onClick={onOpenBoard} whileTap={{scale:.97}} initial={{opacity:.35,y:16}} animate={{opacity:1,y:0}} transition={{...spring,delay:.12}}><span className="v6-todo-kind" data-tone="wait">{uiText("看板")}</span><span className="v6-todo-title"><strong>{uiText("打开看板审批")}</strong></span><span className="v6-todo-detail">{uiText("角色例会和立绘批复在看板里处理。")}</span></motion.button>}
  {!todo&&visible.length>0&&<div className="v6-list"><AnimatePresence>{[...active.map(item),...(rest.length?[<motion.small layout="position" key="section" className="v6-section">{active.length?uiText("其他"):uiText("最近")}</motion.small>]:[]),...rest.map((r,i)=>item(r,i+active.length))]}</AnimatePresence></div>}
  {todo&&!visible.length&&<motion.div className="v6-empty v6-all-clear" initial={{scale:.9,opacity:.4}} animate={{scale:1,opacity:1}} transition={{type:"spring",stiffness:300,damping:18}}><CircleCheck size={30} aria-hidden/><h2>{uiText("没有等你处理的事")}</h2><p>{uiText("有新回复、需要确认或执行失败时，会出现在这里。")}</p></motion.div>}
  {!todo&&!visible.length&&<div className="v6-empty">{rows.length?<p>{uiText("没有符合条件的对话")}</p>:<><MessageSquare size={30} aria-hidden/><h2>{uiText("还没有关注的对话")}</h2><p>{uiText("监听电脑上的 Codex 或 Claude Code 对话，或者把两个对话连成 Bridge。")}</p><button type="button" className="v6-primary" onClick={()=>setSheet("CHOOSE")}>{uiText("开始")}</button></>}</div>}
  <AnimatePresence>{undo&&<motion.div className="v6-toast" role="status" initial={{y:24,opacity:0}} animate={{y:0,opacity:1}} exit={{y:24,opacity:0}} transition={spring}>{uiText("已停止监听")}<button type="button" onClick={()=>void restoreWatch(undo)}>{uiText("撤销")}</button></motion.div>}</AnimatePresence>
  {!todo&&<AddSheet mode={sheet} setMode={setSheet} watchApi={watchApi} watched={watches} onCreateBridge={onCreateBridge} onWatched={w=>{setSheet(null);onOpenWatch(w);}}/>}
 </section>;
}

/** Reads the existing watch projection: cached first, then the live list. */
function useWatches(api?:NotificationApi){
 const [watches,setWatches]=useState<CodexWatch[]>(()=>api?.cachedWatches?.()??[]);
 useEffect(()=>{
  if(!api)return;let alive=true,timer:ReturnType<typeof setTimeout>;
  const poll=async()=>{if(document.visibilityState!=="hidden"){try{const next=await api.watches();if(alive)setWatches(next);}catch{/* keep the cached list */}}if(alive)timer=setTimeout(()=>void poll(),5000);};
  const unsubscribe=api.subscribe?.(()=>{const cached=api.cachedWatches?.();if(alive&&cached)setWatches(cached);});
  const wake=()=>{if(document.visibilityState!=="hidden"){clearTimeout(timer);void poll();}};
  document.addEventListener("visibilitychange",wake);void poll();
  return()=>{alive=false;clearTimeout(timer);unsubscribe?.();document.removeEventListener("visibilitychange",wake);};
 },[api]);
 return watches;
}

function AddSheet({mode,setMode,watchApi,watched,onCreateBridge,onWatched}:{mode:"CHOOSE"|"WATCH"|"BRIDGE"|null;setMode:(m:"CHOOSE"|"WATCH"|"BRIDGE"|null)=>void;watchApi?:NotificationApi;watched:CodexWatch[];onCreateBridge?:(name:string)=>Promise<void>;onWatched:(w:CodexWatch)=>void}){
 useLanguage();
 const [threads,setThreads]=useState<ExistingCodexThreadCandidate[]|null>(null),[query,setQuery]=useState(""),[kind,setKind]=useState<"ALL"|"CODEX"|"CLAUDE">("ALL");
 const [name,setName]=useState(""),[busy,setBusy]=useState(false),[error,setError]=useState<string|null>(null);
 useEffect(()=>{if(!mode){setQuery("");setKind("ALL");setError(null);setName("");}},[mode]);
 useEffect(()=>{
  if(mode!=="WATCH"||!watchApi)return;let alive=true;setThreads(null);setError(null);
  void (async()=>{try{await watchApi.connect();const next=await watchApi.threads();if(alive)setThreads(next.threads);}catch(e){if(alive){setThreads([]);setError(String(e).includes("ROUTER_READ_TIMEOUT")?uiText("读取超时，请稍后重试。"):uiText("暂时读不到电脑上的对话。请确认电脑上的 Router 正在运行。"));}}})();
  return()=>{alive=false;};
 },[mode,watchApi]);
 const enabled=new Set(watched.filter(w=>w.enabled).map(w=>w.threadId));
 const q=query.trim().toLocaleLowerCase();
 const list=(threads??[]).filter(t=>!enabled.has(t.id)&&(kind==="ALL"||(kind==="CLAUDE")===isClaude(t.id))&&(!q||`${t.label} ${t.projectProvenance??""} ${t.projectLabel??""}`.toLocaleLowerCase().includes(q)))
  .sort((a,b)=>(b.recencyAt??0)-(a.recencyAt??0));
 async function watch(t:ExistingCodexThreadCandidate){
  if(!watchApi||busy)return;setBusy(true);setError(null);
  try{await watchApi.enable(t.id);const next=await (watchApi.refreshWatches?.()??watchApi.watches());const w=next.find(x=>x.threadId===t.id);if(w)onWatched(w);else setMode(null);}
  catch(e){setError(String(e).includes("WATCH_ALREADY_IN_BRIDGE")?uiText("这条对话已在 Bridge 中，可直接从 Bridge 打开。"):uiText("没有开始监听，请重试。"));}
  finally{setBusy(false);}
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
  {mode==="WATCH"&&<>
   <label className="v6-search"><Search size={16} aria-hidden/><input type="search" aria-label={uiText("搜索标题或项目")} placeholder={uiText("搜索标题或项目")} value={query} onChange={e=>setQuery(e.target.value)}/></label>
   <div className="v6-chips">{([["ALL",uiText("全部")],["CODEX","Codex"],["CLAUDE","Claude Code"]] as const).map(([id,label])=><button key={id} type="button" aria-pressed={kind===id} onClick={()=>setKind(id)}>{label}</button>)}</div>
   {error&&<p role="alert" className="v6-error">{error}</p>}
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
