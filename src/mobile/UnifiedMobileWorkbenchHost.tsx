import {readModels,isSupersededRead} from "../features/workbench/readModelCache";
import {AssistantSettings,mobileAssistantSettingsApi} from "../features/workbench/AssistantSettings";
import {CodexQuota,mobileQuotaApi,mobileClaudeQuotaApi} from "../features/workbench/CodexQuota";
import {webBridgeActivity} from "../features/workbench/bridgeActivity";
import {bridgeNotificationRoute,BRIDGE_NOTIFICATION_MESSAGE} from "./bridgeNotificationRoute";
import {t as uiText,useLanguage} from "../i18n";
import {flushSync} from "react-dom";
import {useDirectorySync} from "../features/workbench/useDirectorySync";
import {NotificationDeliveryPanel} from "../features/workbench/NotificationDeliveryPanel";
import {WebSessionLogout} from "./WebLogin";
import { NotificationAssistantSettings, CodexNotifications } from "../features/workbench/CodexNotifications";
import { webNotificationApi } from "../features/workbench/notificationApi";
import { RoleBridgePanel } from "../features/workbench/RoleBridgePanel";
import { mobileRoleBridgeApi } from "./roleBridgeApi";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { BridgeStatus } from "../features/workbench/BridgeStatus";
import { UnifiedWorkbench, type WorkbenchSurface } from "../features/workbench/UnifiedWorkbench";
import type { WorkbenchItem, WorkbenchLifecycle } from "../features/workbench/models";

import { mobileApi, type MobileWorkstream } from "./api";
import { disableWebPush, enableWebPush, pushSetupState, type PushSetupState } from "./push";
import { request } from "./api";
// Transport T7 read; a Host without it leaves the settings card generic.
const computerIdentity=()=>request<{hostName:string|null}>("/computer-identity");

// Mobile remains a presentation client; it cannot become a second provider writer.

type MobileSnapshot = Awaited<ReturnType<typeof mobileApi.workstream>>;

function lifecycleOf(workstream: { status: string; trashedAt?: number | null }): WorkbenchLifecycle {
  return workstream.trashedAt ? "TRASHED" : workstream.status === "ARCHIVED" ? "ARCHIVED" : "ACTIVE";
}

function isMobileAccessAuthenticationError(cause: unknown) {
  return typeof cause === "object"
    && cause !== null
    && "status" in cause
    && ((cause as { status?: unknown }).status === 401 || (cause as { status?: unknown }).status === 403);
}

function MobileRuntimeUnavailable({ onRetry, accessAuthenticationRequired }: { onRetry: () => void; accessAuthenticationRequired: boolean }) {
 useLanguage();
  const goBack = () => window.history.back();
  if (accessAuthenticationRequired) {
    return <main className="v3-mobile-runtime-unavailable" aria-label={uiText("手机访问需要重新认证")}>
      <header><button type="button" onClick={goBack}>{uiText("‹ 返回")}</button><h1>{uiText("手机访问")}</h1></header>
      <section><span>{uiText("访问会话需要重新认证")}</span><h2>{uiText("需要重新登录手机访问")}</h2><p>{uiText("Cloudflare Access 会话已失效，或当前访问尚未获得授权。Router 和已保存的工作不会因此被删除。")}</p><p className="v3-mobile-runtime-detail">{uiText("这不表示电脑上的 Router 已关闭。请不要反复刷新。")}</p><article><h3>{uiText("下一步")}</h3><p>{uiText("重新打开手机访问页，按 Cloudflare Access 的正常登录流程完成认证后，再返回这里。")}</p><p>{uiText("认证完成前，手机不能读取新内容或提交操作。")}</p></article></section>
      <footer><button type="button" className="v3-primary" onClick={onRetry}>{uiText("重新打开手机访问页")}</button><button type="button" onClick={goBack}>{uiText("返回仍保留的页面")}</button></footer>
    </main>;
  }
  return <main className="v3-mobile-runtime-unavailable" aria-label={uiText("手机端运行状态不可用")}>
    <header><button type="button" onClick={goBack}>{uiText("‹ 返回")}</button><h1>{uiText("运行状态")}</h1></header>
    <section><span>{uiText("暂时连不上 Router")}</span><h2>{uiText("无法读取 Router 状态")}</h2><p>{uiText("手机不能读取新内容，也不能发送消息。此前已显示的内容不会被空白覆盖。")}</p><p className="v3-mobile-runtime-detail">{uiText("Router 状态请求未成功；不会把它解释为电脑已关闭。")}</p><article><h3>{uiText("在 Windows 上检查 AI Work Router")}</h3><p>{uiText("确认“运行环境”显示可用后，再回到手机刷新。")}</p><p>{uiText("手机不能直接启动一台已关闭的电脑。")}</p></article></section>
    <footer><button type="button" className="v3-primary" onClick={onRetry}>{uiText("重新连接")}</button><button type="button" onClick={goBack}>{uiText("返回仍保留的页面")}</button></footer>
  </main>;
}

