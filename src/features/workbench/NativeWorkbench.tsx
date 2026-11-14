import {SettingsIndexGroup,SettingsIndexItem,SettingsJumpNav,useSettingsIndex,type SettingsGroup} from "./SettingsIndex";
import {ManagementZones} from "./ManagementZones";
import {CacheSettings,useCacheMaintenance} from './CacheSettings';
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
import {LayoutGrid, ListChecks, MessageSquare, ChevronLeft, ChevronRight, CircleCheck, Download, Copy, Ellipsis, History, Link2, Pencil, Plus, Settings, Smartphone, Trash2, Undo2, X} from "lucide-react";
import {version} from "../../../package.json";
const productVersion=(import.meta as ImportMeta & {env?:{VITE_AGBRIO_APP_VERSION?:string}}).env?.VITE_AGBRIO_APP_VERSION||version;
import type {UnifiedWorkbenchProps, WorkbenchSurface} from "./UnifiedWorkbench";
import type {WorkbenchItem} from "./models";
import {NewBridgeForm} from "./BridgeDirectory";
import {PanelDivider} from "./PanelDivider";
import {NativeSurfaceContext,ConnectionSettingsNavigation} from "./NativeSurfaceContext";
import {CopyAction,NativeCopyProvider} from "./NativeReaderActions";
import {MediaPreviewProvider} from "../codex/MediaPreview";
import {SwipeDeleteRow} from "./SwipeDeleteRow";
import {useUndoRemoval} from "./useUndoRemoval";
import {BrandMark} from "./BrandMark";
import {ConversationHome,type WatchOpenTarget} from "./ConversationHome";
import {TodoPage,useTodoCards} from "./TodoPage";
import {DesktopOverview,ComputerStatusCard,type OverviewTarget} from "./SettingsHome";
import {WatchChat} from "./WatchChat";
import type {CodexWatch} from "./CodexNotifications";

// Wide desktops keep the conversation list beside the open conversation.
// Phones (same query as the phone stylesheet) keep the single-column flow.
const PHONE_QUERY="(max-width:760px),(hover:none) and (pointer:coarse) and (max-width:1100px)";
const SPLIT_QUERY="(min-width:1024px)";
function readSplit(){try{return Boolean(window.matchMedia?.(SPLIT_QUERY)?.matches)&&!window.matchMedia?.(PHONE_QUERY)?.matches;}catch{return false;}}
function useSplitLayout(){
 const [split,setSplit]=useState(readSplit);
 useEffect(()=>{
  const lists=[SPLIT_QUERY,PHONE_QUERY].map(q=>{try{return window.matchMedia?.(q);}catch{return undefined;}}).filter((l):l is MediaQueryList=>Boolean(l));
  const update=()=>setSplit(readSplit());
  lists.forEach(l=>l.addEventListener?.("change",update));update();
  return()=>lists.forEach(l=>l.removeEventListener?.("change",update));
 },[]);
 return split;
}

const destinations = [{id:"BRIDGES", get label(){return uiText("对话");}, Icon:MessageSquare}, {id:"BOARD", get label(){return uiText("看板");}, Icon:LayoutGrid}, {id:"NOTIFICATIONS", get label(){return uiText("待办");}, Icon:ListChecks}, {id:"SETTINGS", get label(){return uiText("设置");}, Icon:Settings}] as const;
const date = (value?:number|null) => value ? new Intl.DateTimeFormat(getLanguage(), {month:"numeric",day:"numeric",hour:"2-digit",minute:"2-digit"}).format(value) : "";

