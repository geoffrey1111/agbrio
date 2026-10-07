import {useAppMotion} from "./useAppMotion";
import {LanguagePicker} from "../../i18n/LanguagePicker";
import {t as uiText,tc,useLanguage,getLanguage} from "../../i18n";
import {useVisibleViewport,PHONE_LAYOUT_REVISION} from "./mobileViewport";
import {displayDiagnostics} from "./displayDiagnostics";
import {useWorkbenchHistory,useBackLayer} from "./navigationHistory";
import {useEffect, useLayoutEffect, useMemo, useRef, useState} from "react";
import * as Dialog from "@radix-ui/react-dialog";
import {MotionConfig, motion, useReducedMotion} from "motion/react";
import {Bell, Bot, ChevronLeft, ChevronRight, CircleCheck, Copy, Ellipsis, Info, Link2, Monitor, Pencil, Plus, Search, Settings, Smartphone, Trash2, Undo2, X} from "lucide-react";
import {version} from "../../../package.json";
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

const destinations = [{id:"BRIDGES", label:"Bridge", Icon:Link2}, {id:"NOTIFICATIONS", get label(){return uiText("通知");}, Icon:Bell}, {id:"SETTINGS", get label(){return uiText("设置");}, Icon:Settings}] as const;
const date = (value?:number|null) => value ? new Intl.DateTimeFormat(getLanguage(), {month:"numeric",day:"numeric",hour:"2-digit",minute:"2-digit"}).format(value) : "";

