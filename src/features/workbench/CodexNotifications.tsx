import {SwipeDeleteRow} from "./SwipeDeleteRow";
import {useUndoRemoval} from "./useUndoRemoval";
import {useBackLayer} from "./navigationHistory";
import {createPortal} from "react-dom";
import {MarkdownMessage} from "../codex/MarkdownMessage";
import {messageMedia} from "../codex/messageMedia";
import {WebSessionLogout} from "../../mobile/WebLogin";
import { CodexThreadPicker } from "../codex/CodexThreadPicker";
import { useContext, useEffect, useLayoutEffect, useRef, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import type { ExistingCodexThreadCatalog } from "../codex/types";
import { NotificationDeliveryPanel, type DeliveryApi } from "./NotificationDeliveryPanel";
import {WatchChat} from "./WatchChat";
import {Bell,ChevronRight,Plus} from "lucide-react";
import {NativeSurfaceContext} from "./NativeSurfaceContext";
import type {WatchChatApi} from "./watchChatApi";

export type WatchSnapshot = { state:string; turnId:string|null; itemId:string|null; text:string };
export type CodexWatch = { threadId:string; label:string; cwd:string; enabled:boolean; generation:number; snapshot:WatchSnapshot; checkedAt:number; errorCode:string|null };
export type WatchEvent = { sequence:number; threadId:string; label:string; cwd:string; snapshot:WatchSnapshot; observedAt:number };
export type WatchFeed = { events:WatchEvent[]; nextCursor:number; hasMore:boolean; hiddenSequences?:number[] };
export interface NotificationApi { remove?(kind:"WATCH"|"EVENT",id:string,removed:boolean):Promise<unknown>; watches():Promise<CodexWatch[]>; connect():Promise<unknown>; threads():Promise<ExistingCodexThreadCatalog>; enable(id:string):Promise<unknown>; pause(id:string):Promise<unknown>; feed(after:number):Promise<WatchFeed>; event(sequence:number):Promise<WatchEvent>; webUrl():Promise<string|null>; delivery?:DeliveryApi; chat?:WatchChatApi; onOpen?:(callback:(sequence:number)=>void)=>Promise<()=>void> }

const states:Record<string,string>={IDLE:"尚无任务",RUNNING:"正在执行",ACTION_REQUIRED:"需要你确认或回答",RESULT_READY:"结果已到达",RESULT_PENDING:"结果尚未读到",FAILED:"执行失败",INTERRUPTED:"已中断",INCOMPLETE:"任务未完成（执行中或已中断）",UNKNOWN:"状态未确定"};
const time=(value:number)=>new Date(value).toLocaleString();
const watchState=(w:CodexWatch)=>!w.enabled?"已暂停":w.errorCode?"暂时无法读取":Date.now()-w.checkedAt>30_000?"最近未成功检查":states[w.snapshot.state]??w.snapshot.state;
const notificationError=(error:unknown)=>String(error).includes("WATCH_EVENT_NOT_FOUND")?"这条通知已清理或不可用。请查看最近通知或对话当前内容。":String(error);
type Removable={kind:"WATCH";item:CodexWatch}|{kind:"EVENT";item:WatchEvent};
type View="RECENT"|"WATCHES"|"SETTINGS";
export function CodexNotifications({api,standalone=false,workbenchPage,onCountChange,onDetailChange}:{api:NotificationApi;standalone?:boolean;onCountChange?:(count:number)=>void;onDetailChange?:(active:boolean)=>void;workbenchPage?:{active:boolean;open:()=>void}}) {
 const native=useContext(NativeSurfaceContext);
 const tabs:readonly (readonly [View,string])[]=native?[["RECENT","最近"],["WATCHES","监听"]]:[["RECENT","最近通知"],["WATCHES","监听对话"],["SETTINGS","通知设置"]];
 const pageNavigation=useRef(workbenchPage);pageNavigation.current=workbenchPage;
 const [open,setOpen]=useState(standalone),[view,setView]=useState<View>("RECENT");
 const [watches,setWatches]=useState<CodexWatch[]>([]),[events,setEvents]=useState<WatchEvent[]>([]);
 const [hiddenSequences,setHiddenSequences]=useState<number[]>([]);
 const [catalog,setCatalog]=useState<ExistingCodexThreadCatalog|null>(null),[selected,setSelected]=useState("");
 useBackLayer(Boolean(catalog),()=>setCatalog(null));
 const [error,setError]=useState<string|null>(null),[busy,setBusy]=useState(false);
 const [detail,setDetail]=useState<WatchEvent|null>(null),[current,setCurrent]=useState<CodexWatch|null>(null),[linkNotice,setLinkNotice]=useState<string|null>(null);
 useEffect(()=>{onDetailChange?.(Boolean(detail||current));},[Boolean(detail||current),onDetailChange]);
 const removal=useUndoRemoval<Removable>(row=>row.kind=== "WATCH"?`watch:${row.item.threadId}`:`event:${row.item.sequence}`,async(row,removed)=>{if(!api.remove)throw Error("WATCH_REMOVAL_UNAVAILABLE");await api.remove(row.kind,row.kind==="WATCH"?row.item.threadId:String(row.item.sequence),removed);await refreshWatches();},message=>setLinkNotice(message));
 const shown=removal.project([...watches.map(item=>({kind:"WATCH" as const,item})),...events.filter(e=>!hiddenSequences.includes(e.sequence)).map(item=>({kind:"EVENT" as const,item}))]);
 const visibleWatches=shown.filter((row):row is {kind:"WATCH";item:CodexWatch}=>row.kind==="WATCH").map(row=>row.item);
 const visibleEvents=shown.filter((row):row is {kind:"EVENT";item:WatchEvent}=>row.kind==="EVENT").map(row=>row.item).sort((a,b)=>a.sequence-b.sequence);
 useEffect(()=>{onCountChange?.(visibleEvents.length);},[visibleEvents.length,onCountChange]);
 const [webUrl,setWebUrl]=useState<string|null>(null);
 const returnKey=useRef<string|null>(null),returnScroll=useRef<{element:HTMLElement|null;top:number}>({element:null,top:0}),restoreList=useRef(false);
 const cursor=useRef(0),epoch=useRef(0),mounted=useRef(true),detailElement=useRef<HTMLElement|null>(null);
 useEffect(()=>{if(detail||current){detailElement.current?.focus();detailElement.current?.scrollIntoView?.({block:"start"});}},[detail,current]);
 useEffect(()=>{let active=true;void api.webUrl().then(url=>{if(active)setWebUrl(url);}).catch(()=>{});return()=>{active=false;};},[api]);
 useEffect(()=>{mounted.current=true;return()=>{mounted.current=false;epoch.current++;};},[]);
 useEffect(()=>{
  let active=true,request=0,dispose:(()=>void)|undefined;
  function show(sequence:number){if(!active)return;pageNavigation.current?.open();const ticket=++request;setOpen(true);setView("RECENT");setDetail(null);setCurrent(null);setLinkNotice(null);if(sequence>0)void api.event(sequence).then(e=>{if(active&&ticket===request){setDetail(e);setCurrent(null);}}).catch(e=>{if(active&&ticket===request)setLinkNotice(notificationError(e));});}
  const value=new URL(location.href).searchParams.get("event");if(value&&/^\d+$/.test(value)&&Number.isSafeInteger(Number(value)))show(Number(value));
  void api.onOpen?.(show).then(fn=>{if(active)dispose=fn;else fn();}).catch(()=>{if(active)setError("通知点击入口暂时不可用；仍可从通知列表查看内容。");});
  return()=>{active=false;dispose?.();};
 },[api]);
 useEffect(()=>{
  let active=true,timer:ReturnType<typeof setTimeout>;
  async function poll(){try{const [next,feed]=await Promise.all([open?api.watches():Promise.resolve(null),api.feed(cursor.current)]);if(!active)return;if(next)setWatches(next);setHiddenSequences(feed.hiddenSequences??[]);setEvents(old=>[...old,...feed.events.filter(e=>!old.some(o=>o.sequence===e.sequence))].slice(-20));cursor.current=feed.nextCursor;setError(null);timer=setTimeout(()=>void poll(),feed.hasMore?100:5000);}catch(e){if(active){setError(String(e));timer=setTimeout(()=>void poll(),10000);}}}
  void poll();return()=>{active=false;clearTimeout(timer);};
 },[api,open]);
 async function act(work:()=>Promise<void>){if(busy)return;setBusy(true);setError(null);const mark=++epoch.current;try{await work();}catch(e){if(mounted.current&&mark===epoch.current)setLinkNotice(notificationError(e));}finally{if(mounted.current&&mark===epoch.current)setBusy(false);}}
 async function refreshWatches(){const next=await api.watches();if(mounted.current)setWatches(next);}
 function changeView(next:View){setView(next);setDetail(null);setCurrent(null);setLinkNotice(null);}
 function rememberList(button:HTMLButtonElement,key:string){returnKey.current=key;let element=button.closest<HTMLElement>(".v3-notifications-body");if(element&&getComputedStyle(element).overflowY==="visible")element=button.closest<HTMLElement>(".r2-content")??element;returnScroll.current={element,top:element?.scrollTop??window.scrollY};}
 useBackLayer(Boolean(detail||current),()=>{restoreList.current=true;setDetail(null);setCurrent(null);},"event");
 function closeDetail(){restoreList.current=true;setDetail(null);setCurrent(null);}
 useLayoutEffect(()=>{if(!restoreList.current||detail||current)return;restoreList.current=false;const button=Array.from(document.querySelectorAll<HTMLElement>("[data-notification-key]")).find(n=>n.dataset.notificationKey===returnKey.current);(button??document.getElementById(`notification-tab-${view}`))?.focus({preventScroll:true});const {element,top}=returnScroll.current;if(element)element.scrollTop=top;else window.scrollTo({top,behavior:"instant"});},[detail,current,view]);
 const original=detail??current;
 const content=<div className="v3-notifications-body v4-notification-center">
  {error&&<p role="alert">{error}</p>}
  {linkNotice&&<p role="status">{linkNotice}<button type="button" onClick={()=>setLinkNotice(null)}>知道了</button></p>}
  {original&&api.chat?<WatchChat key={`${original.threadId}:${detail?.sequence??"current"}`} original={original} api={api.chat} onBack={closeDetail} backLabel={current?"监听对话":"最近通知"}/>:original?<>
   <button type="button" className="v4-back-link" onClick={closeDetail}>‹ {current?"返回监听对话":"返回最近通知"}</button>
   <section ref={detailElement} tabIndex={-1} aria-label="完整通知内容" className="v4-notification-original">
    <header><p className="v4-meta">{states[original.snapshot.state]??original.snapshot.state}{detail?` · ${time(detail.observedAt)}`:""}</p><h2>{original.label}</h2></header>
    <MarkdownMessage text={original.snapshot.text||"暂时没有可读取的公开消息。"} media={messageMedia({kind:"WATCH",threadId:original.threadId,sequence:detail?.sequence??null})}/>
    <details className="v4-secondary-details"><summary>对话信息</summary><p>{original.cwd}</p><code>{original.threadId}</code></details>
    <button type="button" onClick={closeDetail}>关闭内容</button>
   </section>
  </>:<>
   <div className="v4-notification-tabs" role="tablist" aria-label="通知中心栏目">
    {tabs.map(([id,title])=><button type="button" key={id} role="tab" id={`notification-tab-${id}`} aria-controls="notification-panel" tabIndex={view===id?0:-1} aria-selected={view===id} disabled={busy} onClick={()=>changeView(id)} onKeyDown={event=>{const ids=tabs.map(([id])=>id);if(["ArrowLeft","ArrowRight","Home","End"].includes(event.key)){event.preventDefault();const index=ids.indexOf(id);const next=ids[event.key==="Home"?0:event.key==="End"?ids.length-1:(index+(event.key==="ArrowRight"?1:ids.length-1))%ids.length];changeView(next);(event.currentTarget.parentElement?.querySelector(`#notification-tab-${next}`) as HTMLButtonElement|null)?.focus();}}}>{title}</button>)}
   </div>
   {view==="RECENT"&&<section id="notification-panel" role="tabpanel" aria-labelledby="notification-tab-RECENT" aria-label="Codex 通知列表">
    {!native&&<p className="v4-meta">只保留最近 20 条，更早的自动清理。</p>}
    {visibleEvents.length===0?<div className="v4-reader-empty"><h2>有新结果时，会出现在这里</h2><p>先在「监听对话」选择你要关注的 Codex 对话。</p></div>:[...visibleEvents].reverse().map(e=><SwipeDeleteRow className="agbrio-card-row" key={e.sequence} label={`通知 #${e.sequence}`} disabled={!api.remove} onDelete={()=>removal.remove({kind:"EVENT",item:e})}><article className="v4-notification-item">
     <header><strong>{e.label}</strong><span className="v4-event-state" data-state={e.snapshot.state}>{states[e.snapshot.state]??e.snapshot.state}</span></header>
     <p className="v4-meta">{time(e.observedAt)}</p><p className="v4-notification-preview">{e.snapshot.text||"本次通知记录了状态变化。"}</p>
     <button type="button" data-notification-key={`event-${e.sequence}`} disabled={busy} onClick={click=>{rememberList(click.currentTarget,`event-${e.sequence}`);void act(async()=>{const next=await api.event(e.sequence);if(mounted.current){setDetail(next);setCurrent(null);}});}} aria-label={`查看完整通知 #${e.sequence}`}>{native?<><Bell size={18}/><span>查看</span><ChevronRight size={16}/></>:`查看完整通知 #${e.sequence}`}</button>
    </article></SwipeDeleteRow>)}
   </section>}
   {view==="WATCHES"&&<section id="notification-panel" role="tabpanel" aria-labelledby="notification-tab-WATCHES" aria-label="已监听对话">
    <div className="v4-section-heading"><h2>已监听对话</h2><button type="button" className="v3-primary" aria-label="选择监听对话" disabled={busy} onClick={()=>void act(async()=>{await api.connect();const next=await api.threads();if(mounted.current)setCatalog(next);})}>{native?<Plus size={20}/>:busy&&!catalog?"正在扫描本地对话…":"选择监听对话"}</button></div>
    {catalog&&<section aria-label="添加监听" className="v4-watch-picker"><CodexThreadPicker threads={catalog.threads} complete={catalog.complete} value={selected} onChange={setSelected} ariaLabel="本地 Codex 对话" searchLabel="搜索对话或项目" disabled={busy}/><div className="v3-notification-actions"><button type="button" onClick={()=>setCatalog(null)} disabled={busy}>取消</button><button type="button" className="v3-primary" disabled={busy||!catalog.threads.some(t=>t.id===selected)} onClick={()=>void act(async()=>{await api.enable(selected);await refreshWatches();if(mounted.current){setSelected("");setCatalog(null);}})}>开始监听所选对话</button></div></section>}
    {visibleWatches.length===0?<div className="v4-reader-empty"><p>选择后会展示当前内容；后续变化进入最近通知。</p></div>:visibleWatches.map(w=><SwipeDeleteRow className="agbrio-card-row" key={w.threadId} label={`监听 ${w.label}`} disabled={!api.remove} onDelete={()=>removal.remove({kind:"WATCH",item:w})}><article className="v4-watch-item"><header><strong>{w.label}</strong><span>{watchState(w)}</span></header><p className="v4-meta">最后成功检查：{time(w.checkedAt)}</p><div className="v3-notification-actions"><button type="button" data-notification-key={`watch-${w.threadId}`} onClick={click=>{rememberList(click.currentTarget,`watch-${w.threadId}`);setDetail(null);setCurrent(w);}}>查看当前内容</button><button type="button" disabled={busy} onClick={()=>void act(async()=>{if(w.enabled)await api.pause(w.threadId);else await api.enable(w.threadId);await refreshWatches();})}>{w.enabled?"暂停监听":"恢复监听"}</button></div><details className="v4-secondary-details"><summary>对话信息</summary><p>{w.cwd}</p><code>{w.threadId}</code></details></article></SwipeDeleteRow>)}
   </section>}
   {view==="SETTINGS"&&<section id="notification-panel" role="tabpanel" aria-labelledby="notification-tab-SETTINGS" aria-label="通知设置内容">
    <WebSessionLogout/>
    {api.delivery&&<NotificationDeliveryPanel api={api.delivery}/>}
    {!standalone&&webUrl&&<a className="v4-web-entry" href={webUrl} target="_blank" rel="noreferrer">在网页打开通知中心</a>}
    <details className="v4-secondary-details"><summary>给 Dot 与其他助手的读取方式</summary>
     <p>让助手定期检查此页。仅打开页面不保证自动唤醒它。</p>{webUrl&&<code>{webUrl}</code>}
     <p>接口可按游标读取新增通知，再读取完整原文。</p><code>GET /v1/mobile/codex-watches/events?after=0&amp;waitSeconds=20</code>
     <p>保存 nextCursor；hasMore 为 true 时继续读取。完整内容：GET /v1/mobile/codex-watches/events/通知序号。网页与接口均使用现有登录。</p>
     <p>Host 停止时暂停检查。更早通知会自动清理，暂停期间的短暂变化不保证全部捕获。</p>
    </details>
   </section>}
  </>}
 {removal.undoItem&&<div className="r2-toast" role="status">已移除{removal.undoItem.kind==="WATCH"?"监听":"通知"}<button type="button" onClick={removal.undo}>撤销</button><button type="button" aria-label="关闭提示" onClick={removal.dismiss}>×</button></div>}
 </div>;
 useEffect(()=>{if(workbenchPage)setOpen(workbenchPage.active);},[workbenchPage?.active]);
 if(workbenchPage){const target=document.querySelector(".v3-workbench-main");return <><button type="button" className="v3-notification-entry" onClick={()=>{setOpen(true);workbenchPage.open();}}>通知中心{events.length>0?<span className="v4-notification-count">{events.length}</span>:null}</button>{workbenchPage.active&&target&&createPortal(<section className="v5-notifications-page" aria-label="通知中心"><header><h1>通知</h1></header>{content}</section>,target)}</>;}
 if(standalone)return <main className="v3-notifications-page v4-notifications-page"><header><div><p className="v4-meta">AI Work Router</p><h1>通知中心</h1></div><a href="/mobile">返回工作区</a></header>{content}</main>;
 return <><button type="button" className="v3-notification-entry" onClick={()=>setOpen(true)}>通知中心{events.length>0?<span className="v4-notification-count">{events.length}</span>:null}</button><Dialog.Root open={open} onOpenChange={value=>{if(!busy)setOpen(value);}}><Dialog.Portal><Dialog.Overlay className="v3-role-dialog-overlay"/><Dialog.Content className="v3-role-dialog v3-notifications-dialog" aria-describedby={undefined}><header><Dialog.Title>通知中心</Dialog.Title><Dialog.Close asChild><button type="button" disabled={busy}>关闭</button></Dialog.Close></header>{content}</Dialog.Content></Dialog.Portal></Dialog.Root></>;
}

/** Advanced assistant instructions belong under Settings, not a third notification destination. */
export function NotificationAssistantSettings({api}:{api:NotificationApi}){const[url,setUrl]=useState<string|null>(null);useEffect(()=>{let active=true;void api.webUrl().then(v=>{if(active)setUrl(v);}).catch(()=>{});return()=>{active=false;};},[api]);return <details className="r2-assistant-settings"><summary>Dot 与其他助手</summary><p>让助手定期检查通知页，读取新通知和完整原文。仅打开网页不保证自动唤醒。</p>{url&&<a href={url} target="_blank" rel="noreferrer noopener">{url}</a>}<code>GET /v1/mobile/codex-watches/events?after=0&amp;waitSeconds=20</code><p>保存 nextCursor；完整内容可按通知序号读取。网页与接口使用现有登录。</p></details>;}
