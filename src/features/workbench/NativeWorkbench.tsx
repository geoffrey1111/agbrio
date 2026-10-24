import {PwaInstallGuide} from "./ConnectionGuides";
import {BoardView,BoardSourceSettings} from "../character-board/BoardSource";
import {bridgeReviewAttention} from "./bridgeActivity";
import {PwaUpdateSettings} from '../../mobile/PwaUpdateNotice';
import {useBridgeActivity} from "./useBridgeActivity";
import {BridgeActivityStrip} from "./BridgeActivityStrip";
import {useAppMotion} from "./useAppMotion";
import {useSurfaceTransition} from "./InteractionMotion";
import {LanguagePicker} from "../../i18n/LanguagePicker";
import {t as uiText,tc,useLanguage,getLanguage} from "../../i18n";
import {useVisibleViewport,PHONE_LAYOUT_REVISION} from "./mobileViewport";
import {displayDiagnostics} from "./displayDiagnostics";
import {useWorkbenchHistory,useBackLayer} from "./navigationHistory";
import {useEffect, useLayoutEffect, useMemo, useRef, useState} from "react";
import * as Dialog from "@radix-ui/react-dialog";
import {MotionConfig, motion, useReducedMotion} from "motion/react";
import {Bell, Bot, LayoutGrid, ListChecks, MessageSquare, ChevronLeft, ChevronRight, CircleCheck, Download, Copy, Ellipsis, Info, Link2, Monitor, Pencil, Plus, Search, Settings, Smartphone, Trash2, Undo2, X} from "lucide-react";
import {version} from "../../../package.json";
const productVersion=(import.meta as ImportMeta & {env?:{VITE_AGBRIO_APP_VERSION?:string}}).env?.VITE_AGBRIO_APP_VERSION||version;
import type {UnifiedWorkbenchProps, WorkbenchSurface} from "./UnifiedWorkbench";
import type {WorkbenchItem} from "./models";
import {NewBridgeForm} from "./BridgeDirectory";
import {PanelDivider} from "./PanelDivider";
import {NativeSurfaceContext} from "./NativeSurfaceContext";
import {CopyAction,NativeCopyProvider} from "./NativeReaderActions";
import {MediaPreviewProvider} from "../codex/MediaPreview";
import {SwipeDeleteRow} from "./SwipeDeleteRow";
import {useUndoRemoval} from "./useUndoRemoval";
import {BrandMark} from "./BrandMark";
import {ConversationHome} from "./ConversationHome";
import {WatchChat} from "./WatchChat";
import type {CodexWatch} from "./CodexNotifications";

const destinations = [{id:"BRIDGES", get label(){return uiText("对话");}, Icon:MessageSquare}, {id:"BOARD", get label(){return uiText("看板");}, Icon:LayoutGrid}, {id:"NOTIFICATIONS", get label(){return uiText("待办");}, Icon:ListChecks}, {id:"SETTINGS", get label(){return uiText("设置");}, Icon:Settings}] as const;
const date = (value?:number|null) => value ? new Intl.DateTimeFormat(getLanguage(), {month:"numeric",day:"numeric",hour:"2-digit",minute:"2-digit"}).format(value) : "";