/** The mobile host is a thin authenticated projection over the same Router
 * commands used by desktop. It has no provider, browser, or SQLite access. */
export function UnifiedMobileWorkbenchHost({initialSurface="BRIDGES"}:{initialSurface?:WorkbenchSurface}={}) {
  useLanguage();
  const [notificationDetail,setNotificationDetail]=useState(false);
  const [index, setIndex] = useState<MobileWorkstream[]>(()=>readModels.peek<MobileWorkstream[]>("directory:mobile")??[]);
  const [snapshot, setSnapshot] = useState<MobileSnapshot | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const selectedWorkstreamRef = useRef<string | null>(null);
  const [pushSetup, setPushSetup] = useState<PushSetupState | "CHECKING">("CHECKING");
  const [routerSubscriptionCount, setRouterSubscriptionCount] = useState<number | null>(null);
  const [pushBusy, setPushBusy] = useState(false);
  const [pushPanelExpanded, setPushPanelExpanded] = useState(false);
  const [error, setErrorState] = useState<string | null>(null);
  const setError=useCallback((value:string|null)=>{if(value!==null&&isSupersededRead(value))return;setErrorState(value);},[]);
  const [accessAuthenticationRequired, setAccessAuthenticationRequired] = useState(false);
  const [surface, setSurface] = useState<WorkbenchSurface>(()=>bridgeNotificationRoute(window.location.href,window.location.origin)?"WORKSPACE":initialSurface);
  const [bridgeEntryRequest,setBridgeEntryRequest]=useState(0);
  const [roleMode, setRoleMode] = useState(false);
  const [bindingRequest, setBindingRequest] = useState(0);
  const roleApi = useMemo(() => selectedId ? mobileRoleBridgeApi(selectedId) : null, [selectedId]);
  const indexLoadGeneration=useRef(0),loadGeneration=useRef(0);
  const [notificationTarget,setNotificationTarget] = useState(() => {
    const query = new URLSearchParams(window.location.search);
    return {
      workstreamId: query.get("workstream")?.trim() || null,
      replyId: query.get("reply")?.trim() || null,
    };
  });
  useEffect(()=>{
    const receive=(event:MessageEvent)=>{if(event.data?.type!==BRIDGE_NOTIFICATION_MESSAGE||typeof event.data.target!=="string"||event.origin&&event.origin!==window.location.origin)return;const route=bridgeNotificationRoute(event.data.target,window.location.origin);if(!route)return;
      indexLoadGeneration.current++;loadGeneration.current++;setNotificationTarget(route);setSelectedId(null);setSnapshot(null);setSurface("WORKSPACE");setBridgeEntryRequest(v=>v+1);history.replaceState({...history.state,aiwrBridgeEntry:{id:route.workstreamId,url:new URL(event.data.target,window.location.origin).href}},"",event.data.target);event.ports?.[0]?.postMessage({accepted:true});event.ports?.[0]?.close?.();
    };navigator.serviceWorker?.addEventListener("message",receive);return()=>navigator.serviceWorker?.removeEventListener("message",receive);
  },[]);

  const refreshPushSetup = useCallback(async () => {
    const [browserState, routerState] = await Promise.all([
      pushSetupState(),
      mobileApi.pushStatus(),
    ]);
    setPushSetup(browserState);
    setRouterSubscriptionCount(routerState.activeSubscriptionCount);
  }, []);

  useEffect(() => {
    void refreshPushSetup().catch((cause) => {
      setPushSetup("CHECKING");
      setError(`手机通知状态未读取：${String(cause)}`);
    });
  }, [refreshPushSetup]);

  const enablePhoneNotifications = async () => {
    if (pushBusy) return;
    setPushBusy(true);
    try {
      await enableWebPush();
      await refreshPushSetup();
      setError(null);
    } catch (cause) {
      setError(`手机通知未开启：${String(cause)}`);
      await refreshPushSetup().catch(() => setPushSetup("CHECKING"));
    } finally {
      setPushBusy(false);
    }
  };

  const disablePhoneNotifications = async () => {
    if (pushBusy) return;
    setPushBusy(true);
    try {
      await disableWebPush();
      await refreshPushSetup();
      setError(null);
    } catch (cause) {
      setError(`手机通知未关闭：${String(cause)}`);
    } finally {
      setPushBusy(false);
    }
  };

  const refreshIndex = useCallback(async () => {
    const ticket=++indexLoadGeneration.current;
    let next:MobileWorkstream[];try{next=await mobileApi.workstreams();}catch(cause){if(ticket!==indexLoadGeneration.current)return;throw cause;}
    if(ticket!==indexLoadGeneration.current)return;
    setAccessAuthenticationRequired(false);
    setIndex(next);
    if (notificationTarget.replyId && !notificationTarget.workstreamId) {
      setSelectedId(null);
      setError("通知回复缺少精确工作区 ID，未选择其他工作区。");
      return;
    }
    if (notificationTarget.workstreamId) {
      if (!next.some((item) => item.id === notificationTarget.workstreamId&&(!item.status||lifecycleOf({status:item.status,trashedAt:item.trashedAt})==="ACTIVE"))) {
        setSelectedId(null);
        setSnapshot(null);
        setError("通知指定的工作区当前不可用，未回退到其他工作区。");
        return;
      }
      setSelectedId(notificationTarget.workstreamId);
      return;
    }
    setSelectedId((current) => current ?? next[0]?.id ?? null);
  }, [notificationTarget]);

  useEffect(()=>readModels.subscribe("directory:mobile",()=>{const next=readModels.peek<MobileWorkstream[]>("directory:mobile");if(next)setIndex(next);}),[]);
  useEffect(()=>{if(!index.length&&!readModels.peek("directory:mobile"))return;const ids=new Set(index.filter(row=>!row.trashedAt&&row.status!=="ARCHIVED").map(row=>row.id));readModels.retain("bridge:",ids);for(const id of ids)mobileRoleBridgeApi(id).warmState?.(id);},[index]);
  useDirectorySync(async()=>{setIndex(await mobileApi.refreshWorkstreams());},{onMount:true});

  const load = useCallback(async (workstreamId: string) => {
    const ticket=++loadGeneration.current;
    try {
    setSelectedId(workstreamId);
    setError(null);
    const nextSnapshot=await mobileApi.workstream(workstreamId);
    if(ticket!==loadGeneration.current)return;
    if(nextSnapshot.selectedWorkstreamId!==workstreamId)throw Error("通知指定的工作区当前不可用，未回退到其他工作区。");
    setSnapshot(nextSnapshot);
    return nextSnapshot;
    }catch(cause){if(ticket!==loadGeneration.current)return;throw cause;}
  }, [setError]);

  useEffect(() => {
    void refreshIndex().catch((cause) => {
      setAccessAuthenticationRequired(isMobileAccessAuthenticationError(cause));
      setError(String(cause));
    });
  }, [refreshIndex]);
  useEffect(() => { if (selectedId) void load(selectedId).catch((cause) => setError(String(cause))); }, [load, selectedId]);
  useEffect(() => { selectedWorkstreamRef.current = selectedId; }, [selectedId]);
  const selected = snapshot?.workstreams.find((item) => item.id === selectedId) ?? null;
  const items = useMemo<WorkbenchItem[]>(() => index.map((item) => {
    const detailed = snapshot?.workstreams.find((candidate) => candidate.id === item.id);
    return {
      id: item.id,
      name: item.name,
      projectName: item.projectName ?? (detailed ? snapshot?.projects.find((project) => project.id === detailed.projectId)?.name : undefined),
      lifecycle: item.status ? lifecycleOf({status:item.status,trashedAt:item.trashedAt}) : detailed ? lifecycleOf(detailed) : "ACTIVE",
      pinned: Boolean(item.pinnedAt ?? detailed?.pinnedAt),
      updatedAt: item.updatedAt ?? detailed?.updatedAt ?? item.lastActivityAt ?? null,
      lastMessageAt: item.lastMessageAt,
      attentionCount: item.attentionCount,
      attentionItems: item.attentionItems,
      sourceLabel: item.sourceLabel || (item.sourceLabel===undefined?item.bindingSummary:undefined),
      // A provider run state is not an owner action. The Inbox renders each
      // exact attention item instead of letting this summary masquerade as the
      // currently selected reply.
      statusLabel: item.attentionItems[0]?.message ?? item.chatgptStatus ?? item.codexStatus ?? "状态待读取",
    };
  }), [index, snapshot]);
  const changeLifecycle = async (workstreamId: string, lifecycle: WorkbenchLifecycle) => {
    if (lifecycle === "ARCHIVED") await mobileApi.archiveWorkstream(workstreamId);
    else if (lifecycle === "TRASHED") await mobileApi.trashWorkstream(workstreamId);
    else await mobileApi.restoreWorkstream(workstreamId);
    await refreshIndex();
    await load(workstreamId);
  };
  const requestBinding = () => {setSurface("WORKSPACE");setBindingRequest(value => value + 1);};
  if (error && !snapshot && !index.length) return <MobileRuntimeUnavailable accessAuthenticationRequired={accessAuthenticationRequired} onRetry={() => {
    if (accessAuthenticationRequired) {
      window.location.reload();
      return;
    }
    setError(null);
    void refreshIndex().catch((cause) => {
      setAccessAuthenticationRequired(isMobileAccessAuthenticationError(cause));
      setError(String(cause));
    });
  }} />;

  const phonePushReady = pushSetup === "SUBSCRIBED" && (routerSubscriptionCount ?? 0) > 0;
  const phonePushStatus = pushSetup === "CHECKING"
    ? uiText("正在核对这台手机与 Router 的通知订阅…")
    : phonePushReady
      ? uiText("已开启：Router 会为未来新出现的完整回复提交通知。推送服务没有手机弹窗回执；可用下方“测试真实回复通知”核对同一展示路径。")
      : pushSetup === "DENIED"
        ? uiText("此浏览器已拒绝通知权限。请在手机的站点设置中允许通知后再试。")
        : pushSetup === "UNSUPPORTED"
          ? uiText("当前打开方式不支持网站通知。请在受支持的手机浏览器或已添加到主屏幕的 Web App 中打开。")
          : pushSetup === "UPDATING"
            ? uiText("现有订阅已保留，但真实回复通知组件仍在更新。Router 不会把基础测试通知当成业务通知已就绪；请保持此 PWA 打开片刻后刷新状态。")
          : pushSetup === "SUBSCRIBED"
            ? uiText("浏览器显示已订阅，但 Router 尚未收到有效订阅；不会把它当成已开启。请重新开启一次。")
            : uiText("尚未开启。现在开启后，Router 只推送未来的新回复。");
  const phonePushPanel = <section className={`v3-mobile-push-card${phonePushReady ? " is-ready" : ""}`} aria-label={uiText("手机通知")}>
    <div><span>{uiText("手机通知")}</span><h2>{phonePushReady ? uiText("未来回复会通知你") : uiText("开启未来回复通知")}</h2><p>{uiText(phonePushStatus)}</p></div>
    <div className="v3-mobile-push-actions">
      {!phonePushReady && <button type="button" className="v3-primary" disabled={pushBusy || pushSetup === "CHECKING" || pushSetup === "UNSUPPORTED" || pushSetup === "DENIED" || pushSetup === "UPDATING"} onClick={() => void enablePhoneNotifications()}>{pushSetup === "UPDATING" ? uiText("正在更新通知组件…") : pushBusy ? uiText("正在开启…") : uiText("开启本机通知")}</button>}
      {phonePushReady && <><button type="button" disabled={pushBusy} onClick={() => void mobileApi.testPush().catch((cause) => setError(uiText("测试通知没有送达：{0}", String(cause))))}>{uiText("发送测试通知")}</button><button type="button" disabled={pushBusy} onClick={() => void mobileApi.testReplyPush().catch((cause) => setError(uiText("真实回复通知测试没有送达：{0}", String(cause))))}>{uiText("测试真实回复通知")}</button><button type="button" disabled={pushBusy} onClick={() => void disablePhoneNotifications()}>{pushBusy ? uiText("正在关闭…") : uiText("关闭本机通知")}</button></>}
      <button type="button" disabled={pushBusy} onClick={() => void refreshPushSetup().catch((cause) => setError(uiText("手机通知状态未刷新：{0}", String(cause))))}>{uiText("刷新状态")}</button>
      {phonePushReady && <button type="button" onClick={() => setPushPanelExpanded(false)}>{uiText("收起通知设置")}</button>}
    </div>
  </section>;

  // Notification setup is explicit. A transient capability check must never
  // consume the first phone viewport or push a focused exact reply below the
  // fixed Reader dock; the workspace menu remains the single clear entry.
  const showPhonePushPanel = pushPanelExpanded;
  return <>{error && <p role="alert" className="v3-mobile-error">{uiText(error)}</p>}<UnifiedWorkbench computerIdentity={computerIdentity} onRefreshConversationDirectory={async()=>{const ticket=++indexLoadGeneration.current;const next=await mobileApi.refreshWorkstreams();if(ticket===indexLoadGeneration.current)setIndex(next);}} assistantPanel={<AssistantSettings api={mobileAssistantSettingsApi}/>} quotaPanel={<><CodexQuota api={mobileQuotaApi} active={surface==="SETTINGS"}/><CodexQuota provider="Claude Code" api={mobileClaudeQuotaApi} active={surface==="SETTINGS"}/></>} notificationDetail={notificationDetail}  watchApi={webNotificationApi} notificationSettings={<><div data-expanded={showPhonePushPanel}>{phonePushPanel}</div>{webNotificationApi.delivery&&<NotificationDeliveryPanel api={webNotificationApi.delivery}/>}<NotificationAssistantSettings api={webNotificationApi}/></>} devicePanel={<WebSessionLogout/>} onRenameBridge={async(id,name)=>{await mobileApi.renameBridge(id,name);await refreshIndex();}} onManageBindings={requestBinding} onCreateBridge={async name=>{const id=await mobileApi.createBridge(name);await refreshIndex();await load(id);flushSync(()=>setSurface("WORKSPACE"));setBindingRequest(v=>v+1);}}
    bridgeActivityApi={webBridgeActivity} items={items} selectedWorkstreamId={selectedId}

    globalActions={<CodexNotifications onDetailChange={setNotificationDetail} api={webNotificationApi} workbenchPage={{active:surface==="NOTIFICATION_LOG",open:()=>setSurface("NOTIFICATION_LOG")}}/>}
    bridgePanel={<>{!roleMode && selected ? <BridgeStatus name={selected?.name || "AI Work Router"} recoveryError={error} decision={snapshot?.activeChatgptEndpoint} execution={snapshot?.activeCodexEndpoint} nextAction={items.find(item => item.id === selectedId)?.attentionItems?.[0]?.message} onManage={requestBinding} /> : null} {selectedId && roleApi && <RoleBridgePanel key={selectedId} workstreamId={selectedId} workstreamName={items.find(item => item.id === selectedId)?.name} api={roleApi} readerActive={surface==="WORKSPACE"} entryRequest={bridgeEntryRequest} bindingRequest={bindingRequest} externalBindingEntry={Boolean(selected)} onModeChange={setRoleMode} />}</>}
    runtime={{ checks: [{ id: "router-core", label: "Router Core", state: snapshot ? "READY" : "CHECKING", detail: snapshot ? "已通过受认证的手机语义 API 读取当前工作区" : "正在读取受认证的手机语义 API" }, { id: "providers", label: "Provider 连接", state: "WARNING", detail: "按选中精确工作区读取；手机端不会直接连接 Provider" }] }}
    surface={surface} onSurfaceChange={setSurface}

    onSelectWorkstream={(id) => { setNotificationTarget({workstreamId:null,replyId:null}); setSurface("WORKSPACE"); void load(id).catch((cause) => setError(String(cause))); }}

    onPinChange={(id, pinned) => void mobileApi.setWorkstreamPinned(id, pinned).then(refreshIndex).catch((cause) => setError(`置顶未保存：${String(cause)}`))}

    onLifecycleChange={(id, lifecycle) => changeLifecycle(id, lifecycle)}

  /></>;
}
