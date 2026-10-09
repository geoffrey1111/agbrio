import {readModels} from "../features/workbench/readModelCache";
import {CodexQuota,desktopQuotaApi} from "../features/workbench/CodexQuota";
import {DesktopUpdateSettings} from "../features/workbench/DesktopUpdateSettings";
import {desktopBridgeActivity} from "../features/workbench/bridgeActivity";
import {flushSync} from "react-dom";
import {NotificationDeliveryPanel} from "../features/workbench/NotificationDeliveryPanel";
import {SharedCodexConnection} from "../features/workbench/SharedCodexConnection";
import {DesktopWebAccess} from "../features/workbench/DesktopWebAccess";
import {AssistantSettings} from "../features/workbench/AssistantSettings";
import {useDirectorySync} from "../features/workbench/useDirectorySync";
import { NotificationAssistantSettings, CodexNotifications } from "../features/workbench/CodexNotifications";
import { desktopNotificationApi } from "../features/workbench/notificationApi";
import { RoleBridgePanel } from "../features/workbench/RoleBridgePanel";
import { desktopRoleBridgeApi } from "../features/workbench/roleBridgeApi";
import { ObservationAttachmentSelection } from "../features/workbench/ObservationAttachmentSelection";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { MarkdownMessage } from "../features/codex/MarkdownMessage";
import { codexApi } from "../features/codex/api";
import type { BoundChatGptProviderSurfaceStatus, ChatGptConversationHistory, CodexStructuredRequest, DashboardProjection, ExistingCodexThreadCandidate, ExplicitChatGptBindingCandidate, ExternalProjectLink, FeedEvent, HandoffReviewSession, HostEnvironmentStatus, ProviderRunStatus, ReplyObservation, RouterEndpoint, UnprojectedThreadStart, WorkspaceSnapshot, WorkstreamReviewResult } from "../features/codex/types";
import { BridgeStatus, securityRecoveryError } from "../features/workbench/BridgeStatus";
import { LEGACY_WORKBENCH_DETAILS, UnifiedWorkbench } from "../features/workbench/UnifiedWorkbench";
import { ProjectConnectionPanel, type ConnectionChoice, type ConnectionScreen, type ExplicitChatGptConfirmationState } from "../features/workbench/ProjectConnectionPanel";
import type { ExactReply, GoalPresentation, HandoffPresentation, WorkbenchItem, WorkbenchLifecycle } from "../features/workbench/models";
import type { WorkbenchSurface } from "../features/workbench/UnifiedWorkbench";
import { useDraftAutosaveProjection } from "../features/workbench/useDraftAutosaveProjection";
import { useResultScopedDraftAutosave } from "../features/workbench/useResultScopedDraftAutosave";

// The retired Router-owned carrier remains disabled.  The separately approved
// normal-Chrome observer may only read a unique existing exact conversation;
// it has no navigation, setup, attachment, or provider-write capability.
const CHATGPT_ROUTER_OWNED_CARRIER_PAUSED = false;
const CHATGPT_NORMAL_BROWSER_OBSERVER_ENABLED = true;
// These delivery categories fail before Router writes into a ChatGPT composer.
// They may offer the owner the exact default-browser destination, but never a
// substitute target or a Router retry.
const CHATGPT_PREWRITE_HANDOFF_FAILURE_CODES = new Set([
  "CHATGPT_AUTH_OR_COMPOSER_REQUIRED",
  "CHATGPT_ACCOUNT_SECURITY_REQUIRED",
  "CHATGPT_CONVERSATION_ID_INVALID",
  "CHATGPT_CONVERSATION_NOT_OPEN",
  "CHATGPT_EXACT_IDENTITY_MISMATCH",
  "CHATGPT_GENERATING",
  "CHATGPT_HUMAN_DRAFT_PRESENT",
  "CHATGPT_ROUTER_PROFILE_IN_USE",
  "CHATGPT_ROUTER_PROFILE_REQUIRED",
]);

function lifecycleOf(workstream: WorkspaceSnapshot["workstreams"][number]): WorkbenchLifecycle {
  return workstream.trashedAt ? "TRASHED" : workstream.status === "ARCHIVED" ? "ARCHIVED" : "ACTIVE";
}

function attachmentRelayExplanation(integrityStatus?: string | null, warnings: string[] = []) {
  if (integrityStatus === "VERIFIED") return "已验证；可在审阅时选择转发。";
  const warning = warnings.join(" ").toLowerCase();
  if (warning.includes("local path was not found") || warning.includes("cannot be relayed")) return "本地文件已不存在，不能随这次交接转发。";
  if (integrityStatus === "MISMATCH") return "文件校验不一致，不能随这次交接转发。";
  return "此附件当前不可安全转发。";
}

function attachmentCanBeRelayed(integrityStatus?: string | null) {
  return integrityStatus === "VERIFIED";
}

/** This status projection is deliberately record-only.  Showing the three
 * durable timestamps gives an owner context for a terminal state without
 * implying that Router reread a provider page. */
function providerRunTimestamp(value?: number | null) {
  return value ? new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(value) : "Router 尚未记录";
}

function isExactConfirmedChatGptEndpoint(endpoint: RouterEndpoint | null | undefined, workstreamId: string, externalId: string) {
  return endpoint?.provider === "CHATGPT" && endpoint.workstreamId === workstreamId && endpoint.externalId === externalId;
}

/**
 * Projects one Inbox delivery attention item back to its durable Router
 * record. This deliberately does not use recency, current bindings, or a
 * matching title: a delivery failure may itself belong to an older binding.
 */
function handoffFromExactAttention(snapshot: WorkspaceSnapshot, workstreamId: string, handoffId: string): HandoffPresentation | null {
  const record = snapshot.handoffs.find((candidate) => candidate.id === handoffId && candidate.workstreamId === workstreamId);
  if (!record) return null;
  return {
    id: record.id,
    handoffId: record.id,
    status: record.status,
    direction: record.direction,
    message: record.approvedText,
    canApprove: false,
    canSend: false,
    deliveryCode: record.errorCode ?? null,
    deliveryDetail: record.errorMessage ?? null,
    candidates: [{ id: record.id, label: "收件箱指定的精确交付记录", text: "", sourceLabel: "Router 已保存的 Handoff" }],
    destination: { provider: record.destinationEndpoint.provider, label: record.destinationEndpoint.label, externalId: record.destinationEndpoint.externalId },
    origin: { provider: record.sourceEndpoint.provider },
  };
}

type CurrentChatGptHistoryReadFailure = "ACCOUNT_SECURITY_REQUIRED" | "NORMAL_BROWSER_REMOTE_DEBUGGING_REQUIRED" | "NORMAL_BROWSER_CONNECT_TIMED_OUT" | "NORMAL_BROWSER_PERMISSION_DENIED" | "NORMAL_BROWSER_CONNECTION_FAILED" | "NORMAL_BROWSER_UNAVAILABLE" | "PROFILE_IN_USE" | "MANUAL_READ_REQUIRED" | "AUTH_REQUIRED" | "CONTENT_NOT_READY" | "CONTENT_EPOCH_CHANGED" | "COMMAND_NOT_ACCEPTED" | "COMMAND_SOURCE_MISMATCH" | "EXACT_CLIENT_UNAVAILABLE" | "HOST_ACTIVATION_EXACT_CLIENT_UNAVAILABLE" | "HISTORY_INITIAL_EXACT_CLIENT_UNAVAILABLE" | "HISTORY_AFTER_EMPTY_EXACT_CLIENT_UNAVAILABLE" | "HISTORY_POST_READ_EXACT_CLIENT_UNAVAILABLE" | "EXACT_CLIENT_AMBIGUOUS" | "IDENTITY_MISMATCH" | "CONVERSATION_CHANGED" | "BRIDGE_UNAVAILABLE" | "OTHER";

const activeHistoryFailurePrefix = "ACTIVE_HISTORY_READ_FAILED:";

function activeHistoryFailureCode(error: unknown): CurrentChatGptHistoryReadFailure {
  const detail = String(error);
  if (detail.includes("CHATGPT_ACCOUNT_SECURITY_REQUIRED")) return "ACCOUNT_SECURITY_REQUIRED";
  if (detail.includes("AUTH_REQUIRED")) return "AUTH_REQUIRED";
  if (detail.includes("INCOMPLETE")) return "CONTENT_NOT_READY";
  if (detail.includes("CHATGPT_NORMAL_BROWSER_REMOTE_DEBUGGING_REQUIRED")) return "NORMAL_BROWSER_REMOTE_DEBUGGING_REQUIRED";
  if (detail.includes("CHATGPT_NORMAL_BROWSER_AUTO_CONNECT_TIMED_OUT")) return "NORMAL_BROWSER_CONNECT_TIMED_OUT";
  if (detail.includes("CHATGPT_NORMAL_BROWSER_AUTO_CONNECT_PERMISSION_DENIED")) return "NORMAL_BROWSER_PERMISSION_DENIED";
  if (detail.includes("CHATGPT_NORMAL_BROWSER_AUTO_CONNECT_FAILED")) return "NORMAL_BROWSER_CONNECTION_FAILED";
  if (detail.includes("CHATGPT_NORMAL_BROWSER")) return "NORMAL_BROWSER_UNAVAILABLE";
  if (detail.includes("CHATGPT_ROUTER_PROFILE_IN_USE")) return "PROFILE_IN_USE";
  const codes: CurrentChatGptHistoryReadFailure[] = ["ACCOUNT_SECURITY_REQUIRED", "NORMAL_BROWSER_REMOTE_DEBUGGING_REQUIRED", "NORMAL_BROWSER_CONNECT_TIMED_OUT", "NORMAL_BROWSER_PERMISSION_DENIED", "NORMAL_BROWSER_CONNECTION_FAILED", "NORMAL_BROWSER_UNAVAILABLE", "PROFILE_IN_USE", "MANUAL_READ_REQUIRED", "AUTH_REQUIRED", "CONTENT_NOT_READY", "CONTENT_EPOCH_CHANGED", "COMMAND_NOT_ACCEPTED", "COMMAND_SOURCE_MISMATCH", "EXACT_CLIENT_UNAVAILABLE", "HOST_ACTIVATION_EXACT_CLIENT_UNAVAILABLE", "HISTORY_INITIAL_EXACT_CLIENT_UNAVAILABLE", "HISTORY_POST_READ_EXACT_CLIENT_UNAVAILABLE", "EXACT_CLIENT_AMBIGUOUS", "IDENTITY_MISMATCH", "CONVERSATION_CHANGED", "BRIDGE_UNAVAILABLE", "OTHER"];
  const exactCode = codes.find((candidate) => detail.includes(`${activeHistoryFailurePrefix}${candidate}`));
  if (exactCode) return exactCode;
  const prefixIndex = detail.indexOf(activeHistoryFailurePrefix);
  const code = prefixIndex >= 0 ? detail.slice(prefixIndex + activeHistoryFailurePrefix.length) : "OTHER";
  return codes.includes(code as CurrentChatGptHistoryReadFailure) ? code as CurrentChatGptHistoryReadFailure : "OTHER";
}

function presentCurrentChatGptHistoryReadFailure(error: unknown): { failure: CurrentChatGptHistoryReadFailure; message: string } {
  const failure = activeHistoryFailureCode(error);
  if (failure === "ACCOUNT_SECURITY_REQUIRED") return { failure, message: "Router 浏览器需要人工安全验证。请在同一页面完成验证，再选择“我已完成登录或验证”。" };
  if (failure.startsWith("NORMAL_BROWSER_")) return { failure, message: "旧浏览器连接已退役。请使用 AI Work Router Browser。" };
  if (failure === "PROFILE_IN_USE") return { failure, message: "另一个 Router ChatGPT carrier 正在使用专用 profile。Router 不会关闭、刷新、重开或争用该页面，因此本次不会读取。" };
  if (failure === "MANUAL_READ_REQUIRED") return { failure, message: "当前对话已绑定。点击“检查新回复”后，Router 会在普通 Chrome 的可见许可下仅观察这一精确对话；不会打开、导航、刷新或发送 ChatGPT。" };
  if (failure === "AUTH_REQUIRED") {
    return { failure: "AUTH_REQUIRED", message: "当前对话已绑定，需要在 Router 专用 ChatGPT 浏览器完成登录后才能读取内容。" };
  }
  if (failure === "CONTENT_NOT_READY") {
    return { failure: "CONTENT_NOT_READY", message: "当前对话已绑定，但页面内容仍在加载或暂时无法读取。重新读取不会发送消息。" };
  }
  if (failure === "IDENTITY_MISMATCH") return { failure, message: "当前对话已绑定，但读取结果未通过精确身份核验。重新读取不会发送消息。" };
  if (failure === "CONVERSATION_CHANGED") return { failure, message: "当前对话页面已变更，Router 没有读取其他对话。重新读取不会发送消息。" };
  if (failure === "BRIDGE_UNAVAILABLE") return { failure, message: "当前对话已绑定，但连接暂不可用。重新读取不会发送消息。" };
  return { failure, message: "当前对话已绑定，但暂时无法读取现有内容。重新读取不会发送消息。" };
}

function codexFeedbackDraftKey(workstreamId: string, resultRunId: string) {
  return JSON.stringify([workstreamId, resultRunId]);
}

function codexFeedbackDraftTarget(scopeId: string) {
  const parsed: unknown = JSON.parse(scopeId);
  if (!Array.isArray(parsed) || parsed.length !== 2 || !parsed.every((value) => typeof value === "string")) {
    throw new Error("Invalid Codex feedback draft scope");
  }
  return { workstreamId: parsed[0], resultRunId: parsed[1] };
}

/**
 * The desktop host adapts only Router Core projections to the shared V3 view.
 * It deliberately never uses names, list order, or a browser window as routing
 * authority: every read/action below carries the selected Workstream or exact
 * persisted Endpoint identity back to the Core.
 */
