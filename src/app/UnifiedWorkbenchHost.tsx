import {readModels} from "../features/workbench/readModelCache";
import {CodexQuota,desktopQuotaApi,desktopClaudeQuotaApi} from "../features/workbench/CodexQuota";
import {DesktopUpdateSettings} from "../features/workbench/DesktopUpdateSettings";
import {desktopBridgeActivity} from "../features/workbench/bridgeActivity";
import {flushSync} from "react-dom";
import {NotificationDeliveryPanel} from "../features/workbench/NotificationDeliveryPanel";
import {CodexConnection} from "../features/workbench/CodexConnection";
import {desktopOverviewApi} from "../features/workbench/SettingsHome";
import {LocalSafetySettings} from "../features/workbench/LocalSafetySettings";
import {PiSettings} from "../features/workbench/PiSettings";
import {DesktopWebAccess} from "../features/workbench/DesktopWebAccess";
import {AssistantSettings} from "../features/workbench/AssistantSettings";
import {useDirectorySync} from "../features/workbench/useDirectorySync";
import { NotificationAssistantSettings, CodexNotifications } from "../features/workbench/CodexNotifications";
import { desktopNotificationApi } from "../features/workbench/notificationApi";
import { RoleBridgePanel } from "../features/workbench/RoleBridgePanel";
import { desktopRoleBridgeApi } from "../features/workbench/roleBridgeApi";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";

import { codexApi } from "../features/codex/api";
import type { DashboardProjection, HostEnvironmentStatus, WorkspaceSnapshot } from "../features/codex/types";
import { BridgeStatus } from "../features/workbench/BridgeStatus";
import { UnifiedWorkbench } from "../features/workbench/UnifiedWorkbench";
import type { WorkbenchItem, WorkbenchLifecycle } from "../features/workbench/models";
import type { WorkbenchSurface } from "../features/workbench/UnifiedWorkbench";

function lifecycleOf(workstream: WorkspaceSnapshot["workstreams"][number]): WorkbenchLifecycle {
  return workstream.trashedAt ? "TRASHED" : workstream.status === "ARCHIVED" ? "ARCHIVED" : "ACTIVE";
}

/**
 * The desktop host adapts only Router Core projections to the shared V3 view.
 * It deliberately never uses names, list order, or a browser window as routing
 * authority: every read/action below carries the selected Workstream or exact
 * persisted Endpoint identity back to the Core.
 */