/** Native R2 composition. Hosts remain the sole routing and mutation authority. */
export function NativeWorkbench(props:UnifiedWorkbenchProps) {
 const keyboardMotion=useAppMotion();
 useLanguage();useCacheMaintenance();
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
  const [openTarget,setOpenTarget] = useState<WatchOpenTarget|undefined>(undefined);
  const [menu,setMenu] = useState(false);
  const [copyText,setCopyText]=useState<string|null>(null);
  const [renaming,setRenaming]=useState<WorkbenchItem|null>(null),[renameValue,setRenameValue]=useState("");
  const [width,setWidth] = useState(()=>{try{return Math.min(390,Math.max(240,Number(localStorage.getItem("aiwr.r2.master-width"))||290));}catch{return 290;}});
  const [displayReport,setDisplayReport]=useState("");
  const [displayCopied,setDisplayCopied]=useState(false);
  const [actionBusy,setActionBusy]=useState(false),[actionError,setActionError]=useState("");
  const [restoreNotice,setRestoreNotice]=useState('');
  const pendingAction=useRef(false);
  useBackLayer(menu,()=>setMenu(false));
  useBackLayer(Boolean(renaming),()=>setRenaming(null));
  useBackLayer(Boolean(openWatch),()=>setOpenWatch(null));
  const reduced = useReducedMotion();
  const content = useRef<HTMLElement>(null);
  const scrolls = useRef<Record<string,number>>({});
  const [online,setOnline] = useState(()=>navigator.onLine);
  const active = surface === "BOARD" ? "BOARD" : surface === "NOTIFICATIONS" ? "NOTIFICATIONS" : ["SETTINGS","RUNTIME","RECYCLE","INSTALL","NOTIFICATION_LOG"].includes(surface) ? "SETTINGS" : "BRIDGES";
  const removal=useUndoRemoval<WorkbenchItem>(item=>item.id,async(item,removed)=>{await props.onLifecycleChange?.(item.id,removed?"TRASHED":"ACTIVE");if(!removed&&surface==="RECYCLE")setRestoreNotice(uiText("已恢复 {0}",item.name));},setActionError);
  const projected=removal.project(props.items.filter(item=>item.lifecycle==="ACTIVE"));
  function removeBridge(item:WorkbenchItem){removal.remove(item);if(inReader)navigate("BRIDGES");}
  const selected = props.items.find(item=>item.id===props.selectedWorkstreamId);
  const inReader = surface === "WORKSPACE" && Boolean(selected);
  useEffect(()=>{if(inReader&&selected?.lifecycle!=="ACTIVE")navigate("BRIDGES");},[inReader,selected?.lifecycle,navigate]);
  const items = useMemo(()=>projected.sort((a,b)=>Number(Boolean(b.pinned))-Number(Boolean(a.pinned))||(b.updatedAt??0)-(a.updatedAt??0)),[props.items,projected]);
  const visible = items.filter(item=>`${item.name} ${item.projectName??""}`.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()));
  const watchChat = openWatch&&props.watchApi?.chat ? props.watchApi.chat : null;
  const split=useSplitLayout();
  const listPane=split&&(surface==="BRIDGES"||inReader);
  const listScroll=useRef<HTMLElement>(null);
  const watchOpener=useRef<HTMLElement|null>(null);
  const openFromList=(watch:CodexWatch,target?:WatchOpenTarget)=>{watchOpener.current=document.activeElement instanceof HTMLElement?document.activeElement:null;setOpenTarget(target);setOpenWatch(watch);};
  useLayoutEffect(()=>{if(watchChat||!watchOpener.current)return;const opener=watchOpener.current;watchOpener.current=null;if(opener.isConnected&&opener.getClientRects().length)opener.focus({preventScroll:true});},[watchChat]);
  const selectedKey=watchChat&&openWatch?`w:${openWatch.threadId}`:inReader&&selected?`b:${selected.id}`:undefined;
  const trash = props.items.filter(item=>item.lifecycle==="TRASHED"||removal.isRemoved(item));
  const scrollKey = `${surface}:${inReader?props.selectedWorkstreamId??"":""}${watchChat?`:watch:${openWatch?.threadId}`:""}`;
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
  useEffect(()=>{setMenu(false);setOpenWatch(null);},[surface,props.selectedWorkstreamId]);
  const nav = (phone:boolean) => <nav className={phone?"r2-tabbar":"r2-destinations"} aria-label={phone?uiText("主导航"):uiText("桌面导航")}>{destinations.map(({id,label,Icon})=><button key={id} type="button" aria-current={active===id?"page":undefined} onClick={()=>{setOpenWatch(null);navigate(id);}}>{active===id&&<motion.span className="r2-nav-selection" layoutId={phone?"phone-destination":"desktop-destination"} transition={reduced||keyboardMotion?{duration:0}:{type:"spring",stiffness:560,damping:42}}/>}<Icon size={phone?23:20} aria-hidden/><span>{uiText(label)}</span>{id==="SETTINGS"&&props.updateAvailable&&<span className="agbrio-update-dot" role="img" aria-label={uiText("有新版本")} title={uiText("有新版本")}/ >}{id==="NOTIFICATIONS"&&todoCount>0&&<b className="r2-nav-badge" aria-label={tc("{0} 件待办", todoCount)}>{todoCount>99?"99+":todoCount}</b>}</button>)}</nav>;
  const listed=visible.slice(0,limit);
  const activity=useBridgeActivity(items.slice(0,40).map(i=>i.id),props.bridgeActivityApi,surface==="BRIDGES"||surface==="NOTIFICATIONS"||inReader);
  const bridgeUnread=(item:WorkbenchItem)=>bridgeReviewAttention(activity.rows[item.id]?.row,activity.rows[item.id]?Math.max(0,Date.now()-activity.rows[item.id].receivedAt):Infinity);
  // The badge is the final to-do card count; result notifications never count.
  const todoCount=useTodoCards({watchApi:props.watchApi,bridges:items,activity:activity.rows,activityClock:activity.clock}).length;
  const rows = (compact=false) => <div className={`r2-bridge-group${compact?" r2-compact-list":""}`}>
    {(compact&&selected&&selected.lifecycle==="ACTIVE"&&!listed.some(item=>item.id===selected.id)?[selected,...listed]:listed).map(item=><SwipeDeleteRow key={item.id} label={item.name} disabled={!props.onLifecycleChange} onDelete={()=>removeBridge(item)}><div className="r2-bridge-row" data-bridge-id={item.id} data-selected={inReader&&selected?.id===item.id}>
      <button type="button" className="r2-bridge-open" onClick={()=>{props.onSelectWorkstream(item.id);navigate("WORKSPACE");}} aria-current={inReader&&selected?.id===item.id?"page":undefined}>
        <span className="r2-context-icon"><Link2 size={22}/></span><span className="r2-row-content"><span className="r2-row-title"><strong title={item.name}>{item.name}</strong>{Boolean(bridgeUnread(item))&&<span className="r2-bridge-new-reply" aria-label={uiText("有新回复待审阅")}><i aria-hidden/></span>}<time>{date(item.updatedAt)}</time></span><span className="r2-row-state"><span>{item.sourceLabel||item.projectName||uiText("待绑定")}</span></span>{activity.rows[item.id]?.row.sides.length>0&&<BridgeActivityStrip activity={activity.rows[item.id].row} age={Math.max(0,activity.clock-activity.rows[item.id].receivedAt)}/>}</span><ChevronRight size={16} className="r2-chevron"/>
      </button>
    </div></SwipeDeleteRow>)}
  </div>;
  async function changeLifecycle(item:WorkbenchItem,lifecycle:"ACTIVE"|"TRASHED"){
    if(lifecycle==="TRASHED")removeBridge(item);else {setActionError("");setRestoreNotice("");removal.restore(item);}
  }
  const codexPanel=props.codexConnectionPanel??props.runtimeAddon;
  const desktopSettings=Boolean(props.overviewApi||props.onOpenWebAccess||codexPanel||props.piSettingsPanel);
  const inSettings=surface==="SETTINGS";
  const settingsSplit=split&&inSettings;
  const {current:settingsCurrent,jump:jumpSettings}=useSettingsIndex(inSettings,content);
  const settingsGroups:SettingsGroup[]=["OVERVIEW",...(codexPanel||props.piSettingsPanel||props.assistantPanel?["CONNECTIONS" as const]:[]),"DEVICES","NOTIFY",...(props.runtimeProtectionPanel?["PROTECTION" as const]:[]),"WORKSPACE","GENERAL"];
  const jumpTo=(id:SettingsGroup,target?:string)=>{jumpSettings(id,target);if(!inSettings)navigate("SETTINGS");};
  const settingsTask=(target:"RUNTIME"|"RECYCLE"|"NOTIFICATION_LOG")=>navigate(target);
  const go=(icon:React.ReactNode,label:string,onClick:()=>void,sub?:React.ReactNode)=><button type="button" onClick={onClick}>{icon}<span className="v7-row-text"><span>{label}</span>{sub?<small>{sub}</small>:null}</span><ChevronRight size={18}/></button>;
  const group=(children:React.ReactNode)=><div className="r2-settings-group">{children}</div>;
  const checks=props.runtime?.checks.filter(check=>["codex","browser-runtime","mobile-listener","router-host","router-core"].includes(check.id))??[];
  const runtimeChecks=<>{props.runtime?.notice&&<p role="status">{uiText(props.runtime.notice)}</p>}{!checks.length&&<p className="r2-settings-empty">{props.runtime?uiText("没有需要显示的连接。"):uiText("正在读取连接状态…")}</p>}{checks.length>0&&<div className="r2-settings-group">{checks.map(check=><div className="r2-connection-row settings-runtime-row" key={check.id}>
    <div><strong>{uiText(check.label)}</strong><p>{uiText(check.detail)}</p></div><span>{check.state==="READY"?uiText("可用"):check.state==="CHECKING"?uiText("检查中"):uiText("待处理")}</span>
    {check.action&&<button type="button" disabled={check.state==="CHECKING"} onClick={check.action}>{uiText(check.actionLabel??"检查状态")}</button>}
    {check.secondaryAction&&<button type="button" disabled={check.state==="CHECKING"} onClick={check.secondaryAction}>{uiText(check.secondaryActionLabel??"检查状态")}</button>}
  </div>)}</div>}</>;
  const overview=props.overviewApi?<DesktopOverview api={props.overviewApi} onPair={props.onOpenWebAccess} onNavigate={(target:OverviewTarget)=>target==="CODEX"?jumpTo("CONNECTIONS","settings-codex"):target==="PI_SETTINGS"?jumpTo("CONNECTIONS","settings-pi"):target==="DEVICES"?jumpTo("DEVICES"):jumpTo("GENERAL","settings-update")}/>:<ComputerStatusCard runtime={props.runtime} identity={props.computerIdentity} onOpen={()=>settingsTask("RUNTIME")}/>;
  const title = surface==="MANAGEMENT"?uiText("管理区"):surface==="BOARD"?uiText("看板"):surface==="BRIDGES"?uiText("对话"):surface==="NEW_BRIDGE"?uiText("新建 Bridge"):surface==="NOTIFICATIONS"?uiText("待办"):inSettings?uiText("设置"):surface==="RUNTIME"?uiText(codexPanel?"Codex 连接":"电脑连接"):surface==="NOTIFICATION_LOG"?uiText("通知记录"):surface==="RECYCLE"?uiText("已删除的 Bridge"):selected?.name??"Bridge";
  const topLevel = ["BOARD","BRIDGES","NOTIFICATIONS","SETTINGS"].includes(surface)&&!watchChat;
  useSurfaceTransition(content,`${scrollKey}:${Boolean(props.notificationDetail)}:${openWatch?.threadId??""}`,!topLevel,keyboardMotion);
  const back=()=>{const parent=history.state?.aiwrParentSurface;if((surface==="NOTIFICATION_LOG"&&parent==="NOTIFICATIONS")||(["RECYCLE","RUNTIME","NOTIFICATION_LOG"].includes(surface)&&parent==="SETTINGS"))history.back();else navigate(["RECYCLE","RUNTIME","NOTIFICATION_LOG"].includes(surface)?"SETTINGS":"BRIDGES");};
  return <ConnectionSettingsNavigation.Provider value={()=>jumpTo("DEVICES")}><NativeSurfaceContext.Provider value><NativeCopyProvider register={setCopyText}><MediaPreviewProvider><MotionConfig reducedMotion={keyboardMotion?"always":"user"}><main className={`unified-workbench r2-router v3-surface-${surface.toLowerCase()}`} data-detail={!topLevel} aria-label={uiText("Agbrio 统一工作台")} style={{"--r2-master-width":`${width}px`} as React.CSSProperties}>
    <aside className="r2-rail"><div className="r2-brand"><BrandMark/>Agbrio</div>{nav(false)}<small className="r2-rail-caption">Bridge</small><div className="r2-rail-bridges">{items.map(item=><button key={item.id} type="button" aria-current={inReader&&item.id===selected?.id?"page":undefined} onClick={()=>{props.onSelectWorkstream(item.id);navigate("WORKSPACE");}}>{item.name}{bridgeUnread(item)>0&&<i className="r2-rail-reply-dot" aria-label={uiText("有新回复待审阅")}/>}</button>)}</div></aside>
    <section className="r2-workspace" data-split={split||undefined}>{split&&<aside className="v7-list-pane" hidden={!listPane} aria-label={uiText("对话")}>
      <header className="v7-list-head"><h1>{uiText("对话")}</h1><button type="button" className="r2-icon r2-accent" aria-label={uiText("监听对话或新建 Bridge")} onClick={()=>setAddRequest(v=>v+1)}><Plus size={20}/></button></header>
      <section ref={listScroll} className="r2-directory v7-list-scroll"><ConversationHome onOpenManagement={()=>navigate("MANAGEMENT")} scrollContainer={listScroll} selectedKey={selectedKey} onRefreshList={listPane&&props.onRefreshConversationDirectory?async()=>{const results=await Promise.allSettled([props.onRefreshConversationDirectory!(),activity.refresh()]);if(results.some(r=>r.status==="rejected"))throw Error("LIST_REFRESH_INCOMPLETE");}:undefined} bridges={items} activity={activity.rows} activityClock={activity.clock} watchApi={props.watchApi} addRequest={addRequest} onOpenBridge={id=>{props.onSelectWorkstream(id);navigate("WORKSPACE");}} onOpenWatch={openFromList} onCreateBridge={props.onCreateBridge} onDeleteBridge={props.onLifecycleChange?removeBridge:undefined}/></section>
    </aside>}{settingsSplit&&<SettingsJumpNav groups={settingsGroups} current={settingsCurrent} jump={jumpSettings}/>}{(node=>split?<div className="v7-detail-pane">{node}</div>:node)(<><header className="r2-header" hidden={surface==="NOTIFICATION_LOG"&&props.notificationDetail||Boolean(watchChat)||(listPane&&surface==="BRIDGES")}>
      {!topLevel&&!listPane&&<button type="button" className="r2-icon" aria-label={uiText("返回")} title={uiText("返回")} onClick={back}><ChevronLeft size={20}/></button>}<h1>{title}</h1>{surface==="NOTIFICATIONS"&&todoCount>0&&<span className="v7-head-count" role="status">{uiText("{0} 件事等你处理",todoCount)}</span>}
      {surface==="BRIDGES"&&!listPane&&<button type="button" className="r2-icon r2-accent" aria-label={uiText("监听对话或新建 Bridge")} onClick={()=>setAddRequest(v=>v+1)}><Plus size={20}/></button>}
      {inReader&&copyText&&<CopyAction text={copyText} onError={()=>setActionError(uiText("复制未成功，可选择正文复制。"))}/>}
      {inReader&&<Dialog.Root open={menu} onOpenChange={setMenu}><Dialog.Trigger asChild><button type="button" className="r2-icon" aria-label={uiText("Bridge 操作")}><Ellipsis size={20}/></button></Dialog.Trigger><Dialog.Portal><Dialog.Overlay className="r2-menu-overlay"/><Dialog.Content className="r2-menu" aria-describedby={undefined}><Dialog.Title className="r2-sr-only">{uiText("Bridge 操作")}</Dialog.Title><button type="button" onClick={()=>{setMenu(false);props.onManageBindings?.();}}><Link2 size={18}/>{uiText("更换两端")}</button>{props.onRenameBridge&&selected&&<button type="button" onClick={()=>{setMenu(false);setRenameValue(selected.name);setRenaming(selected);setActionError("");}}><Pencil size={18}/>{uiText("重命名")}</button>}{props.onPinChange&&selected&&<button type="button" onClick={()=>{props.onPinChange?.(selected.id,!selected.pinned);setMenu(false);}}><CircleCheck size={18}/>{selected.pinned?uiText("取消置顶"):uiText("置顶")}</button>}{props.onLifecycleChange&&selected&&<button type="button" className="r2-danger" onClick={()=>{setMenu(false);removeBridge(selected);}}><Trash2 size={18}/>{uiText("删除 Bridge")}</button>}<Dialog.Close asChild><button type="button"><X size={18}/>{uiText("关闭")}</button></Dialog.Close></Dialog.Content></Dialog.Portal></Dialog.Root>}
    </header>
    {inSettings&&!settingsSplit&&<SettingsJumpNav groups={settingsGroups} current={settingsCurrent} jump={jumpSettings} compact/>}
    {!online&&<p className="r2-offline" role="status">{uiText("连接中断 · 等待恢复")}</p>}
    <div className={`r2-stage${inReader?" r2-split":""}`}>
      {/* The sidebar already lists every Bridge; the reader uses the full width. */}
      <section ref={content} className="v3-workbench-main r2-content">
        {props.criticalNotice}{actionError&&<p role="alert">{uiText(actionError)}</p>}
        <section hidden={surface!=="BOARD"} className="character-board-mount"><BoardView active={surface==="BOARD"}/></section>
        <div hidden={!inReader} className="r2-reader-mount">{props.bridgePanel}</div>
        {watchChat&&openWatch&&<section className="r2-watch-mount"><WatchChat key={`${openWatch.threadId}:${openTarget?.childId??""}`} original={openWatch} api={watchChat} initialChildId={openTarget?.childId} parentLabel={openTarget?.parentLabel} onBack={()=>setOpenWatch(null)} backLabel={uiText(surface==="MANAGEMENT"?"管理区":surface==="NOTIFICATIONS"?"待办":"对话")} controls={props.watchApi?{enabled:openWatch.enabled,toggle:async()=>{const api=props.watchApi!;if(openWatch.enabled)await api.pause(openWatch.threadId);else await api.enable(openWatch.threadId);setOpenWatch(w=>w&&{...w,enabled:!w.enabled});await (api.refreshWatches?.()??api.watches());},remove:async()=>{const api=props.watchApi!;if(!api.remove)throw Error("WATCH_REMOVAL_UNAVAILABLE");await api.remove("WATCH",openWatch.threadId,true);await (api.refreshWatches?.()??api.watches());setOpenWatch(null);}}:undefined}/></section>}
        {surface==="MANAGEMENT"&&<div hidden={Boolean(watchChat)}><ManagementZones api={props.managementApi} watchApi={props.watchApi} onOpenWatch={openFromList}/></div>}
        {surface==="NOTIFICATIONS"&&<section hidden={Boolean(watchChat)} className="r2-directory r2-todo"><TodoPage watchApi={props.watchApi} bridges={items} activity={activity.rows} activityClock={activity.clock} onOpenBridge={id=>{props.onSelectWorkstream(id);navigate("WORKSPACE");}} onOpenWatch={openFromList} onOpenBoard={()=>navigate("BOARD")} onOpenLog={()=>settingsTask("NOTIFICATION_LOG")} countInHeader/></section>}
        {listPane&&surface==="BRIDGES"&&!watchChat&&<div className="v7-detail-empty" role="status"><MessageSquare size={28} aria-hidden/><p>{uiText("选择左侧对话查看详情")}</p></div>}
        {!split&&<section hidden={surface!=="BRIDGES"||Boolean(watchChat)} className="r2-directory"><ConversationHome onOpenManagement={()=>navigate("MANAGEMENT")} scrollContainer={content} onRefreshList={surface==="BRIDGES"&&!watchChat&&props.onRefreshConversationDirectory?async()=>{const results=await Promise.allSettled([props.onRefreshConversationDirectory!(),activity.refresh()]);if(results.some(r=>r.status==="rejected"))throw Error("LIST_REFRESH_INCOMPLETE");}:undefined} bridges={items} activity={activity.rows} activityClock={activity.clock} watchApi={props.watchApi} addRequest={addRequest} onOpenBridge={id=>{props.onSelectWorkstream(id);navigate("WORKSPACE");}} onOpenWatch={openFromList} onCreateBridge={props.onCreateBridge} onDeleteBridge={props.onLifecycleChange?removeBridge:undefined}/></section>}
        {surface==="NEW_BRIDGE"&&props.onCreateBridge&&<NewBridgeForm create={props.onCreateBridge} cancel={()=>navigate("BRIDGES")}/>}
        <div className="r2-settings v7-settings settings-index" hidden={!inSettings}>
          <SettingsIndexGroup id="OVERVIEW">
            {inSettings&&overview}
            {props.quotaPanel&&<SettingsIndexItem label="用量">{group(props.quotaPanel)}</SettingsIndexItem>}
          </SettingsIndexGroup>
          {inSettings&&<>
            {settingsGroups.includes("CONNECTIONS")&&<SettingsIndexGroup id="CONNECTIONS">
              {codexPanel&&<SettingsIndexItem id="settings-codex" label="Codex 连接">{codexPanel}<details className="v7-help"><summary>{uiText("连接状态")}</summary>{runtimeChecks}</details></SettingsIndexItem>}
              {props.piSettingsPanel&&<SettingsIndexItem id="settings-pi" label="Pi 子任务">{props.piSettingsPanel}</SettingsIndexItem>}
              {props.assistantPanel&&<SettingsIndexItem label="AI 助手">{props.assistantPanel}</SettingsIndexItem>}
            </SettingsIndexGroup>}
            <SettingsIndexGroup id="DEVICES">
              {props.onOpenWebAccess&&group(go(<Smartphone size={20}/>,uiText("手机配对与已登录设备"),props.onOpenWebAccess))}
              {!desktopSettings&&props.devicePanel&&<div className="v7-device-exit">{props.devicePanel}</div>}
              <PwaInstallGuide/>
            </SettingsIndexGroup>
            <SettingsIndexGroup id="NOTIFY">
              {props.notificationSettings??<p className="r2-settings-empty">{uiText("这台设备上没有可设置的通知。")}</p>}
              {group(go(<History size={20}/>,uiText("通知记录"),()=>settingsTask("NOTIFICATION_LOG"),uiText("最近 20 条结果和状态变化，不计入待办")))}
            </SettingsIndexGroup>
            {props.runtimeProtectionPanel&&<SettingsIndexGroup id="PROTECTION">{props.runtimeProtectionPanel}</SettingsIndexGroup>}
            <SettingsIndexGroup id="WORKSPACE">
              <SettingsIndexItem label="看板管理"><BoardSourceSettings onOpen={()=>navigate("BOARD")}/></SettingsIndexItem>
              {group(go(<Trash2 size={20}/>,uiText("已删除的 Bridge"),()=>settingsTask("RECYCLE")))}
            </SettingsIndexGroup>
            <SettingsIndexGroup id="GENERAL">
              {group(<LanguagePicker/>)}
              <SettingsIndexItem id="settings-update" label="版本与更新">
                {props.updateAvailable&&<p className="agbrio-update-indicator"><i className="agbrio-update-dot" aria-hidden/>{uiText("有新版本")}</p>}
                <p className="r2-about-version">Agbrio · {uiText("版本")} {productVersion} · {uiText("界面")} {PHONE_LAYOUT_REVISION}</p>
                {props.updatePanel}
                {!desktopSettings&&location.pathname.startsWith("/mobile")&&<PwaUpdateSettings/>}
              </SettingsIndexItem>
              <CacheSettings/>
              <SettingsIndexItem label="关于">
                <button type="button" className="r2-diagnostic-copy" onClick={()=>{const report=displayDiagnostics();void (async()=>{try{await navigator.clipboard.writeText(report);setDisplayCopied(true);setDisplayReport("");}catch{setDisplayReport(report);setDisplayCopied(false);}})();}}><Copy size={18}/>{displayCopied?uiText("已复制显示诊断"):uiText("复制显示诊断")}</button>
                {displayReport&&<textarea aria-label={uiText("显示诊断")} readOnly value={displayReport} rows={6} onFocus={event=>event.currentTarget.select()}/>}
                <p className="r2-about-credit">{uiText("头像来自 DiceBear Sprouts 与 Critters（CC0 1.0）。")}</p><a href="mailto:geoffreyzjx@qq.com">{uiText("联系作者")}</a>
              </SettingsIndexItem>
            </SettingsIndexGroup>
          </>}
        </div>
        {surface==="RUNTIME"&&<section className="r2-settings v7-settings">{codexPanel}{runtimeChecks}</section>}
        {surface==="NOTIFICATION_LOG"&&!props.notificationDetail&&<p className="v7-log-note">{uiText("最近 20 条结果和状态变化。这里只是记录，不计入待办，也不亮红点。")}</p>}
        {surface==="RECYCLE"&&<section className="r2-directory">{restoreNotice&&<p role="status">{restoreNotice}</p>}<div className="r2-settings-group">{trash.map(item=><div key={item.id} className="r2-connection-row"><strong title={item.name}>{item.name}</strong><button type="button" disabled={actionBusy} className="r2-icon" aria-label={uiText("恢复 {0}", item.name)} title={uiText("恢复 {0}", item.name)} onClick={()=>void changeLifecycle(item,"ACTIVE")}><Undo2 size={18}/></button></div>)}</div>{!trash.length&&<div className="r2-empty"><Trash2 size={28}/><h2>{uiText("没有已删除的 Bridge")}</h2></div>}</section>}
        {!topLevel&&!inReader&&!watchChat&&!["NEW_BRIDGE","RUNTIME","RECYCLE","SETTINGS","NOTIFICATIONS","NOTIFICATION_LOG","MANAGEMENT"].includes(surface)&&<p role="status">{uiText("此入口已收起。")}<button type="button" onClick={()=>navigate("BRIDGES")}>{uiText("返回 Bridge")}</button></p>}
      </section>
    </div></>)}
    {topLevel&&<div className="r2-mobile-navigation">{nav(true)}</div>}
    </section>
    <div className="r2-host-controls">{props.globalActions}</div>
    {removal.undoItem&&<div className="r2-toast" role="status">{uiText("已移除 Bridge")}<button type="button" onClick={removal.undo}>{uiText("撤销")}</button><button type="button" className="r2-icon" aria-label={uiText("关闭提示")} onClick={removal.dismiss}><X size={16}/></button></div>}
    <Dialog.Root open={Boolean(renaming)} onOpenChange={open=>{if(!open&&!actionBusy)setRenaming(null);}}><Dialog.Portal><Dialog.Overlay className="v3-role-dialog-overlay"/><Dialog.Content className="r2-confirmation" aria-describedby={undefined}><Dialog.Title>{uiText("重命名 Bridge")}</Dialog.Title><form onSubmit={event=>{event.preventDefault();if(!renaming||!props.onRenameBridge||pendingAction.current)return;pendingAction.current=true;setActionBusy(true);setActionError("");void props.onRenameBridge(renaming.id,renameValue.trim()).then(()=>setRenaming(null)).catch(()=>setActionError(uiText("名称未保存，请重试。"))).finally(()=>{pendingAction.current=false;setActionBusy(false);});}}><label>{uiText("名称")}<input autoFocus value={renameValue} maxLength={100} disabled={actionBusy} onChange={event=>setRenameValue(event.target.value)}/></label>{actionError&&<p role="alert">{uiText(actionError)}</p>}<footer><Dialog.Close asChild><button type="button" disabled={actionBusy}>{uiText("取消")}</button></Dialog.Close><button type="submit" className="r2-action" disabled={actionBusy||!renameValue.trim()}>{actionBusy?uiText("保存中…"):uiText("保存")}</button></footer></form></Dialog.Content></Dialog.Portal></Dialog.Root>
  </main></MotionConfig></MediaPreviewProvider></NativeCopyProvider></NativeSurfaceContext.Provider></ConnectionSettingsNavigation.Provider>;
}