/** Native R2 composition. Hosts remain the sole routing and mutation authority. */
export function NativeWorkbench(props:UnifiedWorkbenchProps) {
 const keyboardMotion=useAppMotion();
 useLanguage();
  const [localSurface, setLocalSurface] = useState<WorkbenchSurface>("BRIDGES");
  const surface = props.surface ?? localSurface;
  const navigate = props.onSurfaceChange ?? setLocalSurface;
  useVisibleViewport();
  useWorkbenchHistory(surface,props.selectedWorkstreamId,navigate,props.onSelectWorkstream);
  const [query,setQuery] = useState("");
  const [limit,setLimit]=useState(20);
  useEffect(()=>setLimit(20),[query]);
  const [addRequest,setAddRequest] = useState(0);
  const [openWatch,setOpenWatch] = useState<CodexWatch|null>(null);
  const [menu,setMenu] = useState(false);
  const [copyText,setCopyText]=useState<string|null>(null);
  const [renaming,setRenaming]=useState<WorkbenchItem|null>(null),[renameValue,setRenameValue]=useState("");
  const [width,setWidth] = useState(()=>{try{return Math.min(390,Math.max(240,Number(localStorage.getItem("aiwr.r2.master-width"))||290));}catch{return 290;}});
  const [detail,setDetail] = useState<"INSTALL"|"DEVICES"|"NOTIFICATIONS"|"ABOUT"|"ASSISTANT"|"UPDATE"|"BOARD_SOURCE"|null>(null);
  const [displayReport,setDisplayReport]=useState("");
  const [displayCopied,setDisplayCopied]=useState(false);
  const [actionBusy,setActionBusy]=useState(false),[actionError,setActionError]=useState("");
  const pendingAction=useRef(false);
  useBackLayer(Boolean(detail),()=>setDetail(null));
  useBackLayer(menu,()=>setMenu(false));
  useBackLayer(Boolean(renaming),()=>setRenaming(null));
  useBackLayer(Boolean(openWatch),()=>setOpenWatch(null));
  const reduced = useReducedMotion();
  const content = useRef<HTMLElement>(null);
  const scrolls = useRef<Record<string,number>>({});
  const [online,setOnline] = useState(()=>navigator.onLine);
  const active = surface === "BOARD" ? "BOARD" : surface === "NOTIFICATIONS" ? "NOTIFICATIONS" : ["SETTINGS","RUNTIME","RECYCLE","INSTALL"].includes(surface) ? "SETTINGS" : "BRIDGES";
  const removal=useUndoRemoval<WorkbenchItem>(item=>item.id,async(item,removed)=>{await props.onLifecycleChange?.(item.id,removed?"TRASHED":"ACTIVE");},setActionError);
  const projected=removal.project(props.items.filter(item=>item.lifecycle==="ACTIVE"));
  function removeBridge(item:WorkbenchItem){removal.remove(item);if(inReader)navigate("BRIDGES");}
  const selected = props.items.find(item=>item.id===props.selectedWorkstreamId);
  const inReader = surface === "WORKSPACE" && Boolean(selected);
  useEffect(()=>{if(inReader&&selected?.lifecycle!=="ACTIVE")navigate("BRIDGES");},[inReader,selected?.lifecycle,navigate]);
  const items = useMemo(()=>projected.sort((a,b)=>Number(Boolean(b.pinned))-Number(Boolean(a.pinned))||(b.updatedAt??0)-(a.updatedAt??0)),[props.items,projected]);
  const visible = items.filter(item=>`${item.name} ${item.projectName??""}`.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()));
  const watchChat = surface==="BRIDGES"&&openWatch&&props.watchApi?.chat ? props.watchApi.chat : null;
  useEffect(()=>{if(surface!=="BRIDGES")setOpenWatch(null);},[surface]);
  const trash = props.items.filter(item=>item.lifecycle==="TRASHED"||removal.isRemoved(item));
  const scrollKey = `${surface}:${inReader?props.selectedWorkstreamId??"":""}`;
  useLayoutEffect(()=>{
    const element=content.current;if(!element)return;
    element.scrollTop=scrolls.current[scrollKey]??0;
    const track=()=>{scrolls.current[scrollKey]=element.scrollTop;};
    element.addEventListener("scroll",track,{passive:true});return()=>element.removeEventListener("scroll",track);
  },[scrollKey]);
  useEffect(()=>{

    const up=()=>setOnline(true),down=()=>setOnline(false);
    window.addEventListener("online",up);window.addEventListener("offline",down);
    return()=>{window.removeEventListener("online",up);window.removeEventListener("offline",down);};
  },[]);
  useEffect(()=>{setMenu(false);setDetail(null);},[surface,props.selectedWorkstreamId]);
  const nav = (phone:boolean) => <nav className={phone?"r2-tabbar":"r2-destinations"} aria-label={phone?uiText("主导航"):uiText("桌面导航")}>{destinations.map(({id,label,Icon})=><button key={id} type="button" aria-current={active===id?"page":undefined} onClick={()=>{setDetail(null);navigate(id);}}>{active===id&&<motion.span className="r2-nav-selection" layoutId={phone?"phone-destination":"desktop-destination"} transition={reduced||keyboardMotion?{duration:0}:{type:"spring",stiffness:560,damping:42}}/>}<Icon size={phone?23:20} aria-hidden/><span>{uiText(label)}</span>{id==="BRIDGES"&&hasBridgeAttention&&<i className="r2-nav-bridge-dot" aria-label={uiText("有新回复待审阅")}/>}{id==="NOTIFICATIONS"&&Boolean(props.notificationCount)&&<b className="r2-nav-badge" aria-label={tc("{0} 条通知", props.notificationCount??0)}>{Math.min(20,props.notificationCount!)}</b>}</button>)}</nav>;
  const listed=visible.slice(0,limit);
  const activity=useBridgeActivity(items.slice(0,40).map(i=>i.id),props.bridgeActivityApi,surface==="BRIDGES"||surface==="NOTIFICATIONS"||inReader);
  const bridgeUnread=(item:WorkbenchItem)=>bridgeReviewAttention(activity.rows[item.id]?.row,activity.rows[item.id]?Math.max(0,Date.now()-activity.rows[item.id].receivedAt):Infinity);
  const hasBridgeAttention=items.some(i=>bridgeUnread(i)>0);
  const rows = (compact=false) => <div className={`r2-bridge-group${compact?" r2-compact-list":""}`}>
    {(compact&&selected&&selected.lifecycle==="ACTIVE"&&!listed.some(item=>item.id===selected.id)?[selected,...listed]:listed).map(item=><SwipeDeleteRow key={item.id} label={item.name} disabled={!props.onLifecycleChange} onDelete={()=>removeBridge(item)}><div className="r2-bridge-row" data-bridge-id={item.id} data-selected={inReader&&selected?.id===item.id}>
      <button type="button" className="r2-bridge-open" onClick={()=>{props.onSelectWorkstream(item.id);navigate("WORKSPACE");}} aria-current={inReader&&selected?.id===item.id?"page":undefined}>
        <span className="r2-context-icon"><Link2 size={22}/></span><span className="r2-row-content"><span className="r2-row-title"><strong title={item.name}>{item.name}</strong>{Boolean(bridgeUnread(item))&&<span className="r2-bridge-new-reply" aria-label={uiText("有新回复待审阅")}><i aria-hidden/></span>}<time>{date(item.updatedAt)}</time></span><span className="r2-row-state"><span>{item.sourceLabel||item.projectName||uiText("待绑定")}</span></span>{activity.rows[item.id]?.row.sides.length>0&&<BridgeActivityStrip activity={activity.rows[item.id].row} age={Math.max(0,activity.clock-activity.rows[item.id].receivedAt)}/>}</span><ChevronRight size={16} className="r2-chevron"/>
      </button>
    </div></SwipeDeleteRow>)}
  </div>;
  async function changeLifecycle(item:WorkbenchItem,lifecycle:"ACTIVE"|"TRASHED"){
    if(lifecycle==="TRASHED")removeBridge(item);else removal.restore(item);
  }
  const title = detail==="INSTALL"?uiText("添加到手机主屏幕"):detail==="BOARD_SOURCE"?uiText("看板来源"):detail==="UPDATE"?uiText("检查更新"):detail==="ASSISTANT"?uiText("AI 助手"):detail==="ABOUT"?uiText("关于 Router"):detail==="DEVICES"?uiText("设备"):detail==="NOTIFICATIONS"?uiText("消息通知") : surface==="BOARD" ? uiText("看板") : surface==="BRIDGES" ? uiText("对话") : surface==="NEW_BRIDGE" ? uiText("新建 Bridge") : surface==="NOTIFICATIONS" ? uiText("待办") : surface==="SETTINGS" ? uiText("设置") : surface==="RUNTIME" ? uiText("连接") : surface==="RECYCLE" ? uiText("已删除的 Bridge") : selected?.name??"Bridge";
  const topLevel = ["BOARD","BRIDGES","NOTIFICATIONS","SETTINGS"].includes(surface)&&!detail&&!(surface==="NOTIFICATIONS"&&props.notificationDetail)&&!watchChat;
  useSurfaceTransition(content,`${scrollKey}:${detail??""}:${Boolean(props.notificationDetail)}:${openWatch?.threadId??""}`,!topLevel,keyboardMotion);
  const back = ()=>{if(detail)setDetail(null);else navigate(inReader?"BRIDGES":surface==="RECYCLE"||surface==="RUNTIME"?"SETTINGS":"BRIDGES");};
  return <NativeSurfaceContext.Provider value><NativeCopyProvider register={setCopyText}><MediaPreviewProvider><MotionConfig reducedMotion={keyboardMotion?"always":"user"}><main className={`unified-workbench r2-router v3-surface-${surface.toLowerCase()}`} data-detail={!topLevel} aria-label={uiText("Agbrio 统一工作台")} style={{"--r2-master-width":`${width}px`} as React.CSSProperties}>
    <aside className="r2-rail"><div className="r2-brand"><BrandMark/>Agbrio</div>{nav(false)}<small className="r2-rail-caption">Bridge</small><div className="r2-rail-bridges">{items.map(item=><button key={item.id} type="button" aria-current={inReader&&item.id===selected?.id?"page":undefined} onClick={()=>{props.onSelectWorkstream(item.id);navigate("WORKSPACE");}}>{item.name}{bridgeUnread(item)>0&&<i className="r2-rail-reply-dot" aria-label={uiText("有新回复待审阅")}/>}</button>)}</div></aside>
    <section className="r2-workspace"><header className="r2-header" hidden={surface==="NOTIFICATIONS"&&props.notificationDetail||Boolean(watchChat)}>
      {!topLevel&&<button type="button" className="r2-icon" aria-label={uiText("返回")} title={uiText("返回")} onClick={back}><ChevronLeft size={20}/></button>}<h1>{title}</h1>
      {surface==="BRIDGES"&&<button type="button" className="r2-icon r2-accent" aria-label={uiText("监听对话或新建 Bridge")} onClick={()=>setAddRequest(v=>v+1)}><Plus size={20}/></button>}
      {inReader&&copyText&&<CopyAction text={copyText} onError={()=>setActionError(uiText("复制未成功，可选择正文复制。"))}/>}
      {inReader&&<Dialog.Root open={menu} onOpenChange={setMenu}><Dialog.Trigger asChild><button type="button" className="r2-icon" aria-label={uiText("Bridge 操作")}><Ellipsis size={20}/></button></Dialog.Trigger><Dialog.Portal><Dialog.Overlay className="r2-menu-overlay"/><Dialog.Content className="r2-menu" aria-describedby={undefined}><Dialog.Title className="r2-sr-only">{uiText("Bridge 操作")}</Dialog.Title><button type="button" onClick={()=>{setMenu(false);props.onManageBindings?.();}}><Link2 size={18}/>{uiText("更换两端")}</button>{props.onRenameBridge&&selected&&<button type="button" onClick={()=>{setMenu(false);setRenameValue(selected.name);setRenaming(selected);setActionError("");}}><Pencil size={18}/>{uiText("重命名")}</button>}{props.onPinChange&&selected&&<button type="button" onClick={()=>{props.onPinChange?.(selected.id,!selected.pinned);setMenu(false);}}><CircleCheck size={18}/>{selected.pinned?uiText("取消置顶"):uiText("置顶")}</button>}{props.onLifecycleChange&&selected&&<button type="button" className="r2-danger" onClick={()=>{setMenu(false);removeBridge(selected);}}><Trash2 size={18}/>{uiText("删除 Bridge")}</button>}<Dialog.Close asChild><button type="button"><X size={18}/>{uiText("关闭")}</button></Dialog.Close></Dialog.Content></Dialog.Portal></Dialog.Root>}
    </header>
    {!online&&<p className="r2-offline" role="status">{uiText("连接中断 · 等待恢复")}</p>}
    <div className={`r2-stage${inReader?" r2-split":""}`}>
      {/* The sidebar already lists every Bridge; the reader uses the full width. */}
      <section ref={content} className="v3-workbench-main r2-content">
        {props.criticalNotice}{actionError&&<p role="alert">{uiText(actionError)}</p>}
        <section hidden={surface!=="BOARD"} className="character-board-mount"><BoardView active={surface==="BOARD"}/></section>
        <div hidden={!inReader} className="r2-reader-mount">{props.bridgePanel}</div>
        {watchChat&&openWatch&&<section className="r2-watch-mount"><WatchChat key={openWatch.threadId} original={openWatch} api={watchChat} onBack={()=>setOpenWatch(null)} backLabel={uiText("对话")} controls={props.watchApi?{enabled:openWatch.enabled,toggle:async()=>{const api=props.watchApi!;if(openWatch.enabled)await api.pause(openWatch.threadId);else await api.enable(openWatch.threadId);setOpenWatch(w=>w&&{...w,enabled:!w.enabled});await (api.refreshWatches?.()??api.watches());},remove:async()=>{const api=props.watchApi!;if(!api.remove)throw Error("WATCH_REMOVAL_UNAVAILABLE");await api.remove("WATCH",openWatch.threadId,true);await (api.refreshWatches?.()??api.watches());setOpenWatch(null);}}:undefined}/></section>}
        {surface==="NOTIFICATIONS"&&!props.notificationDetail&&<section className="r2-directory r2-todo"><ConversationHome mode="TODO" onOpenBoard={()=>navigate("BOARD")} bridges={items} activity={activity.rows} activityClock={activity.clock} watchApi={props.watchApi} addRequest={0} onOpenBridge={id=>{props.onSelectWorkstream(id);navigate("WORKSPACE");}} onOpenWatch={watch=>{navigate("BRIDGES");setOpenWatch(watch);}}/><small className="v6-section">{uiText("最近通知")}</small></section>}
        <section hidden={surface!=="BRIDGES"||Boolean(watchChat)} className="r2-directory"><ConversationHome bridges={items} activity={activity.rows} activityClock={activity.clock} watchApi={props.watchApi} addRequest={addRequest} onOpenBridge={id=>{props.onSelectWorkstream(id);navigate("WORKSPACE");}} onOpenWatch={setOpenWatch} onCreateBridge={props.onCreateBridge} onDeleteBridge={props.onLifecycleChange?removeBridge:undefined}/></section>
        {surface==="NEW_BRIDGE"&&props.onCreateBridge&&<NewBridgeForm create={props.onCreateBridge} cancel={()=>navigate("BRIDGES")}/>}
        {props.quotaPanel&&<section hidden={surface!=="SETTINGS"||Boolean(detail)} className="r2-settings r2-settings-quota"><small>{uiText("用量")}</small><div className="r2-settings-group">{props.quotaPanel}</div></section>}
        {surface==="SETTINGS"&&!detail&&<section className="r2-settings"><small>{uiText("连接")}</small><div className="r2-settings-group"><button type="button" onClick={()=>navigate("RUNTIME")}><Monitor size={20}/><span>Codex</span><ChevronRight size={18}/></button><button type="button" onClick={()=>{if(props.onOpenWebAccess)props.onOpenWebAccess();else setDetail("DEVICES");}}><Smartphone size={20}/><span>{uiText("设备")}</span><ChevronRight size={18}/></button>{props.assistantPanel&&<button type="button" onClick={()=>setDetail("ASSISTANT")}><Bot size={20}/><span>{uiText("AI 助手")}</span><ChevronRight size={18}/></button>}</div><small>{uiText("偏好")}</small><div className="r2-settings-group"><LanguagePicker/><button type="button" onClick={()=>setDetail("NOTIFICATIONS")}><Bell size={20}/><span>{uiText("消息通知")}</span><ChevronRight size={18}/></button><button type="button" onClick={()=>setDetail("BOARD_SOURCE")}><LayoutGrid size={20}/><span>{uiText("看板来源")}</span><ChevronRight size={18}/></button></div><small>{uiText("应用")}</small><div className="r2-settings-group"><button type="button" onClick={()=>setDetail("INSTALL")}><Smartphone size={20}/><span>{uiText("添加到手机主屏幕")}</span><ChevronRight size={18}/></button><button type="button" onClick={()=>navigate("RECYCLE")}><Trash2 size={20}/><span>{uiText("已删除的 Bridge")}</span><ChevronRight size={18}/></button>{props.updatePanel&&<button type="button" onClick={()=>setDetail("UPDATE")}><Download size={20}/><span>{uiText("检查更新")}</span><ChevronRight size={18}/></button>}<button type="button" onClick={()=>setDetail("ABOUT")}><Info size={20}/><span>{uiText("关于")}</span><ChevronRight size={18}/></button></div></section>}
        {surface==="SETTINGS"&&detail&&<section className="r2-settings">{detail==="INSTALL"?<PwaInstallGuide expanded/>:detail==="ABOUT"?<><h2>Agbrio</h2><p>{uiText("Agent Bridge · 连接你的 Agent 对话")}</p><p className="r2-about-version">{uiText("版本")} {productVersion} {uiText("· 界面")} {PHONE_LAYOUT_REVISION}</p>{location.pathname.startsWith("/mobile")&&<PwaUpdateSettings/>}<div className="r2-settings-group"><div className="r2-connection-row"><Monitor size={20}/><strong>{uiText("Windows 桌面")}</strong><span>{uiText("已支持")}</span></div><div className="r2-connection-row"><Smartphone size={20}/><strong>{uiText("手机 PWA")}</strong><span>{uiText("个人配对")}</span></div><div className="r2-connection-row"><Monitor size={20}/><strong>{uiText("Mac 共享连接")}</strong><span>{uiText("尚未验证")}</span></div></div><p>{uiText("Bridge 连接两端对话；通知可以直接回复原对话。")}</p><p className="r2-about-credit">{uiText("头像来自 DiceBear Sprouts 与 Critters（CC0 1.0）。")}</p><a href="mailto:geoffreyzjx@qq.com">{uiText("联系作者")}</a><button type="button" className="r2-diagnostic-copy" onClick={()=>{const report=displayDiagnostics();void (async()=>{try{await navigator.clipboard.writeText(report);setDisplayCopied(true);setDisplayReport("");}catch{setDisplayReport(report);setDisplayCopied(false);}})();}}><Copy size={18}/>{displayCopied?uiText("已复制显示诊断"):uiText("复制显示诊断")}</button>{displayReport&&<textarea aria-label={uiText("显示诊断")} readOnly value={displayReport} rows={6} onFocus={event=>event.currentTarget.select()}/>}</>:detail==="BOARD_SOURCE"?<BoardSourceSettings/>:detail==="UPDATE"?props.updatePanel:detail==="ASSISTANT"?props.assistantPanel:detail==="NOTIFICATIONS"?(props.notificationSettings??<p className="r2-settings-empty">{uiText("这台设备上没有可设置的通知。")}</p>):props.devicePanel}</section>}
        {surface==="RUNTIME"&&<section className="r2-settings">{props.runtimeAddon}<small>{uiText("连接状态")}</small>{!props.runtime?.checks.length&&<p className="r2-settings-empty">{props.runtime?uiText("没有需要显示的连接。"):uiText("正在读取连接状态…")}</p>}<div className="r2-settings-group" hidden={!props.runtime?.checks.length}>{props.runtime?.checks.filter(check=>["codex","browser-runtime","mobile-listener","router-host"].includes(check.id)).map(check=><div className="r2-connection-row" key={check.id}><strong>{uiText(check.label)}</strong><span>{check.state==="READY"?uiText("可用"):check.state==="CHECKING"?uiText("检查中"):uiText("待处理")}</span>{check.action&&<button type="button" className="r2-icon" aria-label={uiText(check.actionLabel??uiText("检查状态"))} title={uiText(check.actionLabel??uiText("检查状态"))} onClick={check.action}><ChevronRight size={18}/></button>}</div>)}</div></section>}
        {surface==="RECYCLE"&&<section className="r2-directory"><div className="r2-settings-group">{trash.map(item=><div key={item.id} className="r2-connection-row"><strong title={item.name}>{item.name}</strong><button type="button" disabled={actionBusy} className="r2-icon" aria-label={uiText("恢复 {0}", item.name)} onClick={()=>void changeLifecycle(item,"ACTIVE")}><Undo2 size={18}/></button></div>)}</div>{!trash.length&&<div className="r2-empty"><Trash2 size={28}/><h2>{uiText("没有已删除的 Bridge")}</h2></div>}</section>}
        {!topLevel&&!inReader&&!watchChat&&!["NEW_BRIDGE","RUNTIME","RECYCLE","SETTINGS","NOTIFICATIONS"].includes(surface)&&<p role="status">{uiText("此入口已收起。")}<button type="button" onClick={()=>navigate("BRIDGES")}>{uiText("返回 Bridge")}</button></p>}
      </section>
    </div>
    {topLevel&&<div className="r2-mobile-navigation">{nav(true)}</div>}
    </section>
    <div className="r2-host-controls">{props.globalActions}</div>
    {removal.undoItem&&<div className="r2-toast" role="status">{uiText("已移除 Bridge")}<button type="button" onClick={removal.undo}>{uiText("撤销")}</button><button type="button" className="r2-icon" aria-label={uiText("关闭提示")} onClick={removal.dismiss}><X size={16}/></button></div>}
    <Dialog.Root open={Boolean(renaming)} onOpenChange={open=>{if(!open&&!actionBusy)setRenaming(null);}}><Dialog.Portal><Dialog.Overlay className="v3-role-dialog-overlay"/><Dialog.Content className="r2-confirmation" aria-describedby={undefined}><Dialog.Title>{uiText("重命名 Bridge")}</Dialog.Title><form onSubmit={event=>{event.preventDefault();if(!renaming||!props.onRenameBridge||pendingAction.current)return;pendingAction.current=true;setActionBusy(true);setActionError("");void props.onRenameBridge(renaming.id,renameValue.trim()).then(()=>setRenaming(null)).catch(()=>setActionError(uiText("名称未保存，请重试。"))).finally(()=>{pendingAction.current=false;setActionBusy(false);});}}><label>{uiText("名称")}<input autoFocus value={renameValue} maxLength={100} disabled={actionBusy} onChange={event=>setRenameValue(event.target.value)}/></label>{actionError&&<p role="alert">{uiText(actionError)}</p>}<footer><Dialog.Close asChild><button type="button" disabled={actionBusy}>{uiText("取消")}</button></Dialog.Close><button type="submit" className="r2-action" disabled={actionBusy||!renameValue.trim()}>{actionBusy?uiText("保存中…"):uiText("保存")}</button></footer></form></Dialog.Content></Dialog.Portal></Dialog.Root>
  </main></MotionConfig></MediaPreviewProvider></NativeCopyProvider></NativeSurfaceContext.Provider>;
}