export function UnifiedWorkbenchHost({initialSurface="BRIDGES"}:{initialSurface?:WorkbenchSurface}={}) {
  const [notificationDetail,setNotificationDetail]=useState(false);
  const [snapshot, setSnapshot] = useState<WorkspaceSnapshot | null>(()=>readModels.peek<WorkspaceSnapshot>("workspace:desktop")??null);
  // The workspace snapshot carries only the selected Workstream's active
  // Endpoints.  The dashboard projection is the separate Core-owned index
  // used to distinguish otherwise identical visible Workstream names.
  const [dashboard, setDashboard] = useState<DashboardProjection | null>(()=>readModels.peek<DashboardProjection>("directory:desktop")??null);
  const [selectedWorkstreamId, setSelectedWorkstreamId] = useState<string | null>(null);
  const readerSelectionIntent = useRef<string | null>(null);
  const [webAccessRequest,setWebAccessRequest]=useState(0);
  const [runtimeError, setRuntimeError] = useState<string | null>(null);
  const [codexConnected, setCodexConnected] = useState(false);
  const [codexConnectionBusy, setCodexConnectionBusy] = useState(false);
  const [codexConnectionDetail, setCodexConnectionDetail] = useState<string | null>(null);

  const [hostEnvironment, setHostEnvironment] = useState<HostEnvironmentStatus | null>(null);
  const candidateScopeGeneration = useRef(0);
  const [surface, setSurface] = useState<WorkbenchSurface>(initialSurface);
  const [roleMode, setRoleMode] = useState(false);
  const [bindingRequest, setBindingRequest] = useState(0);
  const refreshDashboard = useCallback(async () => {
    setDashboard(await codexApi.dashboardProjection());
  }, []);

  const refreshIndex = useCallback(async () => {
    const next = await codexApi.workspaceSnapshot();
    setSnapshot(next);
    setSelectedWorkstreamId((current) => current ?? next.selectedWorkstreamId ?? next.workstreams[0]?.id ?? null);
    void refreshDashboard().catch(() => setDashboard(null));
  }, [refreshDashboard]);

  useEffect(() => { void refreshIndex().catch((error) => setRuntimeError(String(error))); }, [refreshIndex]);
  useEffect(()=>readModels.subscribe("directory:desktop",()=>{const next=readModels.peek<DashboardProjection>("directory:desktop");if(next){setDashboard(next);setSnapshot(current=>current?{...current,workstreams:next.workstreams.map(item=>item.workstream)}:current);}}),[]);
  useEffect(()=>{if(!dashboard)return;const ids=new Set(dashboard.workstreams.filter(row=>!row.workstream.trashedAt&&row.workstream.status!=="ARCHIVED").map(row=>row.workstream.id));readModels.retain("bridge:",ids);for(const id of ids)desktopRoleBridgeApi.warmState?.(id);},[dashboard]);
  useDirectorySync(async()=>{
    const next=await codexApi.dashboardProjection();setDashboard(next);
    setSnapshot(current=>current?{...current,workstreams:next.workstreams.map(item=>item.workstream)}:current);
  });
  useEffect(() => {
    let stopped = false;
    let inFlight = false;
    let dirty = false;
    const unlisten: Array<() => void> = [];
    const refreshStatus = async () => {
      dirty = true;
      if (inFlight || stopped) return;
      inFlight = true;
      try {
        do {
          dirty = false;
          const status = await codexApi.status();
          if (!stopped) {
            setCodexConnected(status.connected);
            setCodexConnectionDetail(status.detail ?? null);
          }
        } while (dirty && !stopped);
      } catch (error) {
        if (!stopped) setRuntimeError(String(error));
      } finally { inFlight = false; }
    };
    // Events are hints; Core status stays authority. Subscribe before the first
    // read so startup completion cannot be lost between reading and listening.
    void Promise.allSettled([
      listen("codex-backend-connected", () => { void refreshStatus(); }),
      listen("codex-backend-disconnected", () => { void refreshStatus(); }),
    ]).then(results => {
      for (const result of results) {
        if (result.status === "fulfilled") {
          if (stopped) result.value(); else unlisten.push(result.value);
        }
      }
      if (!stopped) void refreshStatus();
    });
    return () => { stopped = true; unlisten.forEach(stop => stop()); };
  }, []);
  const refreshHostEnvironment = useCallback(async () => {
    try {
      const environment = await codexApi.hostEnvironmentStatus();
      setHostEnvironment(environment);
    } catch (error) { setRuntimeError(`Host 运行状态未确认：${String(error)}`); }
  }, []);
  useEffect(() => { void refreshHostEnvironment(); }, [refreshHostEnvironment]);

  const selected = snapshot?.workstreams.find((workstream) => workstream.id === selectedWorkstreamId) ?? null;
  const selectedCodexEndpoint = snapshot?.selectedWorkstreamId === selectedWorkstreamId ? snapshot.activeCodexEndpoint ?? null : null;
  // A Workstream bound to ChatGPT before its removal still names that endpoint.
  const selectedDecisionEndpoint = snapshot?.selectedWorkstreamId === selectedWorkstreamId ? snapshot.activeChatgptEndpoint ?? null : null;
  const refreshRuntime = useCallback(() => {
    void codexApi.status().then((status) => {
      setCodexConnected(status.connected);
      setCodexConnectionDetail(status.detail ?? null);
      setRuntimeError(status.detail ?? null);
    }).catch((error) => setRuntimeError(`Codex 状态未确认：${String(error)}`));
    void refreshHostEnvironment();
  }, [refreshHostEnvironment]);

  const connectCodexReadOnly = useCallback(async () => {
    if (codexConnectionBusy || codexConnected) return;
    setCodexConnectionBusy(true);
    setCodexConnectionDetail("正在建立 Router-owned Codex app-server 的只读连接；不会恢复、发送或取得 writer。");
    try {
      const requested = await codexApi.connect();
      setCodexConnected(requested.connected);
      setCodexConnectionDetail(requested.detail ?? null);
      for (let attempt = 0; !requested.connected && attempt < 120; attempt += 1) {
        await new Promise<void>((resolve) => window.setTimeout(resolve, 250));
        const status = await codexApi.status();
        setCodexConnected(status.connected);
        setCodexConnectionDetail(status.detail ?? null);
        if (status.connected) {
          setRuntimeError(null);
          return;
        }
        if (!status.connecting) throw new Error(status.detail ?? "Codex app-server 未建立连接");
      }
      if (!requested.connected) throw new Error("Codex app-server 连接超时；没有重试、恢复或发送对话。");
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setCodexConnected(false);
      setCodexConnectionDetail(message);
      setRuntimeError(`Codex 只读连接未完成：${message}`);
    } finally {
      setCodexConnectionBusy(false);
    }
  }, [codexConnected, codexConnectionBusy]);

  const loadSelected = useCallback(async (workstreamId: string) => {
    readerSelectionIntent.current = workstreamId;
    const selectionGeneration = ++candidateScopeGeneration.current;
    setSelectedWorkstreamId(workstreamId);
    setRuntimeError(null);
    const next = await codexApi.workstreamSnapshot(workstreamId);
    if (selectionGeneration !== candidateScopeGeneration.current) return;
    // A selected-workstream read owns its presentation, not the global sidebar
    // index. A slower response must not remove another already-listed choice.
    setSnapshot(current => current ? { ...next,
      projects: [...new Map([...current.projects, ...next.projects].map(row => [row.id, row])).values()],
      workstreams: [...new Map([...current.workstreams, ...next.workstreams].map(row => [row.id, row])).values()] } : next);
    return next;
  }, []);

  const selectWorkstream = useCallback(async (workstreamId: string) => {
    const workstream = snapshot?.workstreams.find((candidate) => candidate.id === workstreamId)??dashboard?.workstreams.find(item=>item.workstream.id===workstreamId)?.workstream;
    if (!workstream) throw new Error("要打开的工作区不在当前 Router 索引中。");
    // This is a presentation preference, not endpoint routing. Persist the
    // exact local Workstream identity so a restart cannot silently return to a
    // same-named predecessor.
    // Choose/clear the reader before a deleted predecessor can redirect
    // WORKSPACE back to the directory while selection IPC is pending.
    const [loaded]=await Promise.all([loadSelected(workstreamId),codexApi.selectWorkspace(workstream.projectId, workstreamId)]);
    return loaded;
  }, [loadSelected, snapshot?.workstreams,dashboard?.workstreams]);

  useEffect(() => {
    // A committed older selection may still have a passive effect pending after
    // a newer explicit selection. It must not choose the old reader again.
    if (selectedWorkstreamId && (!readerSelectionIntent.current || readerSelectionIntent.current === selectedWorkstreamId)) void loadSelected(selectedWorkstreamId).catch((error) => setRuntimeError(String(error)));
  // Loading belongs to a chosen exact workstream, not the changing snapshot object.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedWorkstreamId]);

  const dashboardByWorkstreamId = useMemo(() => new Map((dashboard?.workstreams ?? []).map((item) => [item.workstream.id, item])), [dashboard]);
  const indexedWorkstreams=useMemo(()=>{const entries=new Map<string,WorkspaceSnapshot["workstreams"][number]>();for(const row of [...(dashboard?.workstreams.map(item=>item.workstream)??[]),...(snapshot?.workstreams??[])]){const previous=entries.get(row.id);if(!previous||row.updatedAt>=previous.updatedAt)entries.set(row.id,row);}return [...entries.values()];},[dashboard?.workstreams,snapshot?.workstreams]);
  const items = useMemo<WorkbenchItem[]>(() => indexedWorkstreams.map((workstream) => {
    const dashboardItem = dashboardByWorkstreamId.get(workstream.id);
    const attentionItems = dashboardItem?.attentionItems.map((attention) => ({
      sourceId: attention.sourceId,
      kind: attention.kind,
      priority: attention.priority,
      message: attention.message,
      activityAt: attention.activityAt,
    })) ?? [];
    const attentionCount = attentionItems.length;
    // The workstream rail is often the owner's first way to distinguish
    // same-named work.  Showing only Codex made a completed pair look
    // ambiguous, so retain both display identities here.  The connection
    // surface remains the deliberate place for full exact IDs.
    const bindingLabels = dashboardItem ? [
      dashboardItem.chatgptEndpoint?.label ? `ChatGPT · ${dashboardItem.chatgptEndpoint.label}` : null,
      dashboardItem.codexEndpoint?.label ? `Codex · ${dashboardItem.codexEndpoint.label}` : null,
    ].filter((label): label is string => Boolean(label)) : [];
    const sourceLabel = !dashboardItem ? null : bindingLabels.length ? bindingLabels.join("　/　") : "尚未绑定对话";
    const bindingStatus = !dashboardItem ? workstream.status
      : dashboardItem.codexEndpoint
        ? "Codex 已绑定"
        : dashboardItem.chatgptEndpoint
          ? "ChatGPT 已不再支持"
          : workstream.status;
    return {
      id: workstream.id,
      name: workstream.name,
      projectName: snapshot?.projects.find((project) => project.id === workstream.projectId)?.name??dashboardItem?.projectName,
      lifecycle: lifecycleOf(workstream),
      pinned: Boolean(workstream.pinnedAt),
      updatedAt: workstream.updatedAt,
      lastMessageAt: dashboardItem?.lastMessageAt,
      attentionCount,
      attentionItems,
      sourceLabel,
      statusLabel: attentionItems[0]?.message ?? bindingStatus,
    };
  }), [indexedWorkstreams,dashboardByWorkstreamId, snapshot]);

  const changeLifecycle = async (workstreamId: string, lifecycle: WorkbenchLifecycle) => {
    if (lifecycle === "ARCHIVED") await codexApi.archiveWorkstream(workstreamId);
    else if (lifecycle === "TRASHED") await codexApi.trashWorkstream(workstreamId);
    else await codexApi.restoreWorkstream(workstreamId);
    await refreshIndex();
    if (workstreamId === selectedWorkstreamId) await loadSelected(workstreamId);
  };

  const requestBinding = () => {setSurface("WORKSPACE");setBindingRequest(value => value + 1);};
  const statusFor = <BridgeStatus name={selected?.name || "AI Work Router"} recoveryError={runtimeError} showErrors decision={selectedDecisionEndpoint} execution={selectedCodexEndpoint} executionConnected={codexConnected} nextAction={items.find(item => item.id === selectedWorkstreamId)?.attentionItems?.[0]?.message} onManage={requestBinding} />;
  return <><UnifiedWorkbench
    notificationDetail={notificationDetail} notificationSettings={<>{desktopNotificationApi.delivery&&<NotificationDeliveryPanel api={desktopNotificationApi.delivery}/>}<NotificationAssistantSettings api={desktopNotificationApi}/></>}
    onRenameBridge={async(id,name)=>{await codexApi.renameBridge(id,name);await refreshIndex();}}
    onManageBindings={requestBinding}
    onCreateBridge={async name=>{const id=await codexApi.createBridge(name);const exact=await codexApi.workstreamSnapshot(id);const created=exact.workstreams.find(item=>item.id===id);if(!created)throw Error("新建 Bridge 暂时不可读取，请返回列表检查。");await codexApi.selectWorkspace(created.projectId,id);await refreshIndex();await loadSelected(id);flushSync(()=>setSurface("WORKSPACE"));setBindingRequest(v=>v+1);}}
    quotaPanel={<><CodexQuota api={desktopQuotaApi} active={surface==="SETTINGS"}/><CodexQuota provider="Claude Code" api={desktopClaudeQuotaApi} active={surface==="SETTINGS"}/></>} updatePanel={<DesktopUpdateSettings/>} bridgeActivityApi={desktopBridgeActivity} items={items}
    selectedWorkstreamId={selectedWorkstreamId}
    assistantPanel={<AssistantSettings/>} piSettingsPanel={<PiSettings/>} codexConnectionPanel={<CodexConnection/>} runtimeProtectionPanel={<LocalSafetySettings/>} overviewApi={desktopOverviewApi} onOpenWebAccess={()=>setWebAccessRequest(v=>v+1)} watchApi={desktopNotificationApi} globalActions={<><CodexNotifications onDetailChange={setNotificationDetail} api={desktopNotificationApi} workbenchPage={{active:surface==="NOTIFICATION_LOG",open:()=>setSurface("NOTIFICATION_LOG")}}/><DesktopWebAccess openRequest={webAccessRequest}/></>}
    bridgePanel={<>{!roleMode && selected ? statusFor : null} {selectedWorkstreamId && <RoleBridgePanel key={selectedWorkstreamId} workstreamId={selectedWorkstreamId} workstreamName={items.find(item => item.id === selectedWorkstreamId)?.name} api={desktopRoleBridgeApi} readerActive={surface==="WORKSPACE"} onOpenConnection={()=>setSurface("RUNTIME")} bindingRequest={bindingRequest} externalBindingEntry={Boolean(selected)} onModeChange={setRoleMode} onBindingsChanged={() => { void refreshIndex(); }} />}</>}
    runtime={{ checks: [
      { id: "router-host", label: "Router 电脑程序", state: hostEnvironment?.host === "ONLINE" ? "READY" : "CHECKING", detail: hostEnvironment?.host === "ONLINE" ? (snapshot ? "Router Host 与本地工作区已读取" : "当前 Tauri Host 正在响应诊断请求") : "正在读取 Host 状态", action: refreshRuntime, actionLabel: "检查状态" },
      { id: "mobile-listener", label: "手机访问入口", state: hostEnvironment?.mobile.startsWith("AVAILABLE") ? "READY" : hostEnvironment ? "UNAVAILABLE" : "CHECKING", detail: hostEnvironment?.mobile ?? "正在读取 listener 状态", action: refreshRuntime, actionLabel: "检查连接" },
      {
        id: "codex",
        label: "Codex 后端",
        state: codexConnected ? "READY" : codexConnectionBusy ? "CHECKING" : "UNAVAILABLE",
        detail: codexConnected
          ? "Router-owned app-server 已连接；具体对话控制权另行判断。"
          : codexConnectionBusy
          ? "正在建立 Router-owned Codex app-server 的只读连接；不会恢复、发送或取得 writer。"
          : codexConnectionDetail
          ? `Codex app-server 未连接：${codexConnectionDetail}`
          : "Codex app-server 尚未连接；可建立只读连接，随后再检查精确绑定线程。",
        action: codexConnected ? refreshRuntime : () => void connectCodexReadOnly(),
        actionLabel: codexConnected ? "检查状态" : "连接 Codex（只读）",
      },
      { id: "device-notifications", label: "本设备通知", state: "UNAVAILABLE", detail: "按设备通知状态的官方读取能力尚未接入；不会用静态订阅状态替代。" },
    ], onOpenDiagnostics: refreshRuntime }}
    surface={surface}
    onSurfaceChange={setSurface}
    onSelectWorkstream={(id) => { setSurface("WORKSPACE"); void selectWorkstream(id).catch((error) => setRuntimeError(String(error))); }}
    onPinChange={(id, pinned) => void codexApi.setWorkstreamPinned(id, pinned).then(refreshIndex).catch((error) => setRuntimeError(`置顶未保存：${String(error)}`))}
    onLifecycleChange={(id, lifecycle) => changeLifecycle(id, lifecycle)}
  /></>;
}