export function UnifiedWorkbenchHost({initialSurface="BRIDGES"}:{initialSurface?:WorkbenchSurface}={}) {
  const [notificationCount,setNotificationCount]=useState(0);
  const [notificationDetail,setNotificationDetail]=useState(false);
  const [nativeAcceptanceAvailable, setNativeAcceptanceAvailable] = useState(false);
  const [nativeAcceptanceAttempted, setNativeAcceptanceAttempted] = useState(false);
  const [nativeAcceptanceResult, setNativeAcceptanceResult] = useState("");
  const nativeAcceptanceInFlight = useRef(false);
  useEffect(() => { void codexApi.nativeWriterAcceptanceAvailable().then(setNativeAcceptanceAvailable).catch(() => {}); }, []);
  const [snapshot, setSnapshot] = useState<WorkspaceSnapshot | null>(()=>readModels.peek<WorkspaceSnapshot>("workspace:desktop")??null);
  // The workspace snapshot carries only the selected Workstream's active
  // Endpoints.  The dashboard projection is the separate Core-owned index
  // used to distinguish otherwise identical visible Workstream names.
  const [dashboard, setDashboard] = useState<DashboardProjection | null>(()=>readModels.peek<DashboardProjection>("directory:desktop")??null);
  const [selectedWorkstreamId, setSelectedWorkstreamId] = useState<string | null>(null);
  const [history, setHistory] = useState<FeedEvent[]>([]);
  const [codexHistoryError, setCodexHistoryError] = useState<string | null>(null);
  const [goal, setGoal] = useState<GoalPresentation | null>(null);
  const { beginLoad: beginDraftLoad, changeDraft, draft: workbenchDraft, hydrate: hydrateDraft } = useDraftAutosaveProjection((workstreamId, text, expectedRevision) => codexApi.saveWorkstreamDraft(workstreamId, text, expectedRevision));
  const [webAccessRequest,setWebAccessRequest]=useState(0);
  const [runtimeError, setRuntimeError] = useState<string | null>(null);
  const [backupNotice, setBackupNotice] = useState<string | null>(null);
  const [codexConnected, setCodexConnected] = useState(false);
  const [codexConnectionBusy, setCodexConnectionBusy] = useState(false);
  const [codexConnectionDetail, setCodexConnectionDetail] = useState<string | null>(null);
  const [boundChatGptStatus, setBoundChatGptStatus] = useState<BoundChatGptProviderSurfaceStatus | null>(null);
  const selectedWorkstreamScope = useRef<string | null>(null);
  selectedWorkstreamScope.current = selectedWorkstreamId;

  const boundChatGptStatusRequest = useRef(0);
  const [hostEnvironment, setHostEnvironment] = useState<HostEnvironmentStatus | null>(null);
  const [projectLinks, setProjectLinks] = useState<ExternalProjectLink[]>([]);
  const [projectLinksLoading, setProjectLinksLoading] = useState(false);
  const [codexProjectId, setCodexProjectId] = useState("");
  const [projectLinkError, setProjectLinkError] = useState<string | null>(null);
  const [unprojectedThread, setUnprojectedThread] = useState<UnprojectedThreadStart | null>(null);
  const [unprojectedDirectory, setUnprojectedDirectory] = useState("");
  const [unprojectedCreating, setUnprojectedCreating] = useState(false);
  const [newWorkName, setNewWorkName] = useState("");
  const [newWorkCreating, setNewWorkCreating] = useState(false);
  const [newProjectName, setNewProjectName] = useState("");
  const [newProjectWorkName, setNewProjectWorkName] = useState("");
  const [newProjectCreating, setNewProjectCreating] = useState(false);
  const unprojectedCreatingRef = useRef(false);
  const candidateScopeGeneration = useRef(0);
  const [reviewResults, setReviewResults] = useState<WorkstreamReviewResult[]>([]);
  const [resultFocus, setResultFocus] = useState<{ workstreamId: string; provider: "CHATGPT" | "CODEX"; runId: string } | null>(null);
  const [attentionTarget, setAttentionTarget] = useState<{ workstreamId: string; sourceId?: string; kind: string } | null>(null);
  const [providerRunStatus, setProviderRunStatus] = useState<ProviderRunStatus | null>(null);
  const [codexRequests, setCodexRequests] = useState<CodexStructuredRequest[]>([]);
  const [codexRequestSendingId, setCodexRequestSendingId] = useState<string | null>(null);
  const codexRequestResponseRef = useRef<string | null>(null);
  const [codexRequestDecisions, setCodexRequestDecisions] = useState<Record<string, "accept" | "decline">>({});
  const [handoff, setHandoff] = useState<HandoffPresentation | null>(null);
  const handoffSendRef = useRef<string | null>(null);
  const [explicitChatGptUrl, setExplicitChatGptUrl] = useState("");
  const [explicitChatGptCandidate, setExplicitChatGptCandidate] = useState<ExplicitChatGptBindingCandidate | null>(null);
  const [explicitChatGptConfirmation, setExplicitChatGptConfirmation] = useState<{ state: ExplicitChatGptConfirmationState; message: string | null }>({ state: "IDLE", message: null });
  const [codexThreadId, setCodexThreadId] = useState("");
  const [codexThreadLabel, setCodexThreadLabel] = useState("");
  const [existingCodexThreads, setExistingCodexThreads] = useState<ExistingCodexThreadCandidate[]>([]);
  const [existingCodexThreadsLoading, setExistingCodexThreadsLoading] = useState(false);
  const [selectedExistingCodexThread, setSelectedExistingCodexThread] = useState<ExistingCodexThreadCandidate | null>(null);
  const [selectedAttachmentIds, setSelectedAttachmentIds] = useState<Record<string, string[]>>({});
  const codexFeedbackAutosave = useResultScopedDraftAutosave(
    async (scopeId) => {
      const target = codexFeedbackDraftTarget(scopeId);
      const draft = await codexApi.readCodexFeedbackDraft(target.workstreamId, target.resultRunId);
      return draft && { text: draft.text, revision: draft.revision, updatedAt: draft.updatedAt };
    },
    async (scopeId, text, expectedRevision) => {
      const target = codexFeedbackDraftTarget(scopeId);
      const draft = await codexApi.saveCodexFeedbackDraft(target.workstreamId, target.resultRunId, text, expectedRevision);
      return { text: draft.text, revision: draft.revision, updatedAt: draft.updatedAt };
    },
  );
  const [codexFeedbackError, setCodexFeedbackError] = useState<string | null>(null);
  const [codexFeedbackSending, setCodexFeedbackSending] = useState(false);
  const codexFeedbackSendRef = useRef(false);
  const [codexFeedbackTarget, setCodexFeedbackTarget] = useState<{ workstreamId: string; endpointId: string; resultRunId: string } | null>(null);
  const [surface, setSurface] = useState<WorkbenchSurface>(initialSurface);
  const [roleMode, setRoleMode] = useState(false);
  const [bindingRequest, setBindingRequest] = useState(0);
  const [projectEntry, setProjectEntry] = useState<ConnectionScreen | null>(null);
  const [replyObservations, setReplyObservations] = useState<ReplyObservation[]>([]);
  const [selectedReply, setSelectedReply] = useState<{ workstreamId: string; id: string } | null>(null);
  const [selectedCodexObservation, setSelectedCodexObservation] = useState<{ workstreamId: string; id: string } | null>(null);
  const [chatGptRefreshStatus, setChatGptRefreshStatus] = useState<string | null>(null);
  const [chatGptRefreshBusy, setChatGptRefreshBusy] = useState(false);
  const [browserConfigured, setBrowserConfigured] = useState(false);
  const [browserStatusBusy, setBrowserStatusBusy] = useState(false);
  const [chatGptAccountSecurityRequired, setChatGptAccountSecurityRequired] = useState(false);
  const [codexRefreshStatus, setCodexRefreshStatus] = useState<string | null>(null);
  const [codexRefreshBusy, setCodexRefreshBusy] = useState(false);
  const [currentChatGptHistory, setCurrentChatGptHistory] = useState<ChatGptConversationHistory | null>(null);
  const [currentChatGptHistoryReadFailure, setCurrentChatGptHistoryReadFailure] = useState<{ failure: CurrentChatGptHistoryReadFailure; message: string } | null>(null);
  const currentChatGptHistoryRequest = useRef(0);
  // Presentation-only single flight. It never owns a backend queue and is
  // cleared as soon as the exact automatic read settles.
  const currentHistoryFlights = useRef(new Map<string, { dirty: boolean }>());
  const currentReplyObservationRequest = useRef(0);
  const replyTarget = useMemo(() => {
    const query = new URLSearchParams(window.location.search);
    return { workstreamId: query.get("workstream")?.trim() || null, replyId: query.get("reply")?.trim() || null };
  }, []);
  const hideToTray = useCallback(() => {
    void getCurrentWindow().hide().catch((error) => setRuntimeError(`窗口未能隐藏到托盘：${String(error)}`));
  }, []);

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
      setChatGptAccountSecurityRequired(environment.chatgptBrowserMode === "AUTH_REQUIRED");
      setBrowserConfigured(["CONFIGURED", "NOT_STARTED"].includes(environment.chatgptBrowserMode));
    } catch (error) { setRuntimeError(`Host 运行状态未确认：${String(error)}`); }
  }, []);
  useEffect(() => { void refreshHostEnvironment(); }, [refreshHostEnvironment]);

  const selected = snapshot?.workstreams.find((workstream) => workstream.id === selectedWorkstreamId) ?? null;
  const selectedProject = snapshot?.projects.find((project) => project.id === selected?.projectId) ?? null;

  const selectedCodexEndpoint = snapshot?.selectedWorkstreamId === selectedWorkstreamId ? snapshot.activeCodexEndpoint ?? null : null;
  // The current workstream's exact endpoint remains separate so another
  // visible tab cannot make this workstream look ready.
  const selectedChatGptEndpoint = snapshot?.selectedWorkstreamId === selectedWorkstreamId
    ? snapshot.activeChatgptEndpoint ?? null
    : null;
  const openHostBrowserSetup = useCallback(() => {
    const securityRequired = chatGptAccountSecurityRequired || hostEnvironment?.chatgptBrowserMode === "AUTH_REQUIRED";
    const open = !securityRequired && selectedWorkstreamId && selectedChatGptEndpoint
      ? codexApi.openBoundChatGptConversation(selectedWorkstreamId).then(() => codexApi.openHostChatGptBrowserSetup())
      : codexApi.openHostChatGptBrowserSetup();
    return open.then(async () => {
      setRuntimeError(null);
      await refreshHostEnvironment();
      return true;
    }).catch((error) => { setRuntimeError(securityRecoveryError(error, "OPEN")); return false; });
  }, [chatGptAccountSecurityRequired, hostEnvironment?.chatgptBrowserMode, refreshHostEnvironment, selectedWorkstreamId, selectedChatGptEndpoint]);
  const openBoundChatGptInDefaultBrowser = useCallback(() => {
    if (!selectedWorkstreamId || !selectedChatGptEndpoint) {
      setRuntimeError("当前工作区没有精确绑定的 ChatGPT 对话，无法打开默认浏览器入口。");
      return;
    }
    void codexApi.openBoundChatGptInDefaultBrowser(selectedWorkstreamId)
      .then((result) => setChatGptRefreshStatus(
        result.state === "ALREADY_OPEN"
          ? "当前精确绑定对话已在 Chrome 中打开；Router 没有新开、激活或控制标签。"
          : result.state === "CHECK_REQUIRED"
            ? "为避免重复打开 ChatGPT 标签，Router 尚未打开链接：请使用 AI Work Router Browser 查看绑定对话。"
            : "已交给默认浏览器打开精确绑定链接。Router 不会按标题猜测后台标签，也不会读取或控制页面。",
      ))
      .catch((error) => setRuntimeError(`无法在默认浏览器打开精确 ChatGPT 对话：${String(error)}`));
  }, [selectedWorkstreamId, selectedChatGptEndpoint]);
  const openApprovedHandoffDestination = useCallback((handoffId: string) => {
    const current = handoff;
    const approvedManualDispatch = current?.status === "APPROVED" && current.requiresManualDispatch === true;
    const prewriteFailure = current?.status === "FAILED" && CHATGPT_PREWRITE_HANDOFF_FAILURE_CODES.has(current.deliveryCode ?? "");
    if (!current || current.id !== handoffId || current.direction !== "CODEX_TO_CHATGPT" || (!approvedManualDispatch && !prewriteFailure)) {
      const message = "这条交接不再是可安全人工处理的精确记录；Router 没有打开任何 ChatGPT 对话。";
      setHandoff((active) => active?.id === handoffId ? { ...active, unavailableReason: message } : active);
      return;
    }
    if (current.destination?.provider !== "CHATGPT" || !selectedChatGptEndpoint || current.destination.externalId !== selectedChatGptEndpoint.externalId) {
      const message = "已批准交接的精确 ChatGPT 目标不再等于当前绑定；Router 没有用当前或最近对话替代它。";
      setHandoff((active) => active?.id === handoffId ? { ...active, unavailableReason: message } : active);
      return;
    }
    setHandoff((active) => active?.id === handoffId && active.unavailableReason ? { ...active, unavailableReason: null } : active);
    openBoundChatGptInDefaultBrowser();
  }, [handoff, openBoundChatGptInDefaultBrowser, selectedChatGptEndpoint]);
  const refreshBoundChatGptStatus = useCallback(() => {
    const request = ++boundChatGptStatusRequest.current;
    if (!selectedWorkstreamId || !selectedChatGptEndpoint) {
      setBoundChatGptStatus(null);
      return;
    }
    setBoundChatGptStatus(null);
    void codexApi.boundChatGptProviderSurfaceStatus(selectedWorkstreamId)
      .then((status) => {
        if (request === boundChatGptStatusRequest.current) setBoundChatGptStatus(status);
      })
      .catch((error) => {
        if (request === boundChatGptStatusRequest.current) setRuntimeError(`ChatGPT 直接载体状态未确认：${String(error)}`);
      });
  }, [selectedChatGptEndpoint, selectedWorkstreamId]);
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

  const refreshCurrentChatGptHistory = useCallback(function refreshHistory(workstreamId: string, endpoint: RouterEndpoint | null | undefined, automatic = true) {
    const flightKey = endpoint && endpoint.provider === "CHATGPT"
      ? `${workstreamId}:${endpoint.id}:${endpoint.externalId}`
      : null;
    const existingFlight = flightKey ? currentHistoryFlights.current.get(flightKey) : undefined;
    if (existingFlight) {
      if (!automatic) existingFlight.dirty = true;
      return;
    }
    const flight = { dirty: false };
    const generation = candidateScopeGeneration.current;
    const request = ++currentChatGptHistoryRequest.current;
    setCurrentChatGptHistoryReadFailure(null);
    if (!endpoint || endpoint.provider !== "CHATGPT") { setCurrentChatGptHistory(null); return; }
    if (automatic) {
      setCurrentChatGptHistory(null);
      // A workstream switch must not present a historical "paused" error.
      // The explicit check establishes or refreshes the narrow Chrome observer;
      // passive observation never bulk-reads the user's conversation here.
      setCurrentChatGptHistoryReadFailure(null);
      return;
    }
    if (flightKey) currentHistoryFlights.current.set(flightKey, flight);
    void codexApi.readActiveChatGptProviderLatestSnapshot(workstreamId).then((snapshot) => {
      if (request !== currentChatGptHistoryRequest.current) return;
      // The backend sends only the latest nonempty observed turn to this summary view.
      // Full progressive history remains available through the dedicated history reader.
      const messages = snapshot.turns
        // The integrated native terminal DTO used lowercase roles in older
        // installed builds. Normalize presentation at this boundary too.
        .map((turn) => ({ ...turn, role: turn.role.toUpperCase() }))
        .filter((turn) => turn.role === "USER" || turn.role === "ASSISTANT")
        .map((turn, index) => ({
          id: turn.id || `surface-turn-${index}`,
          role: turn.role as "USER" | "ASSISTANT",
          text: turn.text,
          blocks: [],
          codeBlocks: [],
          resources: [],
        }));
      const history: ChatGptConversationHistory = {
        id: snapshot.conversationId,
        title: "ChatGPT 页面中已加载的内容",
        branchScope: "CURRENT_VISIBLE_BRANCH",
        completeness: "PARTIAL",
        messages,
        diagnostics: {
          beginningReached: false,
          endReached: false,
          beginningSteps: 0,
          endSteps: 0,
          initialDomMessageCount: messages.length,
          scrollTarget: "DOCUMENT",
          viewportHeight: 0,
          scrollableHeight: 0,
          endLastScrollTop: 0,
          endLastScrollableHeight: 0,
          endStalledSteps: 0,
          scrollRestored: false,
        },
      };
      // History is ephemeral presentation data and must match the current
      // exact ACTIVE identity before it can be displayed.
      if (history.id !== endpoint.externalId) {
        setCurrentChatGptHistory(null);
        setCurrentChatGptHistoryReadFailure({ failure: "IDENTITY_MISMATCH", message: "当前对话已绑定，但读取结果未通过精确身份核验。重新读取不会发送消息。" });
        return;
      }
      if (!history.messages.length) {
        setCurrentChatGptHistory(null);
        setCurrentChatGptHistoryReadFailure({ failure: "CONTENT_NOT_READY", message: "当前对话已绑定，但页面内容仍在加载或暂时无法读取。重新读取不会发送消息。" });
        return;
      }
      setCurrentChatGptHistory(history);
      setCurrentChatGptHistoryReadFailure(null);
    }).catch((error) => {
      if (request !== currentChatGptHistoryRequest.current) return;
      setCurrentChatGptHistory(null);
      setCurrentChatGptHistoryReadFailure(presentCurrentChatGptHistoryReadFailure(error));
    }).finally(() => {
      if (flightKey) currentHistoryFlights.current.delete(flightKey);
      if (flight.dirty && generation === candidateScopeGeneration.current && request === currentChatGptHistoryRequest.current && selectedWorkstreamScope.current === workstreamId) {
        refreshHistory(workstreamId, endpoint, false);
      }
    });
  }, []);

  const refreshCurrentReplyObservations = useCallback((workstreamId: string, endpoint: RouterEndpoint | null | undefined) => {
    const request = ++currentReplyObservationRequest.current;
    setReplyObservations([]);
    setSelectedReply(null);
    if (!endpoint || endpoint.provider !== "CHATGPT") return;
    void codexApi.replyObservations(workstreamId).catch(() => []).then((nextObservations) => {
      if (request !== currentReplyObservationRequest.current) return;
      // The desktop projection includes both provider surfaces.  The normal
      // ChatGPT lane must not render a Codex observation as a ChatGPT reply.
      setReplyObservations(nextObservations);
      const chatGptObservations = nextObservations.filter((item) => !item.endpointId || item.endpointId === endpoint.id);
      const targetReplyId = replyTarget.workstreamId === workstreamId ? replyTarget.replyId : null;
      const targetExists = targetReplyId ? chatGptObservations.some((item) => item.id === targetReplyId) : true;
      setSelectedReply(targetExists && (targetReplyId || chatGptObservations[0]?.id) ? { workstreamId, id: targetReplyId || chatGptObservations[0].id } : null);
      if (!targetExists) setRuntimeError("通知指定的精确 ChatGPT 回复当前不可读或不存在，未显示其他回复。");
    });
  }, [replyTarget]);

  const checkNewChatGptReplies = useCallback(async () => {
    if (!selectedWorkstreamId || !selectedChatGptEndpoint || chatGptRefreshBusy || chatGptAccountSecurityRequired) return;
    setChatGptRefreshBusy(true);
    setChatGptRefreshStatus(null);
    try {
      const result = await codexApi.checkNewChatGptReplies(selectedWorkstreamId);
      refreshCurrentReplyObservations(selectedWorkstreamId, selectedChatGptEndpoint);
      if (result.state === "OBSERVER_BASELINE_ESTABLISHED") {
        // Baseline suppresses historical notifications, not the current
        // readable content the user explicitly asked to check.
        refreshCurrentChatGptHistory(selectedWorkstreamId, selectedChatGptEndpoint, false);
      }
      // A successful exact check supersedes a transient earlier connection
      // failure. Keeping that failure in the large reply panel contradicts
      // the explicit success result and makes a working observer look broken.
      setCurrentChatGptHistoryReadFailure(null);
      setBrowserConfigured(true);
      setChatGptRefreshStatus(result.observationCreated ? "已记录 1 条新终态回复。" : result.state === "OBSERVER_BASELINE_ESTABLISHED" ? "已建立观察起点；现有历史不会作为新回复通知。" : "没有新的终态回复。");
      setRuntimeError(null);
    } catch (error) {
      setChatGptRefreshStatus("检查未完成；没有发送消息或创建回复记录。");
      const failure = activeHistoryFailureCode(error);
      if (failure === "ACCOUNT_SECURITY_REQUIRED" || failure === "AUTH_REQUIRED") {
        setChatGptAccountSecurityRequired(true);
        setBrowserConfigured(false);
        setCurrentChatGptHistory(null);
        setCurrentChatGptHistoryReadFailure(presentCurrentChatGptHistoryReadFailure(error));
        setChatGptRefreshStatus("Router 浏览器需要登录或人工验证；自动读取和发送已停止。");
      } else if (failure.startsWith("NORMAL_BROWSER_")) {
        setBrowserConfigured(false);
        setCurrentChatGptHistory(null);
        setCurrentChatGptHistoryReadFailure(presentCurrentChatGptHistoryReadFailure(error));
        setChatGptRefreshStatus(presentCurrentChatGptHistoryReadFailure(error).message);
      } else if (failure === "PROFILE_IN_USE") {
        setCurrentChatGptHistory(null);
        setCurrentChatGptHistoryReadFailure(presentCurrentChatGptHistoryReadFailure(error));
        setChatGptRefreshStatus("另一个 Router ChatGPT carrier 正在使用专用 profile；Router 没有关闭、刷新、重开或争用它。");
      }
      setRuntimeError(failure.startsWith("NORMAL_BROWSER_")
        ? `检查 ChatGPT 新回复未执行：${presentCurrentChatGptHistoryReadFailure(error).message}`
        : failure === "PROFILE_IN_USE"
        ? "检查 ChatGPT 新回复未执行：Router 专用 profile 正在使用中。"
        : `检查 ChatGPT 新回复失败：${String(error)}`);
    } finally {
      setChatGptRefreshBusy(false);
    }
  }, [chatGptRefreshBusy, chatGptAccountSecurityRequired, refreshCurrentReplyObservations, refreshCurrentChatGptHistory, selectedChatGptEndpoint, selectedWorkstreamId]);

  const refreshBrowserStatus = useCallback(async () => {
    if (browserStatusBusy) return;
    setBrowserStatusBusy(true);
    try {
      const environment = await codexApi.hostEnvironmentStatus();
      const connected = ["CONFIGURED", "NOT_STARTED"].includes(environment.chatgptBrowserMode);
      setHostEnvironment(environment);
      setChatGptAccountSecurityRequired(environment.chatgptBrowserMode === "AUTH_REQUIRED");
      setBrowserConfigured(connected);
      setChatGptAccountSecurityRequired(environment.chatgptBrowserMode === "AUTH_REQUIRED");
      setCurrentChatGptHistoryReadFailure(null);
      setChatGptRefreshStatus(connected
        ? "Router 浏览器已配置；可检查当前精确绑定对话。绑定和登录状态独立。"
        : "Router 浏览器尚不可用或需要你完成登录。请查看当前浏览器状态。",
      );
      setRuntimeError(null);
    } catch (error) {
      setBrowserConfigured(false);
      setChatGptRefreshStatus("无法确认 Router 浏览器状态；没有发送消息。");
      setRuntimeError(`Router 浏览器状态未确认：${String(error)}`);
    } finally {
      setBrowserStatusBusy(false);
    }
  }, [browserStatusBusy]);

  const checkNewCodexReplies = useCallback(async () => {
    if (!selectedWorkstreamId || !selectedCodexEndpoint || codexRefreshBusy) return;
    setCodexRefreshBusy(true);
    setCodexRefreshStatus(null);
    try {
      const result = await codexApi.checkNewCodexReplies(selectedWorkstreamId);
      const [observations, read] = await Promise.all([
        codexApi.replyObservations(selectedWorkstreamId).catch(() => []),
        codexApi.readThreadHistory(selectedCodexEndpoint.externalId).catch(() => null),
      ]);
      setReplyObservations(observations);
      setHistory(read?.history ?? []);
      setCodexHistoryError(read ? null : "当前绑定 Codex 对话暂时无法通过官方读取显示；绑定没有改变。");
      setCodexRefreshStatus(result.observationCreated
        ? "检查完成：已记录 1 条 Codex 新回复。"
        : result.state === "LATEST_TURN_INTERRUPTED" ? "最新任务已中断，没有新的完成回复。"
        : result.state === "LATEST_TURN_ACTIVE" ? "最新任务仍在运行，尚无新的完成回复。"
        : result.state === "OBSERVATION_MATERIALIZATION_PENDING" ? "最新完成回复正在落盘；可稍后再次检查。"
        : result.lastSuccessfulCheckAt ? "检查完成：没有新终态回复。" : "检查完成：当前没有可读取的已完成回复。");
      setRuntimeError(null);
    } catch (error) {
      const detail = String(error);
      setCodexRefreshStatus(`检查未完成：${detail}。没有发送消息、恢复对话或创建 ProviderRun。`);
      setRuntimeError(`检查 Codex 新回复失败：${detail}`);
    } finally {
      setCodexRefreshBusy(false);
    }
  }, [codexRefreshBusy, selectedCodexEndpoint, selectedWorkstreamId]);

  const loadSelected = useCallback(async (workstreamId: string, preserveExplicitChatGptCandidate = false) => {
    const draftLoadGeneration = beginDraftLoad(workstreamId);
    const selectionGeneration = ++candidateScopeGeneration.current;
    setUnprojectedThread(null);
    setUnprojectedDirectory("");
    setExistingCodexThreads([]);
    setSelectedExistingCodexThread(null);
    setExplicitChatGptUrl("");
    if (!preserveExplicitChatGptCandidate) {
      setExplicitChatGptCandidate(null);
      setExplicitChatGptConfirmation({ state: "IDLE", message: null });
    }
    setCodexThreadId("");
    setCodexThreadLabel("");
    setSelectedWorkstreamId(workstreamId);
    setHistory([]);
    setCodexHistoryError(null);
    setReplyObservations([]);
    setSelectedReply((current) => current?.workstreamId === workstreamId ? current : null);
    setChatGptRefreshStatus(null);
    setCodexRefreshStatus(null);
    setCurrentChatGptHistory(null);
    setCurrentChatGptHistoryReadFailure(null);
    // A later confirmation refresh increments this generation. A pre-confirm
    // load that finishes afterwards must never restore the superseded reply.
    const replyObservationRequest = ++currentReplyObservationRequest.current;
    setCodexRequests([]);
    setCodexRequestDecisions({});
    setGoal(null);
    setHandoff(null);
    setRuntimeError(null);
    const next = await codexApi.workstreamSnapshot(workstreamId);
    if (selectionGeneration !== candidateScopeGeneration.current) return;
    // A selected-workstream read owns its presentation, not the global sidebar
    // index. A slower response must not remove another already-listed choice.
    setSnapshot(current => current ? { ...next,
      projects: [...new Map([...current.projects, ...next.projects].map(row => [row.id, row])).values()],
      workstreams: [...new Map([...current.workstreams, ...next.workstreams].map(row => [row.id, row])).values()] } : next);
    const activeChatGptEndpoint = next.selectedWorkstreamId === workstreamId ? next.activeChatgptEndpoint : null;
    if(LEGACY_WORKBENCH_DETAILS)refreshCurrentChatGptHistory(workstreamId, activeChatGptEndpoint);
    const [savedDraft, nextResults, nextObservations, nextCodexRequests] = await Promise.all([
      codexApi.readWorkstreamDraft(workstreamId),
      codexApi.reviewWorkstreamResults(workstreamId).catch(() => []),
      LEGACY_WORKBENCH_DETAILS?codexApi.replyObservations(workstreamId).catch(() => []):Promise.resolve([]),
      // Keep legacy test doubles compatible while real Tauri uses the same
      // exact, revision-checked projection as the mobile surface.
      LEGACY_WORKBENCH_DETAILS&&next.activeCodexEndpoint&&typeof codexApi.codexRequests === "function" ? codexApi.codexRequests(workstreamId).catch(() => []) : Promise.resolve([]),
    ]);
    if (selectionGeneration !== candidateScopeGeneration.current) return;
    hydrateDraft(workstreamId, savedDraft, draftLoadGeneration);
    setReviewResults(nextResults);
    setCodexRequests(nextCodexRequests);
    if (replyObservationRequest === currentReplyObservationRequest.current) {
      setReplyObservations(nextObservations);
      const chatGptObservations = activeChatGptEndpoint
        ? nextObservations.filter((item) => !item.endpointId || item.endpointId === activeChatGptEndpoint.id)
        : [];
      const targetReplyId = replyTarget.workstreamId === workstreamId ? replyTarget.replyId : null;
      const targetExists = targetReplyId ? chatGptObservations.some((item) => item.id === targetReplyId) : true;
      setSelectedReply((current) => current?.workstreamId === workstreamId && chatGptObservations.some((item) => item.id === current.id)
        ? current
        : targetExists && (targetReplyId || chatGptObservations[0]?.id) ? { workstreamId, id: targetReplyId || chatGptObservations[0].id } : null);
      if (!targetExists) setRuntimeError("通知指定的精确 ChatGPT 回复当前不可读或不存在，未显示其他回复。");
    }
    if (!LEGACY_WORKBENCH_DETAILS||!next.activeCodexEndpoint) return next;
    const [historyRead, currentGoal] = await Promise.all([
      codexApi.readThreadHistory(next.activeCodexEndpoint.externalId)
        .then((read) => ({ read, error: null as string | null }))
        .catch(() => ({ read: null, error: "当前绑定 Codex 对话暂时不能通过官方读取显示；绑定没有改变。" })),
      codexApi.readCodexGoal(workstreamId).catch(() => null),
    ]);
    if (selectionGeneration !== candidateScopeGeneration.current) return;
    setHistory(historyRead.read?.history ?? []);
    setCodexHistoryError(historyRead.error);
    setGoal(currentGoal ? {
      threadId: currentGoal.threadId,
      text: currentGoal.objective,
      status: currentGoal.status.toUpperCase() as GoalPresentation["status"],
      reportedAt: typeof currentGoal.updatedAt === "number" ? currentGoal.updatedAt : null,
      readAt: Date.now(),
      usageLabel: currentGoal.tokensUsed == null ? undefined : `${currentGoal.tokensUsed} tokens`,
      activeTurnId: currentGoal.activeTurnId,
      controllableActions: currentGoal.status === "active" ? ["PAUSE", "DELETE", ...(currentGoal.activeTurnId ? ["STOP_TURN" as const] : [])] : currentGoal.status === "paused" ? ["RESUME", "DELETE"] : ["DELETE"],
    } : { threadId: next.activeCodexEndpoint.externalId, status: "NONE", readAt: Date.now(), controllableActions: [] });
    return next;
  }, [beginDraftLoad, hydrateDraft, refreshCurrentChatGptHistory, replyTarget]);

  const selectWorkstream = useCallback(async (workstreamId: string) => {
    const workstream = snapshot?.workstreams.find((candidate) => candidate.id === workstreamId)??dashboard?.workstreams.find(item=>item.workstream.id===workstreamId)?.workstream;
    if (!workstream) throw new Error("要打开的工作区不在当前 Router 索引中。");
    // This is a presentation preference, not endpoint routing. Persist the
    // exact local Workstream identity so a restart cannot silently return to a
    // same-named predecessor.
    setSelectedCodexObservation((current) => current?.workstreamId === workstreamId ? current : null);
    // Choose/clear the reader before a deleted predecessor can redirect
    // WORKSPACE back to the directory while selection IPC is pending.
    const [loaded]=await Promise.all([loadSelected(workstreamId),codexApi.selectWorkspace(workstream.projectId, workstreamId)]);
    return loaded;
  }, [loadSelected, snapshot?.workstreams,dashboard?.workstreams]);

  useEffect(() => {
    if (selectedWorkstreamId) void loadSelected(selectedWorkstreamId).catch((error) => setRuntimeError(String(error)));
  // Loading belongs to a chosen exact workstream, not the changing snapshot object.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedWorkstreamId]);

  useEffect(() => {
    if (!selectedProject) { setProjectLinks([]); setProjectLinksLoading(false); return; }
    setProjectLinksLoading(true);
    void codexApi.listExternalProjectLinks(selectedProject.id)
      .then(setProjectLinks)
      .catch((error) => setProjectLinkError(`项目关联读取失败：${String(error)}`))
      .finally(() => setProjectLinksLoading(false));
  }, [selectedProject?.id]);

  const addProjectLink = async () => {
    if (!selectedProject) return;
    const raw = codexProjectId.trim();
    if (!raw) return;
    const link = await codexApi.upsertExternalProjectLink(selectedProject.id, {
      provider: "CODEX", externalProjectId: raw, canonicalUrl: null,
      label: raw, sourceKind: "user-confirmed-existing-project", sourceVersion: null, verifiedAt: null,
    });
    const nextLinks = [...projectLinks.filter((item) => item.provider !== link.provider), link];
    setProjectLinks(nextLinks);
    setCodexProjectId("");
    setProjectLinkError(null);
  };

  const pairSelectedEndpoints = async () => {
    if (!selected || !snapshot) return;
    const selectedCodexThreadId = codexThreadId.trim();
    const codex = selectedCodexThreadId ? {
      expectedActiveEndpointId: snapshot.activeCodexEndpoint?.id ?? null,
      externalId: selectedCodexThreadId, label: codexThreadLabel.trim(),
    } : null;
    if (!codex) throw new Error("先选择一条 Codex 精确对话，或验证 ChatGPT 具体对话链接。");
    if (!codex.label) throw new Error("每个已选对话都必须写明标题、日期和来源信息。");
    const chatgptCandidate = explicitChatGptCandidate?.workstreamId === selected.id && selectedExistingCodexThread
      ? explicitChatGptCandidate
      : null;
    const paired = chatgptCandidate
      ? await codexApi.confirmExplicitChatGptCodexPairing(selected.id, codex)
      : await codexApi.pairWorkstreamEndpoints(selected.id, {
        expectedBindingRevision: selected.bindingRevision,
        codex,
      });
    if (paired.codexEndpoint?.externalId !== selectedCodexThreadId) {
      throw new Error("配对事务没有返回所选的 Codex 精确对话；未进入工作区。");
    }
    if (chatgptCandidate && paired.chatgptEndpoint?.externalId !== chatgptCandidate.externalId) {
      throw new Error("配对事务没有返回所选的 ChatGPT 精确对话；未进入工作区。");
    }
    setCodexThreadId("");
    setCodexThreadLabel("");
    setSelectedExistingCodexThread(null);
    if (chatgptCandidate) {
      setExplicitChatGptCandidate(null);
      setExplicitChatGptConfirmation({ state: "IDLE", message: null });
    }
    // Pairing is identity persistence, not a request to read a transcript,
    // reconcile writer state, or inspect/change a Goal. A post-save full
    // `loadSelected` would perform those unrelated operations and could turn
    // an already committed exact binding into a misleading UI failure.
    setHistory([]);
    setGoal(null);
    try {
      const refreshed = await codexApi.workstreamSnapshot(selected.id);
      if (refreshed.selectedWorkstreamId !== selected.id || refreshed.activeCodexEndpoint?.externalId !== selectedCodexThreadId) {
        setRuntimeError("Codex 精确配对已保存，但当前 SQLite 绑定投影尚未返回所选对话。请重新读取状态，不要重复保存。");
        return;
      }
      setSnapshot(refreshed);
      // Sidebar bindings/attention come from a separate Core projection. Refresh
      // it after persistence too; never retain an old "missing binding" badge.
      void refreshDashboard().catch(() => setDashboard(null));
    } catch (error) {
      setRuntimeError(`Codex 精确配对已保存，但当前 SQLite 绑定投影读取失败：${String(error)}。不要重复保存。`);
    }
  };

  const prepareExplicitChatGptBinding = async (input = explicitChatGptUrl) => {
    if (!selected) throw new Error("先选择当前 Router Workstream。");
    const generation = candidateScopeGeneration.current;
    const candidate = await codexApi.prepareExplicitChatGptEndpointBinding(selected.id, input);
    if (generation !== candidateScopeGeneration.current || candidate.workstreamId !== selected.id) return;
    setExplicitChatGptCandidate(candidate);
    setExplicitChatGptConfirmation({ state: "IDLE", message: null });
    setExplicitChatGptUrl("");
    setProjectLinkError(null);
  };

  const prepareOwnerConfirmedChatGptBinding = async (input = explicitChatGptUrl) => {
    if (!selected) throw new Error("先选择当前 Router Workstream。");
    const generation = candidateScopeGeneration.current;
    const candidate = await codexApi.prepareOwnerConfirmedChatGptEndpointBinding(selected.id, input);
    if (generation !== candidateScopeGeneration.current || candidate.workstreamId !== selected.id) return;
    setExplicitChatGptCandidate(candidate);
    setExplicitChatGptConfirmation({ state: "IDLE", message: null });
    setExplicitChatGptUrl("");
    setProjectLinkError(null);
  };

  const createUnprojectedThread = async () => {
    if (!selected || unprojectedCreatingRef.current || unprojectedThread) return;
    unprojectedCreatingRef.current = true;
    setUnprojectedCreating(true);
    const generation = candidateScopeGeneration.current;
    try {
      const thread = await codexApi.startUnprojectedCodexThread(unprojectedDirectory);
      if (generation !== candidateScopeGeneration.current) return;
      setUnprojectedThread(thread);
      setUnprojectedDirectory(thread.directory);
      setCodexThreadId(thread.thread.id);
      setCodexThreadLabel("新建且已验证的 Codex 对话");
      setProjectLinkError(null);
    } catch (error) {
      if (generation === candidateScopeGeneration.current) setProjectLinkError(`创建未完成；未自动重试：${String(error)}`);
    } finally {
      unprojectedCreatingRef.current = false;
      setUnprojectedCreating(false);
    }
  };

  const ensureCodexCatalogConnection = async () => {
    const initial = await codexApi.status();
    if (initial.connected) return;
    await codexApi.connect();
    for (let attempt = 0; attempt < 20; attempt += 1) {
      await new Promise<void>((resolve) => window.setTimeout(resolve, 250));
      const status = await codexApi.status();
      if (status.connected) {
        setCodexConnected(true);
        return;
      }
      if (!status.connecting) throw new Error(status.detail ?? "Codex 目录暂不可用");
    }
    throw new Error("Codex 目录连接超时；没有尝试恢复或启动任何既有对话。");
  };

  const loadExistingCodexThreads = async () => {
    if (existingCodexThreadsLoading) return;
    setExistingCodexThreadsLoading(true);
    setProjectLinkError(null);
    try {
      await ensureCodexCatalogConnection();
      const catalog = await codexApi.listExistingCodexThreads();
      if (!catalog.complete) throw new Error("Codex 对话目录未完整读取；没有显示部分结果。");
      setExistingCodexThreads(catalog.threads);
    } catch (error) {
      setProjectLinkError(`已有 Codex 对话读取未完成：${String(error)}`);
      throw error;
    } finally {
      setExistingCodexThreadsLoading(false);
    }
  };

  const selectExistingCodexThread = async (threadId: string) => {
    setProjectLinkError(null);
    try {
      await ensureCodexCatalogConnection();
      const verified = await codexApi.verifyExistingCodexThread(threadId);
      setCodexThreadId(verified.id);
      setCodexThreadLabel(verified.label);
      setSelectedExistingCodexThread(verified);
      setUnprojectedThread(null);
    } catch (error) {
      setProjectLinkError(`已有 Codex 对话未通过精确身份核验：${String(error)}`);
      throw error;
    }
  };

  const presentExplicitChatGptConfirmationFailure = (error: unknown) => {
    const detail = String(error);
    if (detail.includes("binding changed after this pairing review") || detail.includes("ACTIVE CHATGPT Endpoint changed") || detail.includes("reviewed ACTIVE CHATGPT Endpoint no longer exists")) {
      return "当前工作区的绑定状态已经变化。请重新验证这个 ChatGPT 对话后再确认。当前绑定没有改变。";
    }
    if (detail.includes("already ACTIVE") || detail.includes("UNIQUE constraint failed")) {
      return "这个 ChatGPT 对话已经绑定到另一个工作区。当前绑定没有改变。";
    }
    if (detail.includes("No validated ChatGPT binding")) {
      return "验证候选已不在当前 Router 会话中。请重新验证这个 ChatGPT 对话后再确认。当前绑定没有改变。";
    }
    return "绑定没有保存。当前绑定没有改变。";
  };

  const confirmExplicitChatGptBinding = async () => {
    if (!selected || !explicitChatGptCandidate || explicitChatGptCandidate.workstreamId !== selected.id) {
      setExplicitChatGptConfirmation({ state: "FAILED", message: "没有等待确认的 ChatGPT 候选绑定。当前绑定没有改变。" });
      return;
    }
    // When this exact ChatGPT candidate accompanies a freshly verified
    // existing Codex thread, the visible Pair Review owns the single atomic
    // persistence transaction. Merely entering that review must not write an
    // Endpoint or begin a provider action.
    if (selectedExistingCodexThread && codexThreadId.trim()) return;
    const workstreamId = selected.id;
    const candidate = explicitChatGptCandidate;
    setExplicitChatGptConfirmation({ state: "CONFIRMING", message: null });
    let confirmed: RouterEndpoint;
    try {
      confirmed = await codexApi.confirmExplicitChatGptEndpointBinding(workstreamId);
    } catch (error) {
      setExplicitChatGptConfirmation({ state: "FAILED", message: presentExplicitChatGptConfirmationFailure(error) });
      return;
    }

    // A successful checked transaction is authoritative. A malformed response
    // or later presentation read must never reclassify it as an unsaved bind.
    if (!isExactConfirmedChatGptEndpoint(confirmed, workstreamId, candidate.externalId)) {
      setExplicitChatGptConfirmation({ state: "SUCCEEDED", message: "ChatGPT 对话已经绑定，但返回的 Endpoint 未得到一致确认。不要重复确认；请重新读取状态。" });
      return;
    }
    setExplicitChatGptConfirmation({ state: "SUCCEEDED", message: "ChatGPT 对话已绑定。" });
    try {
      // Narrow exact-active reproof: this does not depend on draft, result,
      // observation, Codex history, or Goal presentation reads.
      const refreshed = await codexApi.workstreamSnapshot(workstreamId);
      if (!isExactConfirmedChatGptEndpoint(refreshed.selectedWorkstreamId === workstreamId ? refreshed.activeChatgptEndpoint : null, workstreamId, confirmed.externalId)) {
        setExplicitChatGptConfirmation({ state: "SUCCEEDED", message: "绑定事务已完成，但刷新后的 ACTIVE Endpoint 尚未得到一致确认。不要重复确认；请重新读取状态。" });
        return;
      }
      setSnapshot(refreshed);
      // Immediately clear the superseded Endpoint's local presentation before
      // returning to the Workbench, then refill only current ACTIVE data.
      refreshCurrentReplyObservations(workstreamId, refreshed.activeChatgptEndpoint);
      refreshCurrentChatGptHistory(workstreamId, refreshed.activeChatgptEndpoint);
      void refreshDashboard().catch(() => setDashboard(null));
      setExplicitChatGptCandidate(null);
      setSurface("WORKSPACE");
    } catch {
      setExplicitChatGptConfirmation({ state: "SUCCEEDED", message: "ChatGPT 对话已经绑定，但工作区状态刷新失败。不要重复确认；返回工作区后可重新读取状态。" });
    }
  };

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
    // The selected exact Workstream also has its freshly read local
    // ReplyObservations available. Keep a just-arrived unread reply visible
    // if the dashboard refresh has not caught up yet, without duplicating the
    // same exact source record.
    if (workstream.id === selectedWorkstreamId) {
      for (const observation of replyObservations.filter((item) => !item.readAt && !item.handledAt)) {
        if (!attentionItems.some((item) => item.sourceId === observation.id)) {
          attentionItems.push({ sourceId: observation.id, kind: observation.endpointId === selectedCodexEndpoint?.id ? "CODEX_REPLY_OBSERVED" : "CHATGPT_REPLY_OBSERVED", priority: 3, message: observation.endpointId === selectedCodexEndpoint?.id ? "Codex has a newly observed reply" : "ChatGPT has a newly observed reply", activityAt: observation.observedAt });
        }
      }
    }
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
      : dashboardItem.chatgptEndpoint && dashboardItem.codexEndpoint
        ? "ChatGPT 与 Codex 已绑定"
        : dashboardItem.chatgptEndpoint
          ? "ChatGPT 已绑定"
          : dashboardItem.codexEndpoint
            ? "Codex 已绑定"
            : workstream.status;
    return {
      id: workstream.id,
      name: workstream.name,
      projectName: snapshot?.projects.find((project) => project.id === workstream.projectId)?.name??dashboardItem?.projectName,
      lifecycle: lifecycleOf(workstream),
      pinned: Boolean(workstream.pinnedAt),
      updatedAt: workstream.updatedAt,
      attentionCount,
      attentionItems,
      sourceLabel,
      statusLabel: attentionItems[0]?.message ?? bindingStatus,
    };
  }), [indexedWorkstreams,dashboardByWorkstreamId, replyObservations, selectedCodexEndpoint, selectedWorkstreamId, snapshot]);

  const selectedChatGptObservationId = selectedReply?.workstreamId === selectedWorkstreamId
    ? selectedReply.id : null;
  const selectedObservation = selectedChatGptObservationId && selectedChatGptEndpoint
    ? replyObservations.find((item) => item.id === selectedChatGptObservationId && (!item.endpointId || item.endpointId === selectedChatGptEndpoint.id)) ?? null
    : null;
  const selectedCodexObservationId = selectedCodexObservation?.workstreamId === selectedWorkstreamId
    ? selectedCodexObservation.id : null;
  const exactCodexObservation = selectedCodexObservationId && selectedCodexEndpoint
    ? replyObservations.find((item) => item.id === selectedCodexObservationId && item.endpointId === selectedCodexEndpoint.id) ?? null
    : null;
  const latestCodexObservation = selectedCodexEndpoint
    ? selectedCodexObservationId
      ? exactCodexObservation
      : replyObservations.filter((item) => item.endpointId === selectedCodexEndpoint.id).sort((a, b) => b.observedAt - a.observedAt)[0] ?? null
    : null;
  const exactCurrentHistory = selectedChatGptEndpoint && currentChatGptHistory?.id === selectedChatGptEndpoint.externalId ? currentChatGptHistory : null;
  const latestCurrentHistoryMessage = exactCurrentHistory ? [...exactCurrentHistory.messages].reverse().find((message) => message.text.trim()) ?? null : null;
  const chatGptRuntimeDetail = selectedChatGptEndpoint
    ? "Router 读取精确绑定的 ChatGPT 对话。已打开页面的被动观察可降级；随时可手动检查新回复。"
    : "当前工作流没有 ACTIVE ChatGPT 对话。";
  const latest = [...history].reverse().find((event) => event.kind === "AgentMessage" && event.text?.trim());
  const reply: ExactReply | null = selectedChatGptObservationId && !selectedObservation && selectedWorkstreamId ? {
    id: selectedChatGptObservationId,
    workstreamId: selectedWorkstreamId,
    provider: "CHATGPT",
    title: selected?.name ?? "ChatGPT 完整回复",
    readState: "READ",
    text: "收件箱指定的精确 ChatGPT 回复当前不可读；Router 没有显示当前页内容或其他回复。",
    completeness: "UNAVAILABLE",
    sourceLabel: "收件箱指定的精确 ChatGPT 回复 · 当前不可读",
    readOnly: true,
  } : selectedObservation && selectedWorkstreamId ? {
    id: selectedObservation.id,
    workstreamId: selectedWorkstreamId,
    provider: "CHATGPT",
    title: selected?.name ?? "ChatGPT 完整回复",
    observedAt: selectedObservation.observedAt,
    readState: selectedObservation.handledAt ? "HANDLED" : selectedObservation.readAt ? "READ" : "UNREAD",
    text: selectedObservation.text,
    completeness: "COMPLETE",
    sourceLabel: "Router 已观察到的当前 ChatGPT 回复",
  } : exactCurrentHistory && selectedWorkstreamId ? {
    id: `current-history-${exactCurrentHistory.id}`,
    workstreamId: selectedWorkstreamId,
    provider: "CHATGPT",
    title: exactCurrentHistory.title,
    readState: "READ",
    text: latestCurrentHistoryMessage?.text ?? "当前 ChatGPT 对话已绑定，当前可读内容为空。",
    completeness: exactCurrentHistory.completeness === "COMPLETE_VISIBLE_BRANCH" ? "COMPLETE" : "PARTIAL",
    sourceLabel: "当前绑定 ChatGPT 直接载体 · 最新终态内容",
    readOnly: true,
  } : selectedChatGptEndpoint && selectedWorkstreamId ? {
    id: `current-history-unavailable-${selectedChatGptEndpoint.id}`,
    workstreamId: selectedWorkstreamId,
    provider: "CHATGPT",
    title: selected?.name ?? "当前 ChatGPT 对话",
    readState: "READ",
    text: currentChatGptHistoryReadFailure?.message ?? "当前对话已绑定。检查新回复会读取这条精确对话，不会发送消息。",
    completeness: "UNAVAILABLE",
    sourceLabel: "当前绑定 ChatGPT 对话 · 只读精确观察",
    readOnly: true,
    historyReadFailure: currentChatGptHistoryReadFailure?.failure,
  } : latest && selectedCodexEndpoint && selectedWorkstreamId ? {
    id: latest.id,
    workstreamId: selectedWorkstreamId,
    provider: "CODEX",
    title: selected?.name ?? "Codex 完整回复",
    observedAt: selected?.updatedAt ?? null,
    readState: "READ",
    text: latest.text ?? "",
    completeness: "COMPLETE",
    sourceLabel: `精确线程 ${selectedCodexEndpoint.externalId.slice(0, 8)}…`,
  } : null;

  const markReply = async (replyId: string, handled: boolean) => {
    if (!selectedWorkstreamId) return;
    if (handled) await codexApi.markReplyObservationHandled(selectedWorkstreamId, replyId);
    else await codexApi.markReplyObservationRead(selectedWorkstreamId, replyId);
    await loadSelected(selectedWorkstreamId);
  };

  const changeLifecycle = async (workstreamId: string, lifecycle: WorkbenchLifecycle) => {
    if (lifecycle === "ARCHIVED") await codexApi.archiveWorkstream(workstreamId);
    else if (lifecycle === "TRASHED") await codexApi.trashWorkstream(workstreamId);
    else await codexApi.restoreWorkstream(workstreamId);
    await refreshIndex();
    if (workstreamId === selectedWorkstreamId) await loadSelected(workstreamId);
  };

  const changeGoal = async (_threadId: string, action: "PAUSE" | "RESUME" | "DELETE" | "STOP_TURN") => {
    if (!selectedWorkstreamId) return;
    if (action === "PAUSE") await codexApi.pauseCodexGoal(selectedWorkstreamId);
    else if (action === "RESUME") await codexApi.resumeCodexGoal(selectedWorkstreamId);
    else if (action === "DELETE") await codexApi.clearCodexGoal(selectedWorkstreamId);
    else {
      const turnId = goal?.activeTurnId;
      if (!turnId) throw new Error("当前没有 Router 已观察到的精确活动 Turn，未发送中断。");
      await codexApi.interruptCodexTurn(selectedWorkstreamId, turnId);
    }
    await loadSelected(selectedWorkstreamId);
  };

  const respondToCodexRequest = async (request: CodexStructuredRequest, decision: "accept" | "decline") => {
    if (!selectedWorkstreamId || request.responseSent || codexRequestResponseRef.current) return;
    if (!request.choices.some((choice) => choice.id === decision)) {
      throw new Error("Router 没有提供该一次性回应选项；未发送任何选择。");
    }
    codexRequestResponseRef.current = request.requestId;
    setCodexRequestSendingId(request.requestId);
    try {
      await codexApi.respondToCodexRequest(selectedWorkstreamId, request.requestId, { revision: request.revision, decision });
      // This is only an optimistic presentation update after the Core has
      // accepted the exact revision. The server remains lifecycle authority.
      setCodexRequests((current) => current.map((item) => item.requestId === request.requestId ? { ...item, responseSent: true } : item));
      setCodexRequestDecisions((current) => ({ ...current, [request.requestId]: decision }));
    } finally {
      if (codexRequestResponseRef.current === request.requestId) codexRequestResponseRef.current = null;
      setCodexRequestSendingId(null);
    }
  };

  const showHandoff = (session: HandoffReviewSession, direction: HandoffPresentation["direction"], candidates: HandoffPresentation["candidates"] = [], origin: HandoffPresentation["origin"] = null, destination: HandoffPresentation["destination"] = direction === "CHATGPT_TO_CODEX"
    ? selectedCodexEndpoint ? { provider: "CODEX", label: selectedCodexEndpoint.label, externalId: selectedCodexEndpoint.externalId } : null
    : selectedChatGptEndpoint ? { provider: "CHATGPT", label: selectedChatGptEndpoint.label, externalId: selectedChatGptEndpoint.externalId } : null) => {
    const requiresManualDispatch = session.requiresManualDispatch === true;
    setHandoff({ id: session.actionId, revision: session.revision, status: session.status, direction, message: session.message, candidates, selectedAttachmentLabels: session.attachments, attachmentOptions: session.attachmentOptions, canApprove: session.status === "READY", canSend: session.status === "APPROVED" && !requiresManualDispatch, requiresManualDispatch, origin, destination });
  };
  const prepareChatGptHandoff = async (replyId: string, selectedText?: string) => {
    if (!selectedWorkstreamId) return;
    const observation = replyObservations.find((item) => item.id === replyId && item.endpointId === selectedChatGptEndpoint?.id) ?? null;
    // This UI route is entered from an exact ReplyObservation.  A retained
    // ProviderRun can share an opaque identity but is a different record class;
    // it must never replace the observed reply in the owner review.
    if (!observation) throw new Error("收件箱指定的精确 ChatGPT 回复当前不可读；未创建审阅，也没有用历史结果替代。");
    const session = await codexApi.prepareChatGptToCodexHandoff(selectedWorkstreamId, replyId, selectedText);
    showHandoff(session, "CHATGPT_TO_CODEX", [{ id: observation.id, label: "ChatGPT 完整原回复", text: observation.text, sourceLabel: "已选精确 ChatGPT 外部观察" }], { provider: "CHATGPT", sourceKind: "REPLY_OBSERVATION", observedAt: observation.observedAt, manualSelectionCount: selectedText ? 1 : 0 });
  };
  const selectChatGptHandoffAttachments = async (actionId: string, filenames: string[]) => {
    if (!handoff || handoff.id !== actionId || handoff.revision == null) return;
    const session = await codexApi.selectChatGptToCodexHandoffAttachments(actionId, handoff.revision, filenames);
    if (session.actionId !== actionId || session.status !== "READY") throw new Error("附件结果未匹配当前审阅；没有批准或发送。");
    // Selection changes the resource revision, not the locally edited text.
    // Also preserve edits typed while materialization was in flight.
    setHandoff((current) => current?.id === actionId && current.status === "READY"
      ? { ...current, revision: session.revision, selectedAttachmentLabels: session.attachments, attachmentOptions: session.attachmentOptions }
      : current);
  };
  const prepareCodexHandoff = async (result: WorkstreamReviewResult) => {
    if (!selectedWorkstreamId || result.provider !== "CODEX") return;
    const attachmentIds = selectedAttachmentIds[result.runId] ?? result.attachments.filter((attachment) => attachment.defaultSelected).map((attachment) => attachment.id);
    const session = await codexApi.prepareCodexToChatGptHandoff(selectedWorkstreamId, result.runId, attachmentIds);
    showHandoff(session, "CODEX_TO_CHATGPT", [{ id: result.runId, label: "Codex 完整结果", text: result.text, sourceLabel: "已选精确 Codex ProviderRun" }], { provider: "CODEX", sourceKind: "PROVIDER_RUN", observedAt: result.reviewedAt ?? null });
  };
  const prepareCodexObservationHandoff = async (observation: ReplyObservation) => {
    if (!selectedWorkstreamId || !selectedCodexEndpoint || observation.endpointId !== selectedCodexEndpoint.id) return;
    const session = await codexApi.prepareCodexToChatGptHandoff(selectedWorkstreamId, observation.id, selectedAttachmentIds[observation.id] ?? []);
    showHandoff(session, "CODEX_TO_CHATGPT", [{ id: observation.id, label: "外部 Codex 最新回复", text: observation.text, sourceLabel: "已选精确 Codex 外部观察" }], { provider: "CODEX", sourceKind: "REPLY_OBSERVATION", observedAt: observation.observedAt });
  };
  const toggleAttachment = (runId: string, attachmentId: string) => {
    setSelectedAttachmentIds((current) => {
      const result = reviewResults.find(item => item.runId === runId);
      const observation = replyObservations.find(item => item.id === runId && item.endpointId === selectedCodexEndpoint?.id);
      const attachment = (result?.attachments ?? observation?.attachments)?.find(item => item.id === attachmentId);
      if (!attachment || !attachmentCanBeRelayed(attachment.integrityStatus)) return current;
      const selected = current[runId] ?? result?.attachments.filter((attachment) => attachment.defaultSelected && attachmentCanBeRelayed(attachment.integrityStatus)).map((attachment) => attachment.id) ?? [];
      const next = selected.includes(attachmentId) ? selected.filter((id) => id !== attachmentId) : [...selected, attachmentId];
      return { ...current, [runId]: next };
    });
  };
  const updateHandoffMessage = (id: string, message: string) => setHandoff((current) => current?.id === id && current.status === "READY" ? { ...current, message } : current);
  const approveHandoff = async (id: string) => {
    if (!handoff || handoff.id !== id || handoff.revision == null) return;
    const session = handoff.direction === "CHATGPT_TO_CODEX"
      ? await codexApi.approveChatGptToCodexHandoff(id, handoff.revision, handoff.message)
      : await codexApi.approveCodexToChatGptHandoff(id, handoff.revision, handoff.message);
    showHandoff(session, handoff.direction, handoff.candidates, handoff.origin, handoff.destination);
  };
  const sendHandoff = async (id: string) => {
    if (!handoff || handoff.id !== id || handoff.revision == null || handoff.status !== "APPROVED" || !handoff.canSend || handoffSendRef.current) return;
    handoffSendRef.current = id;
    try {
      if (handoff.direction === "CHATGPT_TO_CODEX") {
        await codexApi.sendChatGptToCodexHandoffReview(id, handoff.revision);
        setHandoff({ ...handoff, status: "SENT", canApprove: false, canSend: false, runStatus: "Codex Turn 已接受；执行完成状态独立刷新。" });
      } else {
        const delivery = await codexApi.sendCodexToChatGptHandoffReview(id, handoff.revision);
        setHandoff({ ...handoff, status: delivery.status, canApprove: false, canSend: false, deliveryDetail: delivery.detail ?? null });
      }
    } finally {
      if (handoffSendRef.current === id) handoffSendRef.current = null;
    }
    // Keep the exact post-send Handoff projection visible. loadSelected clears
    // transient review state by design, so refreshing it here would erase the
    // just-confirmed delivery before the user can inspect D16.
  };

  const chatgptResults = reviewResults.filter((result) => result.provider === "CHATGPT");
  const codexResults = reviewResults.filter((result) => result.provider === "CODEX");
  const focusedChatGptResultId = resultFocus?.workstreamId === selectedWorkstreamId && resultFocus.provider === "CHATGPT"
    ? resultFocus.runId : null;
  const focusedCodexResultId = resultFocus?.workstreamId === selectedWorkstreamId && resultFocus.provider === "CODEX"
    ? resultFocus.runId : null;
  const focusedChatGptResult = focusedChatGptResultId
    ? chatgptResults.find((result) => result.runId === focusedChatGptResultId) ?? null : null;
  const focusedCodexResult = focusedCodexResultId
    ? codexResults.find((result) => result.runId === focusedCodexResultId) ?? null : null;
  const visibleChatGptResults = focusedChatGptResultId ? (focusedChatGptResult ? [focusedChatGptResult] : []) : chatgptResults;
  const visibleCodexResults = focusedCodexResultId ? (focusedCodexResult ? [focusedCodexResult] : []) : codexResults;
  const chatgptResultsPanel = (chatgptResults.length || focusedChatGptResultId) ? <section className="v3-codex-results-focus" aria-label="ChatGPT 历史结果">
    <header><div><p>{focusedChatGptResultId ? "从收件箱打开的精确 ChatGPT 执行结果" : "Router 保留的执行结果"}</p><h1>{selected?.name ?? "当前工作"} · ChatGPT 历史结果</h1><span>这些记录与当前精确 ChatGPT 对话的新回复分开；不会因打开此页而发送、批准或标为已读。</span>{focusedChatGptResultId ? <small>精确 Router 执行结果 ID：<code>{focusedChatGptResultId}</code></small> : null}</div><div>{focusedChatGptResultId ? <button type="button" onClick={() => setResultFocus(null)}>查看全部 ChatGPT 历史结果</button> : null}<button type="button" onClick={() => setSurface("WORKSPACE")}>返回当前回复</button></div></header>
    {focusedChatGptResultId && !focusedChatGptResult ? <aside className="v3-reader-history-recovery" role="alert">这条精确 ChatGPT 历史结果当前不可读；Router 没有切换到其他结果。</aside> : null}
    {visibleChatGptResults.map((result) => <article key={result.runId}><p>{focusedChatGptResult ? "收件箱指定的精确 Router 执行结果 · 只读回看" : "历史 Router 执行结果 · 只读回看"}</p><h2>{result.reviewedAt ? "已审阅的 ChatGPT 结果" : "ChatGPT 结果等待阅读"}</h2><p>这不是当前可直接交接的外部 ReplyObservation。若要交给 Codex，请回到当前 ChatGPT 回复并明确选择范围。</p><div className="v3-codex-result-body"><MarkdownMessage text={result.text} /></div>{result.markerText?.trim() ? <details><summary>当时识别到的 Codex 指令范围</summary><pre>{result.markerText}</pre></details> : null}</article>)}
  </section> : null;
  const openCodexFeedback = (resultRunId: string) => {
    if (!selectedWorkstreamId || !selectedCodexEndpoint) {
      setRuntimeError("当前工作区没有可确认的 Codex 主对话，未打开修改草稿。");
      return;
    }
    if (codexFeedbackTarget) {
      codexFeedbackAutosave.flush(codexFeedbackDraftKey(codexFeedbackTarget.workstreamId, codexFeedbackTarget.resultRunId));
    }
    const target = { workstreamId: selectedWorkstreamId, endpointId: selectedCodexEndpoint.id, resultRunId };
    setCodexFeedbackTarget(target);
    codexFeedbackAutosave.load(codexFeedbackDraftKey(target.workstreamId, target.resultRunId));
    setCodexFeedbackError(null);
    setSurface("CODEX_FEEDBACK");
  };
  const sendCodexFeedback = async () => {
    const target = codexFeedbackTarget;
    const draftKey = target ? codexFeedbackDraftKey(target.workstreamId, target.resultRunId) : null;
    const draft = draftKey ? codexFeedbackAutosave.draft(draftKey).value : "";
    if (!target || !draftKey || !draft.trim() || codexFeedbackSendRef.current) return;
    if (selectedWorkstreamId !== target.workstreamId || selectedCodexEndpoint?.id !== target.endpointId) {
      setCodexFeedbackError("当前 Codex 主对话已变更；请重新打开结果后再发送，草稿仍会保留。");
      return;
    }
    codexFeedbackSendRef.current = true;
    setCodexFeedbackSending(true);
    setCodexFeedbackError(null);
    try {
      await codexApi.sendWorkstreamCodexFeedback(target.workstreamId, target.endpointId, target.resultRunId, draft);
      codexFeedbackAutosave.change(draftKey, "");
      codexFeedbackAutosave.flush(draftKey);
      await loadSelected(target.workstreamId);
      setSurface("CODEX_RESULTS");
    } catch (error) {
      setCodexFeedbackError(`修改意见没有发送：${String(error)}`);
    } finally {
      codexFeedbackSendRef.current = false;
      setCodexFeedbackSending(false);
    }
  };
  const activeCodexEndpoint=snapshot?.activeCodexEndpoint;
  const connectionChoices: ConnectionChoice[] = [
    ...(activeCodexEndpoint ? [{
      id: activeCodexEndpoint.externalId,
      provider: "CODEX" as const,
      title: activeCodexEndpoint.label || "已绑定 Codex 主对话",
      detail: "当前已绑定，无需更换即可继续",
      selected: codexThreadId === activeCodexEndpoint.externalId,
      onSelect: () => {
        setCodexThreadId(activeCodexEndpoint.externalId);
        setCodexThreadLabel(activeCodexEndpoint.label || "已绑定 Codex 主对话 · 当前工作区已绑定");
        setProjectLinkError(null);
      },
    }] : []),
  ];
  const createNewWorkstream = async () => {
    const name = newWorkName.trim();
    if (!selectedProject || !name || newWorkCreating) return;
    setNewWorkCreating(true);
    setRuntimeError(null);
    try {
      const created = await codexApi.createWorkstream(selectedProject.id, name);
      setNewWorkName("");
      await loadSelected(created.id);
      setProjectEntry(null);
      setSurface("PROJECT");
    } catch (error) {
      setRuntimeError(`新工作未创建：${String(error)}`);
    } finally {
      setNewWorkCreating(false);
    }
  };
  const createNewProjectWorkstream = async () => {
    const projectName = newProjectName.trim();
    const workstreamName = newProjectWorkName.trim();
    if (!projectName || !workstreamName || newProjectCreating) return;
    setNewProjectCreating(true);
    setRuntimeError(null);
    try {
      const project = await codexApi.createProject(projectName);
      const created = await codexApi.createWorkstream(project.id, workstreamName);
      setNewProjectName("");
      setNewProjectWorkName("");
      await loadSelected(created.id);
      setProjectEntry(null);
      setSurface("PROJECT");
    } catch (error) {
      setRuntimeError(`新项目或工作未创建：${String(error)}`);
    } finally {
      setNewProjectCreating(false);
    }
  };
  const openProject = (entry: ConnectionScreen | null = null) => { setProjectEntry(entry); setSurface("PROJECT"); };
  const requestBinding = () => {setSurface("WORKSPACE");setBindingRequest(value => value + 1);};
  const connectionPanel = surface === "PROJECT" ? <ProjectConnectionPanel
    guidedBindingEntry onManageBindings={selectedWorkstreamId ? requestBinding : undefined}
    projectName={selectedProject?.name} workstreamName={selected?.name} links={projectLinks} choices={connectionChoices} codexProjectId={codexProjectId}
    explicitChatGptUrl={explicitChatGptUrl} explicitChatGptCandidate={explicitChatGptCandidate}
    explicitChatGptConfirmationState={explicitChatGptConfirmation.state} explicitChatGptConfirmationMessage={explicitChatGptConfirmation.message}
    activeChatGptLabel={snapshot?.activeChatgptEndpoint?.label ?? null}
    activeChatGptId={snapshot?.activeChatgptEndpoint?.externalId ?? null}
    activeCodexLabel={snapshot?.activeCodexEndpoint?.label ?? null}
    activeCodexThreadId={snapshot?.activeCodexEndpoint?.externalId ?? null}
    codexThreadId={codexThreadId} codexThreadLabel={codexThreadLabel} error={projectLinkError}
    existingCodexThreads={existingCodexThreads} existingCodexThreadsLoading={existingCodexThreadsLoading} selectedExistingCodexThread={selectedExistingCodexThread}
    unprojectedDetail={unprojectedThread ? `已取得精确对话 ID；目录：${unprojectedThread.directory}。尚未确认可在重启后重新打开；首次有效 Turn 前不要把它视为持久会话。` : null}
    onCodexProjectIdChange={setCodexProjectId}
    onSaveCodexProject={() => void addProjectLink().catch((error) => setProjectLinkError(String(error)))}
    onExplicitChatGptUrlChange={(value) => { setExplicitChatGptUrl(value); setExplicitChatGptConfirmation({ state: "IDLE", message: null }); }}
    onPrepareOwnerConfirmedChatGptBinding={() => void prepareOwnerConfirmedChatGptBinding().catch((error) => setProjectLinkError(`人工核对的链接未准备为候选：${String(error)}`))}
    onOpenHostChatGptSetup={openHostBrowserSetup}
    onUseCurrentChatGptConversation={() => { if (!selectedWorkstreamId) return; void codexApi.prepareCurrentChatGptEndpointBinding(selectedWorkstreamId).then(candidate => { setExplicitChatGptCandidate(candidate); setProjectLinkError(null); setExplicitChatGptConfirmation({ state: "IDLE", message: null }); }).catch(error => setProjectLinkError(`请选择 Router 浏览器中的具体对话，再核对绑定：${String(error)}`)); }}
    onConfirmExplicitChatGptBinding={() => void confirmExplicitChatGptBinding()}
    onLoadExistingCodexThreads={loadExistingCodexThreads}
    onSelectExistingCodexThread={selectExistingCodexThread}
    onCodexThreadIdChange={setCodexThreadId} onCodexThreadLabelChange={setCodexThreadLabel}
    onPair={() => void pairSelectedEndpoints().then(() => setSurface("WORKSPACE")).catch((error) => setProjectLinkError(`配对未保存：${String(error)}`))}
    unprojectedDirectory={unprojectedDirectory} onUnprojectedDirectoryChange={setUnprojectedDirectory}
    initialScreen={projectEntry ?? undefined}
    onCreateUnprojected={() => void createUnprojectedThread()}
    unprojectedCreating={unprojectedCreating}
    onCopyUnprojectedThreadId={() => { if (unprojectedThread) void navigator.clipboard.writeText(unprojectedThread.thread.id); }}
    unprojectedThreadId={unprojectedThread?.thread.id}
    onBack={() => setSurface("WORKSPACE")}
  /> : null;
  const projectHomePanel = surface === "PROJECT_HOME" ? <section className="v3-project-home" aria-label="项目">
    <header><h1>项目</h1><p>项目关联供新工作沿用；当前工作流的对话绑定在下方单独管理。</p></header>
    <section className="v3-project-home-current-work"><p>当前工作流</p><h2>{selected?.name ?? "尚未选择工作"}</h2><span>{snapshot?.activeChatgptEndpoint ? "ChatGPT 已绑定" : "ChatGPT 尚未绑定"}</span><small>{snapshot?.activeChatgptEndpoint ? "要替换当前工作流的 ChatGPT 对话，不会改动项目或 Codex 对话。" : "先为当前工作流绑定一条精确 ChatGPT 对话。"}</small><footer><button type="button" className="v3-primary" disabled={projectLinksLoading || !selected} onClick={() => openProject("REPLACE_CHATGPT")}>{selected ? `更换当前 ${selected.name} 的 ChatGPT 对话` : "选择工作后绑定 ChatGPT 对话"}</button><button type="button" disabled={!selected} onClick={() => openProject()}>查看当前两端绑定</button></footer></section>
    <section className="v3-project-home-primary"><h2>项目关联</h2>{projectLinksLoading ? <span className="v3-project-home-unavailable">正在读取已保存的项目关联</span> : projectLinks.some((link) => link.provider === "CHATGPT") ? <span>ChatGPT 项目链接已保存</span> : <span className="v3-project-home-unavailable">尚未保存 ChatGPT 项目链接</span>}<p>Codex 项目</p><strong>{projectLinks.find((link) => link.provider === "CODEX")?.label ?? "尚未关联"}</strong><small>{projectLinks.find((link) => link.provider === "CODEX") ? "本机已有项目 · 精确来源与目录在详情中查看" : "可立即或稍后关联已有 Codex 项目；不会创建项目。"}</small><footer><button type="button" className="v3-primary" disabled={projectLinksLoading} onClick={() => openProject()}>选择对话，开始工作</button><button type="button" disabled={projectLinksLoading} onClick={() => openProject("CODEX_PROJECT")}>更换关联项目</button></footer></section>
    <section className="v3-project-home-later"><h2>另一个 ChatGPT 项目</h2><p>{projectLinks.some((link) => link.provider === "CHATGPT") ? "链接已保存 · Codex 项目尚未关联" : "尚未保存项目链接；可以先明确添加，再选择对话。"}</p><button type="button" onClick={() => openProject("CODEX_PROJECT")}>现在关联 Codex</button></section>
    <footer className="v3-project-home-actions"><button type="button" className="v3-secondary-action" onClick={() => openProject()}>绑定精确 ChatGPT 对话</button><button type="button" className="v3-secondary-action" onClick={() => openProject("UNPROJECTED")}>新建无项目 Codex 对话</button></footer>
  </section> : null;
  const newWorkPanel = surface === "NEW_WORK" ? <section className="v3-new-work-focus" aria-label="新建工作">
    <header><h1>新建工作</h1><p>先创建一个未绑定的 Router 工作区；两端对话可在下一步按精确身份分别绑定。</p></header>
    <div className="v3-new-work-grid"><section><h2>从已有项目开始</h2><p>新的工作区不会复用当前工作区的 ChatGPT 或 Codex 对话。</p><label>工作名称<input aria-label="新工作名称" value={newWorkName} disabled={newWorkCreating || !selectedProject} onChange={(event) => setNewWorkName(event.target.value)} placeholder="例如：本周需求梳理" /></label><button type="button" className="v3-primary" disabled={newWorkCreating || !selectedProject || !newWorkName.trim()} onClick={() => void createNewWorkstream()}>{newWorkCreating ? "正在创建未绑定工作…" : "创建未绑定工作并选择对话"}</button><small>{selectedProject ? `将创建在 ${selectedProject.name} 中，然后进入两端对话选择。` : "请先选择一个 Router 项目。"}</small></section><section className="v3-new-project-work"><h2>新建项目与工作</h2><p>为一个独立事项创建 Router 项目和未绑定工作；不会复用当前两端对话。</p><label>项目名称<input aria-label="新 Router 项目名称" value={newProjectName} disabled={newProjectCreating} onChange={(event) => setNewProjectName(event.target.value)} placeholder="例如：游戏开发" /></label><label>工作名称<input aria-label="新 Router 项目工作名称" value={newProjectWorkName} disabled={newProjectCreating} onChange={(event) => setNewProjectWorkName(event.target.value)} placeholder="例如：当前游戏开发" /></label><button type="button" className="v3-primary" disabled={newProjectCreating || !newProjectName.trim() || !newProjectWorkName.trim()} onClick={() => void createNewProjectWorkstream()}>{newProjectCreating ? "正在创建项目与工作…" : "创建项目并选择两端对话"}</button><small>先只创建本地 Router 结构；下一步仍会核对并保存精确 ChatGPT 与 Codex ID。</small></section><section><h2>临时讨论</h2><p>创建无项目 Codex 对话，不创建新的 Codex 项目。</p><button type="button" onClick={() => openProject("UNPROJECTED")}>新建无项目对话</button></section></div>
  </section> : null;
  const codexHistoryPanel = selectedCodexEndpoint ? <section className="v3-codex-results-focus" aria-label="Codex 对话">
    <header><div><h1>{selected?.name ?? "当前工作"} · Codex 对话</h1><p>只读官方精确线程观察；不恢复、不发送、不获取 writer。</p></div><button type="button" onClick={() => setSurface("WORKSPACE")}>返回工作区</button></header>
    <section className="v3-codex-result-block"><article className="v3-codex-result-card"><p>当前已绑定 Codex 对话</p><h2>{selectedCodexEndpoint.label || "Codex 对话"}</h2><details><summary>精确绑定标识</summary><code>{selectedCodexEndpoint.externalId}</code></details><section aria-label="Codex 观察状态"><p>{codexHistoryError ? "观察不可读" : "正在监控已完成回复"}</p><button type="button" disabled={codexRefreshBusy} onClick={() => void checkNewCodexReplies()}>{codexRefreshBusy ? "正在检查…" : "检查 Codex 新回复"}</button><small>{codexRefreshStatus ?? "检查只读取当前精确绑定线程；首次成功读取只建立基线，不会把历史回复当成新通知。"}</small></section>{selectedCodexObservationId && !exactCodexObservation ? <aside className="v3-reader-history-recovery" role="alert">收件箱指定的精确 Codex 回复当前不可读；Router 没有显示同一线程的较新回复。</aside> : null}{latestCodexObservation ? <section className="v3-codex-result-body" aria-label={exactCodexObservation ? "收件箱选定的 Codex 回复" : "最新外部 Codex 回复"}><p>{exactCodexObservation ? "从收件箱打开的精确 Codex 回复" : "当前可转交的外部 Codex 回复"}</p><small>下一步：先审阅并编辑，再批准；批准后单独发送到已绑定的 ChatGPT 对话。</small><MarkdownMessage text={latestCodexObservation.text} /><ObservationAttachmentSelection files={latestCodexObservation.attachments ?? []} selectedIds={selectedAttachmentIds[latestCodexObservation.id] ?? []} onToggle={id => toggleAttachment(latestCodexObservation.id, id)} /><button type="button" onClick={() => void markReply(latestCodexObservation.id, false)}>标为已读</button><button type="button" onClick={() => void markReply(latestCodexObservation.id, true)}>标记已处理</button><button type="button" className="v3-primary" onClick={() => void prepareCodexObservationHandoff(latestCodexObservation).then(() => setSurface("HANDOFF_REVIEW")).catch((error) => setRuntimeError(`无法准备外部 Codex 回复回传：${String(error)}`))}>审阅此回复，准备转发给 ChatGPT</button></section> : null}{codexHistoryError ? <aside className="v3-reader-history-recovery" role="status"><p>{codexHistoryError}</p><button type="button" onClick={() => void checkNewCodexReplies()}>重新读取当前 Codex 对话</button><small>重新读取不会发送消息。</small></aside> : history.length ? <div className="v3-codex-result-body"><p>只读线程历史（用于核对；不等同于可转交的外部观察）</p>{history.filter((event) => event.text?.trim()).map((event) => <article key={event.id}><p>{event.kind === "AgentMessage" ? "线程中的 Codex 回复" : "Codex 线程事件"}</p><MarkdownMessage text={event.text ?? ""} /></article>)}</div> : <p>当前没有可显示的已完成 Codex 回复；绑定仍有效。</p>}</article></section>
  </section> : null;
  const codexResultsPanel = (codexResults.length || focusedCodexResultId) ? <section className="v3-codex-results-focus" aria-label="Codex 结果与附件">
    <header><div><h1>{selected?.name ?? "当前工作"} · Codex</h1><p>{focusedCodexResultId ? "从收件箱打开的精确 Codex 执行结果" : `${selectedProject?.name ?? "AI Work Router 项目"}　/　精确主对话`}</p>{focusedCodexResultId ? <small>精确 Router 执行结果 ID：<code>{focusedCodexResultId}</code></small> : null}</div><div>{focusedCodexResultId ? <button type="button" onClick={() => setResultFocus(null)}>查看全部 Codex 历史结果</button> : null}<button type="button" className="v3-primary" onClick={() => setSurface("NEW_WORK")}>＋ 新建工作</button></div></header>
    <nav className="v3-codex-result-tabs" aria-label="结果来源"><button type="button" onClick={() => setSurface("WORKSPACE")}>ChatGPT</button><span aria-current="page">Codex</span></nav>
    {focusedCodexResultId && !focusedCodexResult ? <aside className="v3-reader-history-recovery" role="alert">这条精确 Codex 历史结果当前不可读；Router 没有切换到其他结果。</aside> : null}
    <div className="v3-codex-result-layout"><section className="v3-codex-result-list">{visibleCodexResults.map((result) => { const selectedIds = selectedAttachmentIds[result.runId] ?? result.attachments.filter((attachment) => attachment.defaultSelected).map((attachment) => attachment.id); const relayableCount = result.attachments.filter((attachment) => attachmentCanBeRelayed(attachment.integrityStatus)).length; return <div key={result.runId} className="v3-codex-result-block"><article className="v3-codex-result-card">
      <span className="v3-codex-result-badge">{focusedCodexResult ? "收件箱指定的精确 Router 执行结果 · 可单独审阅" : "历史 Router 执行结果 · 可单独审阅"}</span><h2>{selected?.name ? `${selected.name} · 历史 Codex 执行结果` : "历史 Codex 执行结果"}</h2>
      <p className="v3-detail">这不是当前 Codex 对话的新回复；它是 Router 过去一次执行保留的结果。可单独审阅后转发给 ChatGPT。</p><details className="v3-result-source-id"><summary>精确 Router 执行结果 ID</summary><code>{result.runId}</code></details><div className="v3-codex-result-body"><MarkdownMessage text={result.text} /></div>
      <section className="v3-codex-attachment-summary" aria-label="结论附件"><h3>{result.attachments.length} 个附件　·　{relayableCount} 项可安全转发</h3><div>{result.attachments.length ? result.attachments.map((attachment) => { const relayable = attachmentCanBeRelayed(attachment.integrityStatus); return <button key={attachment.id} type="button" aria-pressed={selectedIds.includes(attachment.id)} disabled={!relayable} onClick={() => toggleAttachment(result.runId, attachment.id)}>{selectedIds.includes(attachment.id) ? "✓ " : ""}{attachment.filename} · {attachmentRelayExplanation(attachment.integrityStatus, attachment.warnings ?? [])}</button>; }) : <span>此结果没有附件。</span>}</div></section><footer className="v3-codex-result-card-actions">{selectedCodexEndpoint ? <button type="button" className="v3-codex-result-modify" onClick={() => openCodexFeedback(result.runId)}>要求 Codex 修改</button> : <span className="v3-codex-result-modify v3-codex-result-unavailable" role="status">当前没有可确认的 Codex 主对话</span>}<button type="button" className="v3-primary" onClick={() => void prepareCodexHandoff(result).catch((error) => setRuntimeError(`无法准备 Codex Handoff：${String(error)}`))}>审阅此历史结果，准备转发给 ChatGPT</button></footer>
    </article></div>; })}</section>
      <section className="v3-codex-goal-column"><aside className="v3-codex-goal-overview" aria-label="当前目标概览"><p>当前目标</p><h2>{goal?.text ?? "当前没有可读取 Goal"}</h2><span className={`v3-codex-goal-status v3-codex-goal-status-${goal?.status?.toLowerCase() ?? "none"}`}>{goal?.status === "PAUSED" ? "已暂停 · 最近读取" : goal?.status === "ACTIVE" ? "目标进行中 · 最近读取" : goal ? `${goal.status} · 最近读取` : "状态未确认 · 最近读取"}</span><small>{goal?.usageLabel ? `Codex 报告用量：${goal.usageLabel}` : "用量或耗时尚未报告"}</small></aside><button type="button" className="v3-codex-goal-control" onClick={() => setSurface("GOAL")}>查看目标与控制</button><p>目标状态不等于本轮执行状态。</p></section>
    </div>
    <footer className="v3-codex-result-band"><button type="button" className="v3-codex-connection-link" onClick={() => setSurface("PROJECT")}>连接与对话 ID</button></footer>
  </section> : null;
  const codexFeedbackDraftKeyForTarget = codexFeedbackTarget ? codexFeedbackDraftKey(codexFeedbackTarget.workstreamId, codexFeedbackTarget.resultRunId) : null;
  const codexFeedbackDraft = codexFeedbackAutosave.draft(codexFeedbackDraftKeyForTarget);
  const changeCodexFeedbackDraft = (value: string) => {
    if (!codexFeedbackDraftKeyForTarget) return;
    codexFeedbackAutosave.change(codexFeedbackDraftKeyForTarget, value);
  };
  const closeCodexFeedback = () => {
    if (codexFeedbackDraftKeyForTarget) codexFeedbackAutosave.flush(codexFeedbackDraftKeyForTarget);
    setSurface("CODEX_RESULTS");
  };
  const codexFeedbackPanel = surface === "CODEX_FEEDBACK" ? <section className="v3-codex-feedback-focus" aria-label="向 Codex 提出修改"><header><h1>向 Codex 提出修改</h1><button type="button" onClick={closeCodexFeedback}>‹ 返回工作区</button></header><section className="v3-codex-feedback-editor"><span>只发给当前 Codex 主对话</span><textarea aria-label="修改意见" value={codexFeedbackDraft.value} disabled={codexFeedbackSending} onChange={(event) => changeCodexFeedbackDraft(event.target.value)} placeholder="说明需要补充或调整的内容…" /><p>原结果和附件仍保留；这次不回传给 ChatGPT。</p>{codexFeedbackDraft.saveState === "FAILED" && <p className="v3-codex-feedback-error" role="alert">修改草稿没有保存；保留在当前页面，可稍后重试。</p>}{codexFeedbackError && <p className="v3-codex-feedback-error" role="alert">{codexFeedbackError}</p>}</section><footer><button type="button" onClick={closeCodexFeedback}>取消，保留草稿</button><button type="button" className="v3-primary" disabled={codexFeedbackSending || !codexFeedbackDraft.value.trim()} onClick={() => void sendCodexFeedback()}>{codexFeedbackSending ? "正在发送修改意见…" : "发送修改意见"}</button></footer></section> : null;
  const exactCodexRequestId = attentionTarget?.workstreamId === selectedWorkstreamId && attentionTarget.kind === "CODEX_STRUCTURED_REQUEST"
    ? attentionTarget.sourceId ?? null
    : null;
  const selectedCodexRequest = exactCodexRequestId
    ? codexRequests.find((request) => request.requestId === exactCodexRequestId) ?? null
    : codexRequests.find((request) => !request.responseSent && request.method.includes("commandExecution") && request.choices.some((choice) => choice.id === "accept") && request.choices.some((choice) => choice.id === "decline")) ?? codexRequests.find((request) => !request.responseSent) ?? codexRequests[0] ?? null;
  const selectedCodexRequestDecision = selectedCodexRequest ? codexRequestDecisions[selectedCodexRequest.requestId] : undefined;
  const exactProviderRunAttention = attentionTarget?.workstreamId === selectedWorkstreamId
    && ["PROVIDER_RUN_FAILED", "PROVIDER_RUN_CANCELLED", "UNKNOWN_RUN"].includes(attentionTarget.kind)
    ? attentionTarget
    : null;
  const runtimeAttentionNotice = exactProviderRunAttention?.sourceId
    ? `收件箱指定的精确 Provider 执行 ID：${exactProviderRunAttention.sourceId}。此页只用于核对该执行的状态；Router 未收到独立可读结果时，不会以其他执行内容替代。`
    : null;
  const exactProviderRunStatus = providerRunStatus && exactProviderRunAttention?.sourceId === providerRunStatus.runId
    ? providerRunStatus
    : null;
  const providerRunNextAction = exactProviderRunStatus?.status === "FAILED"
    ? "这次执行失败；Router 不会自动重试、发送或替换为另一条结果。请返回收件箱，按实际业务决定是否另行创建新的明确操作。"
    : exactProviderRunStatus?.status === "CANCELLED"
      ? "这次执行已取消；Router 不会恢复、重试或把它当成已完成。请返回收件箱，按实际业务决定下一步。"
      : exactProviderRunStatus?.status === "UNKNOWN"
        ? "Router 无法确认这次执行的最终状态；这既不表示成功也不表示失败。不会自动重试、发送或显示其他结果。"
        : exactProviderRunStatus?.hasReviewableResult
          ? "这次执行已有独立可审阅结果；返回收件箱后只选择该精确结果继续审阅。"
          : "这次执行尚未产生可审阅结果；Router 不会把任何其他执行内容替代进来。";
  const providerRunStatusPanel = exactProviderRunStatus ? <section className="v3-codex-request-focus" aria-label="精确 Provider 执行详情"><span className="v3-codex-request-badge">收件箱指定的精确执行 · 只读</span><article className="v3-codex-request-card"><h2>{exactProviderRunStatus.provider === "CHATGPT" ? "ChatGPT" : "Codex"} 执行：{exactProviderRunStatus.status}</h2><p>{providerRunNextAction}</p><details className="v3-detail" open><summary>精确 Router 执行 ID</summary><code>{exactProviderRunStatus.runId}</code></details><section className="v3-codex-request-facts" aria-label="Router 记录的执行时间"><p>开始：{providerRunTimestamp(exactProviderRunStatus.startedAt)}</p><p>终态确认：{providerRunTimestamp(exactProviderRunStatus.terminalAt)}</p><p>最后状态更新：{providerRunTimestamp(exactProviderRunStatus.updatedAt)}</p></section><p>可审阅结果：{exactProviderRunStatus.hasReviewableResult ? "有；不会在此页替代展示内容。" : "没有。"}</p>{exactProviderRunStatus.terminalCode ? <details className="v3-detail"><summary>稳定终态代码</summary><code>{exactProviderRunStatus.terminalCode}</code></details> : <p>Router 尚未记录稳定终态代码。</p>}{exactProviderRunStatus.originHandoffId ? <details className="v3-detail"><summary>关联交接记录</summary><code>{exactProviderRunStatus.originHandoffId}</code></details> : null}</article></section> : <section className="v3-codex-request-focus" aria-label="精确 Provider 执行详情"><p role="alert">收件箱指定的精确 Provider 执行状态当前不可读或已不存在；Router 没有显示其他执行。</p>{exactProviderRunAttention?.sourceId ? <details className="v3-detail" open><summary>收件箱指定的精确执行 ID</summary><code>{exactProviderRunAttention.sourceId}</code></details> : null}</section>;
  const codexRequestPanel = surface === "CODEX_REQUEST" && selectedCodexRequest ? <section className={`v3-codex-request-focus${selectedCodexRequest.responseSent ? " is-responded" : ""}${selectedCodexRequestDecision === "decline" ? " is-declined" : ""}`} aria-label="Codex 官方结构化请求">
    <header><h1>Codex 需要一次授权</h1><button type="button" className="v3-primary" onClick={() => setSurface("NEW_WORK")}>＋ 新建工作</button></header>
    {!selectedCodexRequest.responseSent ? <><span className="v3-codex-request-badge">精确结构化请求 · 仅一次</span>{exactCodexRequestId && <details className="v3-detail" open><summary>收件箱指定的精确请求 ID</summary><code>{selectedCodexRequest.requestId}</code></details>}<article className="v3-codex-request-card"><h2>允许执行这一次命令？</h2><p>请求原因：{(selectedCodexRequest.reason??selectedCodexRequest.detail)?.trim() || "官方请求未提供可显示说明。"}</p><section aria-label="原始命令当前不可显示，只读"><pre>Router 当前未收到可显示的原始命令。</pre></section><div className="v3-codex-request-facts"><p>工作目录：Router 未提供可显示目录</p><p>授权范围：本次请求，不是整个目标</p><p>请求已在其他端回应或已失效时，当前按钮立即失效。</p></div></article><p className="v3-codex-request-note">授权回应不是 Handoff，不会替你恢复一个暂停的 Goal。</p><footer><button type="button" onClick={() => setSurface("GOAL")}>查看目标与执行状态</button><div><button type="button" disabled={codexRequestSendingId === selectedCodexRequest.requestId} onClick={() => void respondToCodexRequest(selectedCodexRequest, "decline").catch((error) => setRuntimeError(`授权回应没有送达：${String(error)}`))}>拒绝</button><button type="button" className="v3-primary" disabled={codexRequestSendingId === selectedCodexRequest.requestId} onClick={() => void respondToCodexRequest(selectedCodexRequest, "accept").catch((error) => setRuntimeError(`授权回应没有送达：${String(error)}`))}>{codexRequestSendingId === selectedCodexRequest.requestId ? "正在回应…" : "允许这一次"}</button></div></footer></> : <><span className="v3-codex-request-badge">已回应这次请求</span><article className="v3-codex-request-card"><h2>{selectedCodexRequestDecision === "decline" ? "已拒绝这一次" : "已允许这一次"}</h2><p>{selectedCodexRequestDecision === "decline" ? "这项授权不会执行，也不能重复回应。拒绝请求不等于完成整个目标。" : "这项选择已提交，不能重复使用。Codex 后续执行状态会单独显示。"}</p></article><footer><button type="button" onClick={() => setSurface("WORKSPACE")}>返回工作区</button></footer></>}
  </section> : surface === "CODEX_REQUEST" && exactCodexRequestId ? <section className="v3-codex-request-focus" aria-label="Codex 官方结构化请求"><header><h1>Codex 请求当前不可读</h1><button type="button" onClick={() => setSurface("INBOX")}>返回收件箱</button></header><p role="alert">收件箱指定的精确 Codex 请求当前不可读或已不存在；Router 没有显示其他请求。</p><details className="v3-detail" open><summary>收件箱指定的精确请求 ID</summary><code>{exactCodexRequestId}</code></details></section> : null;
  return <>{nativeAcceptanceAvailable && !CHATGPT_ROUTER_OWNED_CARRIER_PAUSED && surface === "RUNTIME" && <aside aria-label="原生发送验收（调试版）">
    <button type="button" disabled={nativeAcceptanceAttempted || !selectedWorkstreamId || !selectedChatGptEndpoint || boundChatGptStatus?.state !== "EXACT_BOUND"}
      onClick={() => {
        if (nativeAcceptanceInFlight.current || !selectedWorkstreamId || !selectedChatGptEndpoint) return;
        nativeAcceptanceInFlight.current = true;
        setNativeAcceptanceAttempted(true);
        setNativeAcceptanceResult("正在验收：一次原生提交，不自动重试。");
        void codexApi.debugAcceptNativeChatGptWriter(selectedWorkstreamId, selectedChatGptEndpoint.id)
          .then(result => setNativeAcceptanceResult(JSON.stringify(result)))
          .catch(error => setNativeAcceptanceResult(`验收未完成：${String(error)}；未自动重试。`));
      }}>调试：向当前绑定对话发送一次验收标记</button>
    <output aria-label="原生发送验收结果">{nativeAcceptanceResult}</output>
  </aside>}<UnifiedWorkbench
    notificationDetail={notificationDetail} notificationCount={notificationCount} notificationSettings={<>{desktopNotificationApi.delivery&&<NotificationDeliveryPanel api={desktopNotificationApi.delivery}/>}<NotificationAssistantSettings api={desktopNotificationApi}/></>}
    onRenameBridge={async(id,name)=>{await codexApi.renameBridge(id,name);await refreshIndex();}}
    onManageBindings={requestBinding}
    onCreateBridge={async name=>{const id=await codexApi.createBridge(name);const exact=await codexApi.workstreamSnapshot(id);const created=exact.workstreams.find(item=>item.id===id);if(!created)throw Error("新建 Bridge 暂时不可读取，请返回列表检查。");await codexApi.selectWorkspace(created.projectId,id);await refreshIndex();await loadSelected(id);flushSync(()=>setSurface("WORKSPACE"));setBindingRequest(v=>v+1);}}
    quotaPanel={<CodexQuota api={desktopQuotaApi}/>} updatePanel={<DesktopUpdateSettings/>} bridgeActivityApi={desktopBridgeActivity} items={items}
    selectedWorkstreamId={selectedWorkstreamId}
    reply={reply}
    roleCompatible={roleMode}
    assistantPanel={<AssistantSettings/>} runtimeAddon={<SharedCodexConnection/>} onOpenWebAccess={()=>setWebAccessRequest(v=>v+1)} globalActions={<><CodexNotifications onDetailChange={setNotificationDetail} onCountChange={setNotificationCount} api={desktopNotificationApi} workbenchPage={{active:surface==="NOTIFICATIONS",open:()=>setSurface("NOTIFICATIONS")}}/><DesktopWebAccess openRequest={webAccessRequest}/></>}
    criticalNotice={(chatGptAccountSecurityRequired || hostEnvironment?.chatgptBrowserMode === "AUTH_REQUIRED") ? <BridgeStatus recoveryOnly name={selected?.name || "AI Work Router"} recoveryError={runtimeError} showErrors decision={selectedChatGptEndpoint} execution={selectedCodexEndpoint} authenticationRequired={chatGptAccountSecurityRequired || hostEnvironment?.chatgptBrowserMode === "AUTH_REQUIRED"} executionConnected={codexConnected} nextAction={items.find(item => item.id === selectedWorkstreamId)?.attentionItems?.[0]?.message} onManage={() => openProject()} onOpenBrowser={openHostBrowserSetup} onAuthenticationCompleted={() => codexApi.confirmChatGptAuthenticationCompleted().then(() => { setChatGptAccountSecurityRequired(false); return refreshHostEnvironment(); }).catch(error => setRuntimeError(securityRecoveryError(error, "COMPLETE")))} /> : null}
    bridgePanel={<>{!roleMode && (selected && !(chatGptAccountSecurityRequired || hostEnvironment?.chatgptBrowserMode === "AUTH_REQUIRED") ? <BridgeStatus name={selected?.name || "AI Work Router"} recoveryError={runtimeError} showErrors decision={selectedChatGptEndpoint} execution={selectedCodexEndpoint} authenticationRequired={chatGptAccountSecurityRequired || hostEnvironment?.chatgptBrowserMode === "AUTH_REQUIRED"} executionConnected={codexConnected} nextAction={items.find(item => item.id === selectedWorkstreamId)?.attentionItems?.[0]?.message} onManage={requestBinding} onOpenBrowser={openHostBrowserSetup} onAuthenticationCompleted={() => codexApi.confirmChatGptAuthenticationCompleted().then(() => { setChatGptAccountSecurityRequired(false); return refreshHostEnvironment(); }).catch(error => setRuntimeError(securityRecoveryError(error, "COMPLETE")))} /> : null)} {selectedWorkstreamId && <RoleBridgePanel key={selectedWorkstreamId} workstreamId={selectedWorkstreamId} workstreamName={items.find(item => item.id === selectedWorkstreamId)?.name} api={desktopRoleBridgeApi} readerActive={surface==="WORKSPACE"} onOpenConnection={()=>setSurface("RUNTIME")} bindingRequest={bindingRequest} externalBindingEntry={Boolean(selected && !(chatGptAccountSecurityRequired || hostEnvironment?.chatgptBrowserMode === "AUTH_REQUIRED"))} onModeChange={setRoleMode} onBindingsChanged={() => { void refreshIndex(); }} />}</>}
    draft={workbenchDraft}
    handoff={handoff}
    goal={goal}
    lifecycle={selected ? {
      canArchive: lifecycleOf(selected) === "ACTIVE",
      canTrash: lifecycleOf(selected) !== "TRASHED",
      canRestore: lifecycleOf(selected) !== "ACTIVE",
      canPurge: lifecycleOf(selected) === "TRASHED",
      trashCount: items.filter((item) => item.lifecycle === "TRASHED").length,
      backupNotice,
    } : null}
    runtime={{ checks: [
      { id: "router-host", label: "Router 电脑程序", state: hostEnvironment?.host === "ONLINE" ? "READY" : "CHECKING", detail: hostEnvironment?.host === "ONLINE" ? (snapshot ? "Router Host 与本地工作区已读取" : "当前 Tauri Host 正在响应诊断请求") : "正在读取 Host 状态", action: refreshRuntime, actionLabel: "检查状态" },
      { id: "mobile-listener", label: "手机访问入口", state: hostEnvironment?.mobile.startsWith("AVAILABLE") ? "READY" : hostEnvironment ? "UNAVAILABLE" : "CHECKING", detail: hostEnvironment?.mobile ?? "正在读取 listener 状态", action: refreshRuntime, actionLabel: "检查连接" },
      { id: "browser-runtime", label: "AI Work Router Browser", state: chatGptAccountSecurityRequired ? "WARNING" : browserConfigured ? "READY" : "UNAVAILABLE", detail: chatGptAccountSecurityRequired ? "需要你在当前页面完成登录或安全验证。" : "产品自带 Chromium，使用专用浏览器资料。可用配置不等于已经登录。", action: chatGptAccountSecurityRequired ? undefined : openHostBrowserSetup, actionLabel: "打开 Router 浏览器", secondaryAction: () => void refreshBrowserStatus(), secondaryActionLabel: "检查浏览器状态" },
      { id: "chatgpt", label: "ChatGPT 新回复", state: selectedChatGptEndpoint ? "READY" : "UNAVAILABLE", detail: selectedChatGptEndpoint ? "手动检查只读取当前精确绑定；被动观察不可用时仍可手动检查。" : "先在两端绑定中选择现有对话。", action: selectedChatGptEndpoint && !chatGptAccountSecurityRequired ? () => void checkNewChatGptReplies() : undefined, actionLabel: "检查 ChatGPT 新回复" },
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
    ], onOpenDiagnostics: refreshRuntime, notice: runtimeAttentionNotice ?? chatGptRefreshStatus }}
    surface={surface}
    onSurfaceChange={(next) => { if (next === "PROJECT_HOME" && window.matchMedia?.("(max-width: 680px)").matches) openProject(); else { if (next === "PROJECT") setProjectEntry(null); setSurface(next); } }}
    projectPanel={connectionPanel}
    projectHomePanel={projectHomePanel}
    newWorkPanel={newWorkPanel}
    codexHistoryPanel={codexHistoryPanel}
    hasCodexEndpoint={Boolean(selectedCodexEndpoint)}
    chatgptResultsPanel={chatgptResultsPanel}
    codexResultsPanel={codexResultsPanel}
    codexFeedbackPanel={codexFeedbackPanel}
    codexRequestPanel={codexRequestPanel}
    providerRunStatusPanel={providerRunStatusPanel}
    codexResultCount={codexResults.length}
    codexRequestCount={codexRequests.filter((request) => !request.responseSent).length}
    inlineComposer={false}
    onSelectWorkstream={(id) => { setResultFocus(null); setAttentionTarget(null); setSurface("WORKSPACE"); void selectWorkstream(id).catch((error) => setRuntimeError(String(error))); }}
    onSelectAttention={(workstreamId, attention) => {
      setAttentionTarget({ workstreamId, sourceId: attention.sourceId, kind: attention.kind });
      setProviderRunStatus(null);
      void selectWorkstream(workstreamId).then((nextSnapshot) => {
        if (attention.kind === "CHATGPT_REPLY_OBSERVED" && attention.sourceId) setSelectedReply({ workstreamId, id: attention.sourceId });
        if (attention.kind === "CODEX_REPLY_OBSERVED" && attention.sourceId) setSelectedCodexObservation({ workstreamId, id: attention.sourceId });
        if (attention.kind === "CHATGPT_RESULT_READY" && attention.sourceId) { setResultFocus({ workstreamId, provider: "CHATGPT", runId: attention.sourceId }); setSurface("CHATGPT_RESULTS"); }
        else if (attention.kind === "CODEX_RESULT_READY" && attention.sourceId) { setResultFocus({ workstreamId, provider: "CODEX", runId: attention.sourceId }); setSurface("CODEX_RESULTS"); }
        else if (attention.kind === "CODEX_REPLY_OBSERVED") setSurface("CODEX_HISTORY");
        else if (attention.kind === "CODEX_STRUCTURED_REQUEST") setSurface("CODEX_REQUEST");
        else if (attention.kind === "HANDOFF_FAILED" || attention.kind === "DELIVERY_UNCERTAIN") {
          const exact = attention.sourceId && nextSnapshot ? handoffFromExactAttention(nextSnapshot, workstreamId, attention.sourceId) : null;
          if (!exact) {
            setRuntimeError("收件箱指定的精确交付记录当前不可读或不存在；Router 没有显示其他交付记录。");
            setSurface("INBOX");
            return;
          }
          setHandoff(exact);
          setSurface("HANDOFF_STATUS");
        }
        else if (attention.kind === "MISSING_CHATGPT_BINDING" || attention.kind === "MISSING_CODEX_BINDING") setSurface("PROJECT");
        else if (attention.kind === "PROVIDER_RUN_FAILED" || attention.kind === "PROVIDER_RUN_CANCELLED" || attention.kind === "UNKNOWN_RUN") {
          if (!attention.sourceId) {
            setRuntimeError("收件箱指定的 Provider 执行没有精确 ID；Router 不会猜测或显示其他执行。");
            setSurface("PROVIDER_RUN_STATUS");
            return;
          }
          void codexApi.providerRunStatus(workstreamId, attention.sourceId).then((status) => {
            setProviderRunStatus(status);
            setSurface("PROVIDER_RUN_STATUS");
          }).catch(() => {
            setProviderRunStatus(null);
            setSurface("PROVIDER_RUN_STATUS");
          });
        }
        else setSurface("INBOX");
      }).catch((error) => setRuntimeError(String(error)));
    }}
    onSelectRecycleWorkstream={(id) => { void loadSelected(id).catch((error) => setRuntimeError(String(error))); }}
    onPinChange={(id, pinned) => void codexApi.setWorkstreamPinned(id, pinned).then(refreshIndex).catch((error) => setRuntimeError(`置顶未保存：${String(error)}`))}
    onDraftChange={changeDraft}
    onSendDiscussion={CHATGPT_ROUTER_OWNED_CARRIER_PAUSED ? undefined : (value) => {
      if (!selectedWorkstreamId || !value.trim()) return Promise.resolve();
      return codexApi.sendChatGptRequest(selectedWorkstreamId, value)
        .then(async () => { await loadSelected(selectedWorkstreamId).catch((error) => {
          setRuntimeError(`讨论已发送，但刷新当前工作区失败：${String(error)}`);
        }); })
        .catch((error) => {
          if (String(error).includes("CHATGPT_ACCEPTED_PENDING_TERMINAL")) { setRuntimeError(null); setChatGptRefreshStatus("已发送，等待 ChatGPT 回复。送达与完成分别记录。"); return refreshIndex(); }
          setRuntimeError(`送达未确认；请检查原记录，Router 不会自动重发：${String(error)}`);
          throw error;
        });
    }}
    onOpenManualDiscussionDestination={selectedChatGptEndpoint ? openBoundChatGptInDefaultBrowser : undefined}
    onReplyRead={(id) => void markReply(id, false).catch((error) => setRuntimeError(String(error)))}
    onReplyHandled={(id) => void markReply(id, true).catch((error) => setRuntimeError(String(error)))}
    onRetryCurrentChatGptHistory={!CHATGPT_NORMAL_BROWSER_OBSERVER_ENABLED ? undefined : chatGptAccountSecurityRequired ? undefined : () => {
      if (selectedWorkstreamId && selectedChatGptEndpoint) refreshCurrentChatGptHistory(selectedWorkstreamId, selectedChatGptEndpoint, false);
    }}
    onCheckNewChatGptReplies={CHATGPT_NORMAL_BROWSER_OBSERVER_ENABLED && selectedChatGptEndpoint && !chatGptAccountSecurityRequired ? () => void checkNewChatGptReplies() : undefined}
    chatGptRefreshStatus={chatGptRefreshStatus}
    chatGptRefreshBusy={chatGptRefreshBusy || chatGptAccountSecurityRequired}
    onOpenBoundChatGptInDefaultBrowser={selectedChatGptEndpoint ? openBoundChatGptInDefaultBrowser : undefined}
    onOpenHandoffDestination={openApprovedHandoffDestination}
    onOpenHostChatGptBrowserSetup={CHATGPT_ROUTER_OWNED_CARRIER_PAUSED || chatGptAccountSecurityRequired ? undefined : openHostBrowserSetup}
    onPrepareHandoff={reply?.provider === "CHATGPT" && Boolean(selectedObservation?.markerText?.trim()) ? (replyId) => prepareChatGptHandoff(replyId).catch((error) => { setRuntimeError(`无法准备 Handoff：${String(error)}`); throw error; }) : undefined}
    onAddSelectionToHandoff={reply?.provider === "CHATGPT" ? (replyId, text) => prepareChatGptHandoff(replyId, text).catch((error) => { setRuntimeError(`无法准备 Handoff：${String(error)}`); throw error; }) : undefined}
    onHandoffMessageChange={updateHandoffMessage}
    onSelectHandoffAttachments={(id, filenames) => selectChatGptHandoffAttachments(id, filenames).catch((error) => { setRuntimeError(`无法确认 ChatGPT 附件：${String(error)}`); throw error; })}
    onOpenCodexResults={() => {
      const sourceId = handoff?.direction === "CODEX_TO_CHATGPT" ? handoff.candidates?.[0]?.id : null;
      const observation = sourceId ? replyObservations.find((item) => item.id === sourceId && item.endpointId === selectedCodexEndpoint?.id) ?? null : null;
      if (observation && selectedWorkstreamId) {
        setSelectedCodexObservation({ workstreamId: selectedWorkstreamId, id: observation.id });
        setSurface("CODEX_HISTORY");
        return;
      }
      if (handoff?.origin?.sourceKind === "REPLY_OBSERVATION") {
        setRuntimeError("这条精确 Codex 外部回复当前不可读；为避免把历史执行结果当成原回复，Router 没有切换页面。");
        return;
      }
      if (sourceId && !reviewResults.some((item) => item.provider === "CODEX" && item.runId === sourceId)) {
        setRuntimeError("这条精确 Codex 历史结果当前不可读；Router 没有切换到其他结果。");
        return;
      }
      setSurface("CODEX_RESULTS");
    }}
    onOpenChatGptOrigin={() => {
      const sourceId = handoff?.direction === "CHATGPT_TO_CODEX" ? handoff.candidates?.[0]?.id : null;
      const observation = sourceId ? replyObservations.find((item) => item.id === sourceId && item.endpointId === selectedChatGptEndpoint?.id) ?? null : null;
      if (observation && selectedWorkstreamId) {
        setSelectedReply({ workstreamId: selectedWorkstreamId, id: observation.id });
        setSurface("WORKSPACE");
        return;
      }
      setRuntimeError("这条精确 ChatGPT 外部回复当前不可读；Router 没有切换到其他回复。");
    }}
    onReopenReverseHandoff={() => {
      const sourceId = handoff?.direction === "CODEX_TO_CHATGPT" ? handoff.candidates?.[0]?.id : null;
      const observation = sourceId ? replyObservations.find((item) => item.id === sourceId && item.endpointId === selectedCodexEndpoint?.id) ?? null : null;
      if (observation) {
        void prepareCodexObservationHandoff(observation).then(() => setSurface("HANDOFF_REVIEW")).catch((error) => setRuntimeError(`无法重新准备外部 Codex 回复回传：${String(error)}`));
        return;
      }
      if (handoff?.origin?.sourceKind === "REPLY_OBSERVATION") {
        setRuntimeError("这条精确 Codex 外部回复当前不可读；未用历史执行结果替代，也未创建新的编辑审阅。");
        return;
      }
      const result = reviewResults.find((item) => item.runId === sourceId);
      if (!result) {
        setRuntimeError("原 Codex 结果当前不可读，未创建新的编辑审阅。");
        return;
      }
      void prepareCodexHandoff(result).then(() => setSurface("HANDOFF_REVIEW")).catch((error) => setRuntimeError(`无法重新准备 Codex Handoff：${String(error)}`));
    }}
    onApproveHandoff={(id) => approveHandoff(id).catch((error) => {
      setRuntimeError(`无法批准 Handoff：${String(error)}`);
      throw error;
    })}
    onSendHandoff={(id) => sendHandoff(id).catch((error) => {
      setRuntimeError(`无法发送 Handoff：${String(error)}`);
      throw error;
    })}
    onLifecycleChange={(id, lifecycle) => changeLifecycle(id, lifecycle)}
    onPurgeTrashedWorkstream={(id) => { const target = snapshot?.workstreams.find((workstream) => workstream.id === id); if (!target) return; void codexApi.purgeTrashedWorkstream(id, target.bindingRevision, "PURGE_LOCAL_WORKSTREAM").then(async () => { setSelectedWorkstreamId(null); await refreshIndex(); }).catch((error) => setRuntimeError(`永久清除未执行：${String(error)}`)); }}
    onCreateVerifiedBackup={() => void codexApi.createVerifiedLocalBackup().then((backup) => setBackupNotice(`已创建并验证本地备份：${backup.path} · ${backup.bytes} bytes · SHA-256 ${backup.sha256}`)).catch((error) => setRuntimeError(`一致性备份失败：${String(error)}`))}
    onGoalAction={(threadId, action) => void changeGoal(threadId, action).catch((error) => setRuntimeError(String(error)))}
    onHideToTray={hideToTray}
  /></>;
}