/** Native R2 composition. Hosts remain the sole routing and mutation authority. */
export function NativeWorkbench(props:UnifiedWorkbenchProps) {
 useAppMotion();
 useLanguage();
  const [localSurface, setLocalSurface] = useState<WorkbenchSurface>("BRIDGES");
  const surface = props.surface ?? localSurface;
  const navigate = props.onSurfaceChange ?? setLocalSurface;
  useVisibleViewport();
  useWorkbenchHistory(surface,props.selectedWorkstreamId,navigate,props.onSelectWorkstream);
  const [query,setQuery] = useState("");
  const [limit,setLimit]=useState(20);
  useEffect(()=>setLimit(20),[query]);
  const [searching,setSearching] = useState(false);
  const [menu,setMenu] = useState(false);
  const [copyText,setCopyText]=useState<string|null>(null);
  const [renaming,setRenaming]=useState<WorkbenchItem|null>(null),[renameValue,setRenameValue]=useState("");
  const [width,setWidth] = useState(()=>{try{return Math.min(390,Math.max(240,Number(localStorage.getItem("aiwr.r2.master-width"))||290));}catch{return 290;}});
  const [detail,setDetail] = useState<"DEVICES"|"NOTIFICATIONS"|"ABOUT"|"ASSISTANT"|null>(null);
  const [displayReport,setDisplayReport]=useState("");
  const [displayCopied,setDisplayCopied]=useState(false);
  const [actionBusy,setActionBusy]=useState(false),[actionError,setActionError]=useState("");
  const pendingAction=useRef(false);
  useBackLayer(Boolean(detail),()=>setDetail(null));
  useBackLayer(menu,()=>setMenu(false));
  useBackLayer(Boolean(renaming),()=>setRenaming(null));
  const reduced = useReducedMotion();
  const content = useRef<HTMLElement>(null);
  const scrolls = useRef<Record<string,number>>({});
  const [online,setOnline] = useState(()=>navigator.onLine);
  const active = surface === "NOTIFICATIONS" ? "NOTIFICATIONS" : ["SETTINGS","RUNTIME","RECYCLE","INSTALL"].includes(surface) ? "SETTINGS" : "BRIDGES";
  const removal=useUndoRemoval<WorkbenchItem>(item=>item.id,async(item,removed)=>{await props.onLifecycleChange?.(item.id,removed?"TRASHED":"ACTIVE");},setActionError);
  const projected=removal.project(props.items.filter(item=>item.lifecycle==="ACTIVE"));
  function removeBridge(item:WorkbenchItem){removal.remove(item);if(inReader)navigate("BRIDGES");}
  const selected = props.items.find(item=>item.id===props.selectedWorkstreamId);
  const inReader = surface === "WORKSPACE" && Boolean(selected);
  useEffect(()=>{if(inReader&&selected?.lifecycle!=="ACTIVE")navigate("BRIDGES");},[inReader,selected?.lifecycle,navigate]);
  const items = useMemo(()=>projected.sort((a,b)=>Number(Boolean(b.pinned))-Number(Boolean(a.pinned))||(b.updatedAt??0)-(a.updatedAt??0)),[props.items,projected]);
  const visible = items.filter(item=>`${item.name} ${item.projectName??""}`.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()));
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
  const nav = (phone:boolean) => <nav className={phone?"r2-tabbar":"r2-destinations"} aria-label={phone?uiText("主导航"):uiText("桌面导航")}>{destinations.map(({id,label,Icon})=><button key={id} type="button" aria-current={active===id?"page":undefined} onClick={()=>{setDetail(null);navigate(id);}}>{active===id&&<motion.span className="r2-nav-selection" layoutId={phone?"phone-destination":"desktop-destination"} transition={reduced?{duration:0}:{type:"spring",stiffness:560,damping:42}}/>}<Icon size={phone?23:20} aria-hidden/><span>{uiText(label)}</span>{id==="NOTIFICATIONS"&&Boolean(props.notificationCount)&&<b className="r2-nav-badge" aria-label={tc("{0} 条通知", props.notificationCount??0)}>{Math.min(20,props.notificationCount!)}</b>}</button>)}</nav>;
  const listed=visible.slice(0,limit);
  const rows = (compact=false) => <div className={`r2-bridge-group${compact?" r2-compact-list":""}`}>
    {(compact&&selected&&selected.lifecycle==="ACTIVE"&&!listed.some(item=>item.id===selected.id)?[selected,...listed]:listed).map(item=><SwipeDeleteRow key={item.id} label={item.name} disabled={!props.onLifecycleChange} onDelete={()=>removeBridge(item)}><div className="r2-bridge-row" data-selected={inReader&&selected?.id===item.id}>
      <button type="button" className="r2-bridge-open" onClick={()=>{props.onSelectWorkstream(item.id);navigate("WORKSPACE");}} aria-current={inReader&&selected?.id===item.id?"page":undefined}>
        <span className="r2-context-icon"><Link2 size={22}/></span><span className="r2-row-content"><span className="r2-row-title"><strong>{item.name}</strong><time>{date(item.updatedAt)}</time></span><span className="r2-row-state"><span>{item.sourceLabel||item.projectName||uiText("待绑定")}</span></span></span><ChevronRight size={16} className="r2-chevron"/>
      </button>
    </div></SwipeDeleteRow>)}
  </div>;
  async function changeLifecycle(item:WorkbenchItem,lifecycle:"ACTIVE"|"TRASHED"){
    if(lifecycle==="TRASHED")removeBridge(item);else removal.restore(item);
  }
  const title = detail==="ASSISTANT"?uiText("AI 助手"):detail==="ABOUT"?uiText("关于 Router"):detail==="DEVICES"?uiText("设备"):detail==="NOTIFICATIONS"?uiText("消息通知") : surface==="BRIDGES" ? "Bridge" : surface==="NEW_BRIDGE" ? uiText("新建 Bridge") : surface==="NOTIFICATIONS" ? uiText("通知") : surface==="SETTINGS" ? uiText("设置") : surface==="RUNTIME" ? uiText("连接") : surface==="RECYCLE" ? uiText("已删除的 Bridge") : selected?.name??"Bridge";
  const topLevel = ["BRIDGES","NOTIFICATIONS","SETTINGS"].includes(surface)&&!detail&&!(surface==="NOTIFICATIONS"&&props.notificationDetail);
  const back = ()=>{if(detail)setDetail(null);else navigate(inReader?"BRIDGES":surface==="RECYCLE"||surface==="RUNTIME"?"SETTINGS":"BRIDGES");};
  return <NativeSurfaceContext.Provider value><NativeCopyProvider register={setCopyText}><MediaPreviewProvider><MotionConfig reducedMotion="user"><main className={`unified-workbench r2-router v3-surface-${surface.toLowerCase()}`} data-detail={!topLevel} aria-label={uiText("Agbrio 统一工作台")} style={{"--r2-master-width":`${width}px`} as React.CSSProperties}>
    <aside className="r2-rail"><div className="r2-brand"><BrandMark/>Agbrio</div>{nav(false)}<small className="r2-rail-caption">Bridge</small><div className="r2-rail-bridges">{items.slice(0,8).map(item=><button key={item.id} type="button" aria-current={inReader&&item.id===selected?.id?"page":undefined} onClick={()=>{props.onSelectWorkstream(item.id);navigate("WORKSPACE");}}>{item.name}</button>)}</div></aside>
    <section className="r2-workspace"><header className="r2-header" hidden={surface==="NOTIFICATIONS"&&props.notificationDetail}>
      {!topLevel&&<button type="button" className="r2-icon" aria-label={uiText("返回")} title={uiText("返回")} onClick={back}><ChevronLeft size={20}/></button>}<h1>{title}</h1>
      {surface==="BRIDGES"&&<><button type="button" className="r2-icon" aria-label={uiText("搜索 Bridge")} aria-expanded={searching} onClick={()=>setSearching(!searching)}><Search size={20}/></button><button type="button" className="r2-icon r2-accent" aria-label={uiText("新建 Bridge")} onClick={()=>navigate("NEW_BRIDGE")}><Plus size={20}/></button></>}
      {inReader&&copyText&&<CopyAction text={copyText} onError={()=>setActionError(uiText("复制未成功，可选择正文复制。"))}/>}
      {inReader&&<Dialog.Root open={menu} onOpenChange={setMenu}><Dialog.Trigger asChild><button type="button" className="r2-icon" aria-label={uiText("Bridge 操作")}><Ellipsis size={20}/></button></Dialog.Trigger><Dialog.Portal><Dialog.Overlay className="r2-menu-overlay"/><Dialog.Content className="r2-menu" aria-describedby={undefined}><Dialog.Title className="r2-sr-only">{uiText("Bridge 操作")}</Dialog.Title><button type="button" onClick={()=>{setMenu(false);props.onManageBindings?.();}}><Link2 size={18}/>{uiText("更换两端")}</button>{props.onRenameBridge&&selected&&<button type="button" onClick={()=>{setMenu(false);setRenameValue(selected.name);setRenaming(selected);setActionError("");}}><Pencil size={18}/>{uiText("重命名")}</button>}{props.onPinChange&&selected&&<button type="button" onClick={()=>{props.onPinChange?.(selected.id,!selected.pinned);setMenu(false);}}><CircleCheck size={18}/>{selected.pinned?uiText("取消置顶"):uiText("置顶")}</button>}{props.onLifecycleChange&&selected&&<button type="button" className="r2-danger" onClick={()=>{setMenu(false);removeBridge(selected);}}><Trash2 size={18}/>{uiText("删除 Bridge")}</button>}<Dialog.Close asChild><button type="button"><X size={18}/>{uiText("关闭")}</button></Dialog.Close></Dialog.Content></Dialog.Portal></Dialog.Root>}
    </header>
    {!online&&<p className="r2-offline" role="status">{uiText("连接中断 · 等待恢复")}</p>}
    <div className={`r2-stage${inReader?" r2-split":""}`}>
      {inReader&&<><aside className="r2-master"><small>{uiText("最近")}</small>{rows(true)}</aside><PanelDivider label={uiText("调整 Bridge 列表宽度")} value={width} min={240} max={390} step={10} onChange={value=>{setWidth(value);try{localStorage.setItem("aiwr.r2.master-width",String(value));}catch{/* optional preference */}}}/></>}
      <section ref={content} className="v3-workbench-main r2-content">
        {props.criticalNotice}{actionError&&<p role="alert">{uiText(actionError)}</p>}
        <div hidden={!inReader} className="r2-reader-mount">{props.bridgePanel}</div>
        {surface==="BRIDGES"&&<section className="r2-directory"><small>{uiText("最近")}</small>{searching&&<label className="r2-search"><Search size={18}/><input autoFocus aria-label={uiText("搜索 Bridge")} placeholder={uiText("搜索")} value={query} onChange={event=>setQuery(event.target.value)}/><button type="button" className="r2-icon" aria-label={uiText("清除搜索")} onClick={()=>setQuery("")}><X size={18}/></button></label>}{visible.length?<>{rows()}{visible.length>limit&&<button type="button" className="r2-load-more" onClick={()=>setLimit(value=>value+20)}>{uiText("更早的 Bridge")}</button>}</>:<div className="r2-empty"><Link2 size={32}/><h2>{query?uiText("没有匹配的 Bridge"):uiText("开始一个 Bridge")}</h2>{!query&&<button type="button" className="r2-action" onClick={()=>navigate("NEW_BRIDGE")}><Plus size={18}/>{uiText("新建 Bridge")}</button>}</div>}</section>}
        {surface==="NEW_BRIDGE"&&props.onCreateBridge&&<NewBridgeForm create={props.onCreateBridge} cancel={()=>navigate("BRIDGES")}/>}
        {surface==="SETTINGS"&&!detail&&<section className="r2-settings"><small>{uiText("连接")}</small><div className="r2-settings-group"><button type="button" onClick={()=>navigate("RUNTIME")}><Monitor size={20}/><span>Codex</span><ChevronRight size={18}/></button><button type="button" onClick={()=>{if(props.onOpenWebAccess)props.onOpenWebAccess();else setDetail("DEVICES");}}><Smartphone size={20}/><span>{uiText("设备")}</span><ChevronRight size={18}/></button></div><small>{uiText("偏好")}</small><div className="r2-settings-group"><LanguagePicker/><button type="button" onClick={()=>setDetail("NOTIFICATIONS")}><Bell size={20}/><span>{uiText("消息通知")}</span><ChevronRight size={18}/></button><button type="button" onClick={()=>navigate("RECYCLE")}><Trash2 size={20}/><span>{uiText("已删除的 Bridge")}</span><ChevronRight size={18}/></button></div></section>}
        {surface==="SETTINGS"&&!detail&&props.assistantPanel&&<section className="r2-settings"><div className="r2-settings-group"><button type="button" onClick={()=>setDetail("ASSISTANT")}><Bot size={20}/><span>{uiText("AI 助手")}</span><ChevronRight size={18}/></button></div></section>}
        {surface==="SETTINGS"&&!detail&&<section className="r2-settings r2-settings-about"><div className="r2-settings-group"><button type="button" onClick={()=>setDetail("ABOUT")}><Info size={20}/><span>{uiText("关于")}</span><ChevronRight size={18}/></button></div></section>}
        {surface==="SETTINGS"&&detail&&<section className="r2-settings">{detail==="ABOUT"?<><h2>Agbrio</h2><p>{uiText("Agent Bridge · 连接你的 Agent 对话")}</p><p className="r2-about-version">{uiText("版本")} {version} {uiText("· 界面")} {PHONE_LAYOUT_REVISION}</p><div className="r2-settings-group"><div className="r2-connection-row"><Monitor size={20}/><strong>{uiText("Windows 桌面")}</strong><span>{uiText("已支持")}</span></div><div className="r2-connection-row"><Smartphone size={20}/><strong>{uiText("手机 PWA")}</strong><span>{uiText("个人配对")}</span></div><div className="r2-connection-row"><Monitor size={20}/><strong>{uiText("Mac 共享连接")}</strong><span>{uiText("尚未验证")}</span></div></div><p>{uiText("Bridge 连接两端对话；通知可以直接回复原对话。")}</p><a href="mailto:geoffreyzjx@qq.com">{uiText("联系作者")}</a><button type="button" className="r2-diagnostic-copy" onClick={()=>{const report=displayDiagnostics();void (async()=>{try{await navigator.clipboard.writeText(report);setDisplayCopied(true);setDisplayReport("");}catch{setDisplayReport(report);setDisplayCopied(false);}})();}}><Copy size={18}/>{displayCopied?uiText("已复制显示诊断"):uiText("复制显示诊断")}</button>{displayReport&&<textarea aria-label={uiText("显示诊断")} readOnly value={displayReport} rows={6} onFocus={event=>event.currentTarget.select()}/>}</>:detail==="ASSISTANT"?props.assistantPanel:detail==="NOTIFICATIONS"?props.notificationSettings:props.devicePanel}</section>}
        {surface==="RUNTIME"&&<section className="r2-settings">{props.runtimeAddon}<small>{uiText("连接状态")}</small><div className="r2-settings-group">{props.runtime?.checks.filter(check=>["codex","browser-runtime","mobile-listener","router-host"].includes(check.id)).map(check=><div className="r2-connection-row" key={check.id}><strong>{uiText(check.label)}</strong><span>{check.state==="READY"?uiText("可用"):check.state==="CHECKING"?uiText("检查中"):uiText("待处理")}</span>{check.action&&<button type="button" className="r2-icon" aria-label={uiText(check.actionLabel??uiText("检查状态"))} title={uiText(check.actionLabel??uiText("检查状态"))} onClick={check.action}><ChevronRight size={18}/></button>}</div>)}</div></section>}
        {surface==="RECYCLE"&&<section className="r2-directory"><div className="r2-settings-group">{trash.map(item=><div key={item.id} className="r2-connection-row"><strong>{item.name}</strong><button type="button" disabled={actionBusy} className="r2-icon" aria-label={uiText("恢复 {0}", item.name)} onClick={()=>void changeLifecycle(item,"ACTIVE")}><Undo2 size={18}/></button></div>)}</div>{!trash.length&&<div className="r2-empty"><Trash2 size={28}/><h2>{uiText("没有已删除的 Bridge")}</h2></div>}</section>}
        {!topLevel&&!inReader&&!["NEW_BRIDGE","RUNTIME","RECYCLE","SETTINGS","NOTIFICATIONS"].includes(surface)&&<p role="status">{uiText("此入口已收起。")}<button type="button" onClick={()=>navigate("BRIDGES")}>{uiText("返回 Bridge")}</button></p>}
      </section>
    </div>
    {topLevel&&<div className="r2-mobile-navigation">{nav(true)}</div>}
    </section>
    <div className="r2-host-controls">{props.globalActions}</div>
    {removal.undoItem&&<div className="r2-toast" role="status">{uiText("已移除 Bridge")}<button type="button" onClick={removal.undo}>{uiText("撤销")}</button><button type="button" className="r2-icon" aria-label={uiText("关闭提示")} onClick={removal.dismiss}><X size={16}/></button></div>}
    <Dialog.Root open={Boolean(renaming)} onOpenChange={open=>{if(!open&&!actionBusy)setRenaming(null);}}><Dialog.Portal><Dialog.Overlay className="v3-role-dialog-overlay"/><Dialog.Content className="r2-confirmation" aria-describedby={undefined}><Dialog.Title>{uiText("重命名 Bridge")}</Dialog.Title><form onSubmit={event=>{event.preventDefault();if(!renaming||!props.onRenameBridge||pendingAction.current)return;pendingAction.current=true;setActionBusy(true);setActionError("");void props.onRenameBridge(renaming.id,renameValue.trim()).then(()=>setRenaming(null)).catch(()=>setActionError(uiText("名称未保存，请重试。"))).finally(()=>{pendingAction.current=false;setActionBusy(false);});}}><label>{uiText("名称")}<input autoFocus value={renameValue} maxLength={100} disabled={actionBusy} onChange={event=>setRenameValue(event.target.value)}/></label>{actionError&&<p role="alert">{uiText(actionError)}</p>}<footer><Dialog.Close asChild><button type="button" disabled={actionBusy}>{uiText("取消")}</button></Dialog.Close><button type="submit" className="r2-action" disabled={actionBusy||!renameValue.trim()}>{actionBusy?uiText("保存中…"):uiText("保存")}</button></footer></form></Dialog.Content></Dialog.Portal></Dialog.Root>
  </main></MotionConfig></MediaPreviewProvider></NativeCopyProvider></NativeSurfaceContext.Provider>;
}
