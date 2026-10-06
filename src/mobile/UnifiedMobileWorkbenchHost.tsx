import {flushSync} from "react-dom";
import {NotificationDeliveryPanel} from "../features/workbench/NotificationDeliveryPanel";
import {WebSessionLogout} from "./WebLogin";
import { NotificationAssistantSettings, CodexNotifications } from "../features/workbench/CodexNotifications";
import { webNotificationApi } from "../features/workbench/notificationApi";
import { RoleBridgePanel } from "../features/workbench/RoleBridgePanel";
import { mobileRoleBridgeApi } from "./roleBridgeApi";
import { ObservationAttachmentSelection } from "../features/workbench/ObservationAttachmentSelection";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { BridgeStatus, securityRecoveryError } from "../features/workbench/BridgeStatus";
import { LEGACY_WORKBENCH_DETAILS, UnifiedWorkbench, type WorkbenchSurface } from "../features/workbench/UnifiedWorkbench";
import { MarkdownMessage } from "../features/codex/MarkdownMessage";
import { ProjectConnectionPanel, type ConnectionChoice, type ExplicitChatGptConfirmationState } from "../features/workbench/ProjectConnectionPanel";
import type { ExactReply, GoalPresentation, HandoffPresentation, WorkbenchItem, WorkbenchLifecycle } from "../features/workbench/models";
import { useDraftAutosaveProjection } from "../features/workbench/useDraftAutosaveProjection";
import { useResultScopedDraftAutosave } from "../features/workbench/useResultScopedDraftAutosave";
import { mobileApi, type MobileCodexRequest, type MobileConfirmedChatGptEndpoint, type MobileExplicitChatGptBindingCandidate, type MobileExternalProjectLink, type MobilePersistedChatGptInboundReview, type MobilePersistedCodexOutboundReview, type MobilePersistedHandoff, type MobileProviderRunStatus, type MobileReplyObservation, type MobileReview, type MobileReviewResult, type MobileUnprojectedThreadStart, type MobileWorkstream } from "./api";
import { disableWebPush, enableWebPush, pushSetupState, type PushSetupState } from "./push";

// Mobile remains a presentation client.  It may request one normal-Chrome
// read-only exact-page observation from the Host; it cannot navigate, submit,
// or become a second provider writer.
const CHATGPT_NORMAL_BROWSER_OBSERVER_ENABLED = true;

type MobileSnapshot = Awaited<ReturnType<typeof mobileApi.workstream>>;

function normalChromeConnectionMessage(_cause: unknown) {
  return "旧的浏览器观察路径已停用。请使用电脑上的 AI Work Router Browser 核对当前绑定对话；手机端不会启动旧链路。";
}

/** Shows only timestamps persisted on this exact ProviderRun.  It does not
 * perform a provider read or infer a status from a newer run. */
function providerRunTimestamp(value?: number | null) {
  return value ? new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(value) : "Router 尚未记录";
}

function isExactConfirmedChatGptEndpoint(endpoint: MobileConfirmedChatGptEndpoint, workstreamId: string, externalId: string) {
  return endpoint.provider === "CHATGPT" && endpoint.workstreamId === workstreamId && endpoint.externalId === externalId;
}

function isExactRefreshedChatGptEndpoint(snapshot: MobileSnapshot, workstreamId: string, confirmed: MobileConfirmedChatGptEndpoint) {
  return snapshot.selectedWorkstreamId === workstreamId
    && snapshot.activeChatgptEndpoint?.provider === "CHATGPT"
    && snapshot.activeChatgptEndpoint.workstreamId === workstreamId
    && snapshot.activeChatgptEndpoint.id === confirmed.id
    && snapshot.activeChatgptEndpoint?.externalId === confirmed.externalId;
}

/** Reopens only a durable reverse Handoff whose endpoints still exactly match
 * the current binding. Session action IDs may disappear after a Host restart. */
function recoverOutboundHandoff(snapshot: MobileSnapshot, workstreamId: string): HandoffPresentation | null {
  const codex = snapshot.activeCodexEndpoint;
  const chatgpt = snapshot.activeChatgptEndpoint;
  if (!codex || !chatgpt) return null;
  const candidates = (snapshot.handoffs ?? [])
    .filter((handoff: MobilePersistedHandoff) => handoff.workstreamId === workstreamId
      && handoff.direction === "CODEX_TO_CHATGPT"
      && handoff.sourceKind === "ENDPOINT"
      && handoff.sourceEndpoint?.id === codex.id
      && handoff.sourceEndpoint.externalId === codex.externalId
      && handoff.destinationEndpoint.id === chatgpt.id
      && handoff.destinationEndpoint.externalId === chatgpt.externalId
      && (handoff.status === "SENDING" || handoff.status === "FAILED"))
    .sort((left, right) => right.createdAt - left.createdAt);
  // A stale mobile action ID must never be used to silently substitute a
  // different delivery attempt.  A durable recovery is safe only when one
  // exact current-binding attempt exists; otherwise Inbox retains each record
  // as a separate owner decision.
  if (candidates.length === 1) {
    const candidate = candidates[0];
    if (!candidate) return null;
    return {
      id: candidate.id,
      handoffId: candidate.id,
      status: candidate.status,
      direction: "CODEX_TO_CHATGPT",
      message: candidate.approvedText,
      canApprove: false,
      canSend: false,
      deliveryCode: candidate.errorCode ?? null,
      deliveryDetail: candidate.errorMessage ?? null,
      candidates: [{
        id: candidate.id,
        label: "已保存的精确交付记录",
        text: "",
        sourceLabel: "Router 已保存的 CODEX_TO_CHATGPT Handoff",
      }],
      destination: {
        provider: "CHATGPT",
        label: "当前精确绑定的 ChatGPT 对话",
        externalId: chatgpt.externalId,
      },
      origin: { provider: "CODEX" },
    };
  }
  // An approved manual review exists before a provider Handoff.  It must be
  // reachable after ordinary workspace navigation, but only if its exact
  // source and destination still match the current binding.  Do not choose a
  // "latest" review when multiple owner decisions remain pending.
  if (candidates.length !== 0) return null;
  const reviews = (snapshot.mobileCodexOutboundReviews ?? [])
    .filter((review: MobilePersistedCodexOutboundReview) => review.workstreamId === workstreamId
      && review.handoffId == null
      && (review.status === "READY" || review.status === "APPROVED")
      && review.sourceEndpointId === codex.id
      && review.sourceCodexThreadId === codex.externalId
      && review.destinationChatgptConversationId === chatgpt.externalId)
    .sort((left, right) => right.updatedAt - left.updatedAt);
  if (reviews.length !== 1) return null;
  const review = reviews[0];
  if (!review) return null;
  const sourceKind = review.sourceRunId ? "PROVIDER_RUN" : "REPLY_OBSERVATION";
  return {
    id: review.actionId,
    status: review.status,
    direction: "CODEX_TO_CHATGPT",
    message: review.approvedText ?? review.originalText,
    canApprove: review.status === "READY",
    // Recovery must never acquire a provider writer.  The persisted review can
    // still be opened, edited, approved, and copied to its exact destination.
    canSend: review.status === "APPROVED",
    requiresManualDispatch: false,
    candidates: [{
      id: review.sourceReferenceId,
      label: sourceKind === "REPLY_OBSERVATION" ? "已保存的精确 Codex 外部回复" : "已保存的精确 Codex 执行结果",
      text: "",
      sourceLabel: `精确 Router 来源记录 · ${sourceKind}`,
    }],
    destination: {
      provider: "CHATGPT",
      label: "本次审阅保存的精确 ChatGPT 对话",
      externalId: review.destinationChatgptConversationId,
    },
    origin: { provider: "CODEX", sourceKind },
  };
}

function recoverInboundHandoff(snapshot: MobileSnapshot, workstreamId: string): HandoffPresentation | null {
  const chatgpt = snapshot.activeChatgptEndpoint;
  const codex = snapshot.activeCodexEndpoint;
  if (!chatgpt || !codex) return null;
  const reviews = (snapshot.mobileChatgptInboundReviews ?? [])
    .filter((review: MobilePersistedChatGptInboundReview) => review.workstreamId === workstreamId
      && review.handoffId == null
      && (review.status === "READY" || review.status === "APPROVED")
      && review.sourceEndpointId === chatgpt.id
      && review.sourceChatgptConversationId === chatgpt.externalId
      && review.destinationCodexThreadId === codex.externalId)
    .sort((left, right) => right.updatedAt - left.updatedAt);
  if (reviews.length !== 1) return null;
  const review = reviews[0];
  if (!review) return null;
  const sourceKind = review.sourceRunId ? "PROVIDER_RUN" : "REPLY_OBSERVATION";
  return {
    id: review.actionId,
    status: review.status,
    direction: "CHATGPT_TO_CODEX",
    message: review.approvedText ?? review.originalText,
    canApprove: review.status === "READY",
    canSend: review.status === "APPROVED",
    candidates: [{
      id: review.sourceReferenceId,
      label: sourceKind === "REPLY_OBSERVATION" ? "已保存的精确 ChatGPT 外部回复" : "已保存的精确 ChatGPT 执行结果",
      text: "",
      sourceLabel: `精确 Router 来源记录 · ${sourceKind}`,
    }],
    destination: {
      provider: "CODEX",
      label: "本次审阅保存的精确 Codex 对话",
      externalId: review.destinationCodexThreadId,
    },
    origin: { provider: "CHATGPT", sourceKind },
  };
}

function recoverPendingHandoff(snapshot: MobileSnapshot, workstreamId: string): HandoffPresentation | null {
  const outbound = recoverOutboundHandoff(snapshot, workstreamId);
  const inbound = recoverInboundHandoff(snapshot, workstreamId);
  // Two independent owner decisions must stay separate.  Returning neither is
  // safer than selecting whichever happened to be most recently updated.
  return outbound && inbound ? null : outbound ?? inbound;
}

/** Opens exactly the durable Handoff referenced by an Inbox attention item.
 * Unlike ordinary workspace recovery, this path never falls back to the
 * newest/current-binding attempt when several delivery records exist. */
function handoffFromExactAttention(snapshot: MobileSnapshot, workstreamId: string, handoffId: string): HandoffPresentation | null {
  const record = (snapshot.handoffs ?? []).find((candidate: MobilePersistedHandoff) => candidate.id === handoffId && candidate.workstreamId === workstreamId);
  if (!record) return null;
  const destinationProvider = record.direction === "CODEX_TO_CHATGPT" ? "CHATGPT" : "CODEX";
  const originProvider = record.direction === "CODEX_TO_CHATGPT" ? "CODEX" : "CHATGPT";
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
    destination: { provider: destinationProvider, label: destinationProvider === "CHATGPT" ? "本次交付保存的精确 ChatGPT 对话" : "本次交付保存的精确 Codex 对话", externalId: record.destinationEndpoint.externalId },
    origin: { provider: originProvider },
  };
}

function lifecycleOf(workstream: { status: string; trashedAt?: number | null }): WorkbenchLifecycle {
  return workstream.trashedAt ? "TRASHED" : workstream.status === "ARCHIVED" ? "ARCHIVED" : "ACTIVE";
}

function codexFeedbackDraftKey(workstreamId: string, resultRunId: string) {
  return JSON.stringify([workstreamId, resultRunId]);
}

function attachmentCanBeSelected(integrityStatus: string) {
  return integrityStatus === "VERIFIED";
}

function attachmentRelayExplanation(integrityStatus: string, warnings: string[] = []) {
  if (integrityStatus === "VERIFIED") return "已验证；可在审阅时选择转发。";
  const warning = warnings.join(" ").toLowerCase();
  if (warning.includes("local path was not found") || warning.includes("cannot be relayed")) return "本地文件已不存在，不能随这次交接转发。";
  if (integrityStatus === "MISMATCH") return "文件校验不一致，不能随这次交接转发。";
  return "此附件当前不可安全转发。";
}

function defaultAttachmentIds(result: MobileReviewResult) {
  return (result.attachments ?? []).filter((attachment) => attachment.defaultSelected && attachmentCanBeSelected(attachment.integrityStatus)).map((attachment) => attachment.id);
}

function codexFeedbackDraftTarget(scopeId: string) {
  const parsed: unknown = JSON.parse(scopeId);
  if (!Array.isArray(parsed) || parsed.length !== 2 || !parsed.every((value) => typeof value === "string")) {
    throw new Error("Invalid Codex feedback draft scope");
  }
  return { workstreamId: parsed[0], resultRunId: parsed[1] };
}

function retainCodexHandoffLocation(workstreamId: string, actionId: string) {
  const target = new URL(window.location.href);
  target.searchParams.set("workstream", workstreamId);
  target.searchParams.set("handoff", actionId);
  window.history.replaceState({}, "", `${target.pathname}${target.search}`);
}

function isMobileAccessAuthenticationError(cause: unknown) {
  return typeof cause === "object"
    && cause !== null
    && "status" in cause
    && ((cause as { status?: unknown }).status === 401 || (cause as { status?: unknown }).status === 403);
}

function MobileRuntimeUnavailable({ onRetry, accessAuthenticationRequired }: { onRetry: () => void; accessAuthenticationRequired: boolean }) {
  const goBack = () => window.history.back();
  if (accessAuthenticationRequired) {
    return <main className="v3-mobile-runtime-unavailable" aria-label="手机访问需要重新认证">
      <header><button type="button" onClick={goBack}>‹ 返回</button><h1>手机访问</h1></header>
      <section><span>访问会话需要重新认证</span><h2>需要重新登录手机访问</h2><p>Cloudflare Access 会话已失效，或当前访问尚未获得授权。Router 和已保存的工作不会因此被删除。</p><p className="v3-mobile-runtime-detail">这不是 ChatGPT 登录，也不表示电脑上的 Router 已关闭。请不要反复刷新。</p><article><h3>下一步</h3><p>重新打开手机访问页，按 Cloudflare Access 的正常登录流程完成认证后，再返回这里。</p><p>认证完成前，手机不能读取新内容或提交操作。</p></article></section>
      <footer><button type="button" className="v3-primary" onClick={onRetry}>重新打开手机访问页</button><button type="button" onClick={goBack}>返回仍保留的页面</button></footer>
    </main>;
  }
  return <main className="v3-mobile-runtime-unavailable" aria-label="手机端运行状态不可用">
    <header><button type="button" onClick={goBack}>‹ 返回</button><h1>运行状态</h1></header>
    <section><span>暂时连不上 Router</span><h2>无法读取 Router 状态</h2><p>手机不能读取新内容，也不能发送消息。此前已显示的内容不会被空白覆盖。</p><p className="v3-mobile-runtime-detail">Router 状态请求未成功；不会把它解释为电脑已关闭。</p><article><h3>在 Windows 上检查 AI Work Router</h3><p>确认“运行环境”显示可用后，再回到手机刷新。</p><p>手机不能直接启动一台已关闭的电脑。</p></article></section>
    <footer><button type="button" className="v3-primary" onClick={onRetry}>重新连接</button><button type="button" onClick={goBack}>返回仍保留的页面</button></footer>
  </main>;
}

/** The mobile host is a thin authenticated projection over the same Router
 * commands used by desktop. It has no provider, browser, or SQLite access. */
export function UnifiedMobileWorkbenchHost({initialSurface="BRIDGES"}:{initialSurface?:WorkbenchSurface}={}) {
  const [notificationCount,setNotificationCount]=useState(0);
  const [notificationDetail,setNotificationDetail]=useState(false);
  const [index, setIndex] = useState<MobileWorkstream[]>([]);
  const [snapshot, setSnapshot] = useState<MobileSnapshot | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const selectedWorkstreamRef = useRef<string | null>(null);
  const [codexHistory, setCodexHistory] = useState<{ id: string; kind: string; text?: string }[]>([]);
  const [observations, setObservations] = useState<MobileReplyObservation[]>([]);
  const [goal, setGoal] = useState<GoalPresentation | null>(null);
  const [manualCheckNotice, setManualCheckNotice] = useState<string | null>(null);
  const [pushSetup, setPushSetup] = useState<PushSetupState | "CHECKING">("CHECKING");
  const [routerSubscriptionCount, setRouterSubscriptionCount] = useState<number | null>(null);
  const [pushBusy, setPushBusy] = useState(false);
  const [pushPanelExpanded, setPushPanelExpanded] = useState(false);
  const [observerDetailsOpen, setObserverDetailsOpen] = useState(false);
  const { beginLoad: beginDraftLoad, changeDraft, draft: workbenchDraft, hydrate: hydrateDraft } = useDraftAutosaveProjection((workstreamId, text, expectedRevision) => mobileApi.saveWorkstreamDraft(workstreamId, text, expectedRevision));
  const [error, setError] = useState<string | null>(null);
  const [browserAuthenticationRequired, setBrowserAuthenticationRequired] = useState(false);
  const [accessAuthenticationRequired, setAccessAuthenticationRequired] = useState(false);
  const [handoff, setHandoff] = useState<HandoffPresentation | null>(null);
  const handoffSendRef = useRef<string | null>(null);
  const [reviewResults, setReviewResults] = useState<MobileReviewResult[]>([]);
  const [codexRequests, setCodexRequests] = useState<MobileCodexRequest[]>([]);
  const [codexRequestSendingId, setCodexRequestSendingId] = useState<string | null>(null);
  const codexRequestResponseRef = useRef<string | null>(null);
  const [codexRequestDecisions, setCodexRequestDecisions] = useState<Record<string, "accept" | "decline">>({});
  const [selectedAttachmentIds, setSelectedAttachmentIds] = useState<Record<string, string[]>>({});
  const [codexAttachmentRunId, setCodexAttachmentRunId] = useState<string | null>(null);
  const [codexFeedbackTarget, setCodexFeedbackTarget] = useState<{ workstreamId: string; runId: string } | null>(null);
  const codexFeedbackAutosave = useResultScopedDraftAutosave(
    async (scopeId) => {
      const target = codexFeedbackDraftTarget(scopeId);
      const draft = await mobileApi.codexFeedbackDraft(target.workstreamId, target.resultRunId);
      return draft && { text: draft.text, revision: draft.revision, updatedAt: draft.updatedAt };
    },
    async (scopeId, text, expectedRevision) => {
      const target = codexFeedbackDraftTarget(scopeId);
      const draft = await mobileApi.saveCodexFeedbackDraft(target.workstreamId, target.resultRunId, text, expectedRevision);
      return { text: draft.text, revision: draft.revision, updatedAt: draft.updatedAt };
    },
  );
  const [codexFeedbackSending, setCodexFeedbackSending] = useState(false);
  const codexFeedbackSendRef = useRef(false);
  const [codexFeedbackError, setCodexFeedbackError] = useState<string | null>(null);
  const [projectLinks, setProjectLinks] = useState<MobileExternalProjectLink[]>([]);
  const [codexProjectId, setCodexProjectId] = useState("");
  const [unprojectedDirectory, setUnprojectedDirectory] = useState("");
  const [unprojectedThread, setUnprojectedThread] = useState<MobileUnprojectedThreadStart | null>(null);
  const [backupNotice, setBackupNotice] = useState<string | null>(null);
  const [explicitChatGptUrl, setExplicitChatGptUrl] = useState("");
  const [explicitChatGptCandidate, setExplicitChatGptCandidate] = useState<MobileExplicitChatGptBindingCandidate | null>(null);
  const [explicitChatGptConfirmation, setExplicitChatGptConfirmation] = useState<{ state: ExplicitChatGptConfirmationState; message: string | null }>({ state: "IDLE", message: null });
  const explicitChatGptConfirmationRef = useRef(false);
  const [codexThreadId, setCodexThreadId] = useState("");
  const [codexThreadLabel, setCodexThreadLabel] = useState("");
  const [surface, setSurface] = useState<WorkbenchSurface>(initialSurface);
  const [roleMode, setRoleMode] = useState(false);
  const [bindingRequest, setBindingRequest] = useState(0);
  const roleApi = useMemo(() => selectedId ? mobileRoleBridgeApi(selectedId) : null, [selectedId]);
  const [attentionTarget, setAttentionTarget] = useState<{ workstreamId: string; sourceId?: string; kind: string } | null>(null);
  const [providerRunStatus, setProviderRunStatus] = useState<MobileProviderRunStatus | null>(null);
  const restoredHandoffRef = useRef<string | null>(null);
  const notificationTarget = useMemo(() => {
    const query = new URLSearchParams(window.location.search);
    return {
      workstreamId: query.get("workstream")?.trim() || null,
      replyId: query.get("reply")?.trim() || null,
      handoffId: query.get("handoff")?.trim() || null,
    };
  }, []);

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
    const next = await mobileApi.workstreams();
    setAccessAuthenticationRequired(false);
    setIndex(next);
    if (notificationTarget.replyId && !notificationTarget.workstreamId) {
      setSelectedId(null);
      setError("通知回复缺少精确工作区 ID，未选择其他工作区。");
      return;
    }
    if (notificationTarget.workstreamId) {
      if (!next.some((item) => item.id === notificationTarget.workstreamId)) {
        setSelectedId(null);
        setSnapshot(null);
        setObservations([]);
        setError("通知指定的工作区当前不可用，未回退到其他工作区。");
        return;
      }
      setSelectedId(notificationTarget.workstreamId);
      return;
    }
    setSelectedId((current) => current ?? next[0]?.id ?? null);
  }, [notificationTarget]);

  const load = useCallback(async (workstreamId: string, preserveExplicitChatGptCandidate = false) => {
    const draftLoadGeneration = beginDraftLoad(workstreamId);
    setSelectedId(workstreamId);
    if (!preserveExplicitChatGptCandidate) {
      setExplicitChatGptUrl("");
      setExplicitChatGptCandidate(null);
      setExplicitChatGptConfirmation({ state: "IDLE", message: null });
      explicitChatGptConfirmationRef.current = false;
    }
    setCodexThreadId("");
    setCodexThreadLabel("");
    setError(null);
    setHandoff(null);
    // Never render a prior Endpoint's observation while an exact current
    // Workstream projection is pending.
    setObservations([]);
    const nextSnapshot=await mobileApi.workstream(workstreamId);
    const hasCodex=LEGACY_WORKBENCH_DETAILS&&Boolean(nextSnapshot.activeCodexEndpoint);
    const hasProvider=LEGACY_WORKBENCH_DETAILS&&(hasCodex||Boolean(nextSnapshot.activeChatgptEndpoint));
    const [history, nextGoal, nextDraft, nextObservations, nextReviewResults, nextProjectLinks, nextCodexRequests] = await Promise.all([
      hasCodex?mobileApi.codexHistory(workstreamId).catch(() => ({ history: [] })):Promise.resolve({history:[]}),
      hasCodex?mobileApi.codexGoal(workstreamId).catch(() => null):Promise.resolve(null),
      mobileApi.workstreamDraft(workstreamId),
      hasProvider?mobileApi.replyObservations(workstreamId).catch(() => []):Promise.resolve([]),
      mobileApi.reviewResults(workstreamId).catch(() => []),
      mobileApi.projectLinks(workstreamId).catch(() => []),
      hasCodex&&typeof mobileApi.codexRequests === "function" ? mobileApi.codexRequests(workstreamId).catch(() => []) : Promise.resolve([]),
    ]);
    setSnapshot(nextSnapshot);
    setHandoff(recoverPendingHandoff(nextSnapshot, workstreamId));
    setCodexHistory(history.history);
    setObservations(nextObservations);
    setReviewResults(nextReviewResults);
    setProjectLinks(nextProjectLinks);
    setCodexRequests(nextCodexRequests);
    if (notificationTarget.replyId && workstreamId === notificationTarget.workstreamId && !nextObservations.some((item) => item.id === notificationTarget.replyId)) {
      setError("通知指定的精确回复当前不可读或不存在，未显示其他回复。");
    }
    hydrateDraft(workstreamId, nextDraft, draftLoadGeneration);
    const endpoint = nextSnapshot.activeCodexEndpoint;
    setGoal(nextGoal && endpoint ? {
      threadId: nextGoal.threadId,
      text: nextGoal.objective,
      status: nextGoal.status.toUpperCase() as GoalPresentation["status"],
      readAt: Date.now(),
      reportedAt: nextGoal.updatedAt ?? null,
      usageLabel: nextGoal.tokensUsed == null ? undefined : `${nextGoal.tokensUsed} tokens`,
      activeTurnId: nextGoal.activeTurnId,
      controllableActions: nextGoal.status === "active" ? ["PAUSE", "DELETE", ...(nextGoal.activeTurnId ? ["STOP_TURN" as const] : [])] : nextGoal.status === "paused" ? ["RESUME", "DELETE"] : ["DELETE"],
    } : endpoint ? { threadId: endpoint.externalId, status: "NONE", readAt: Date.now(), controllableActions: [] } : null);
    return nextSnapshot;
  }, [beginDraftLoad, hydrateDraft, notificationTarget]);

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
      attentionCount: item.attentionCount,
      attentionItems: item.attentionItems,
      sourceLabel: item.sourceLabel || (item.sourceLabel===undefined?item.bindingSummary:undefined),
      // A provider run state is not an owner action. The Inbox renders each
      // exact attention item instead of letting this summary masquerade as the
      // currently selected reply.
      statusLabel: item.attentionItems[0]?.message ?? item.chatgptStatus ?? item.codexStatus ?? "状态待读取",
    };
  }), [index, snapshot]);
  const focusedReplyId = notificationTarget.replyId ?? (attentionTarget?.workstreamId === selectedId && (attentionTarget.kind === "CHATGPT_REPLY_OBSERVED" || attentionTarget.kind === "CODEX_REPLY_OBSERVED") ? attentionTarget.sourceId ?? null : null);
  const targetedObservation = focusedReplyId && selectedId === (notificationTarget.workstreamId ?? attentionTarget?.workstreamId)
    ? observations.find((item) => item.id === focusedReplyId) ?? null
    : null;
  const latestObservation = focusedReplyId ? targetedObservation : observations[0];
  const latestObservationProvider = latestObservation?.endpointId === snapshot?.activeCodexEndpoint?.id
    ? "CODEX"
    : latestObservation?.endpointId === snapshot?.activeChatgptEndpoint?.id
      ? "CHATGPT"
      : !latestObservation?.endpointId && snapshot?.activeChatgptEndpoint
        ? "CHATGPT"
      : null;
  const selectedCodexObservationId = attentionTarget?.workstreamId === selectedId
    && attentionTarget.kind === "CODEX_REPLY_OBSERVED"
    ? attentionTarget.sourceId ?? null : null;
  const exactCodexObservation = selectedCodexObservationId && snapshot?.activeCodexEndpoint
    ? observations.find((item) => item.id === selectedCodexObservationId && item.endpointId === snapshot?.activeCodexEndpoint?.id) ?? null
    : null;
  const latestCodexObservation = snapshot?.activeCodexEndpoint
    ? selectedCodexObservationId
      ? exactCodexObservation
      : observations.filter((item) => item.endpointId === snapshot.activeCodexEndpoint?.id).sort((left, right) => right.observedAt - left.observedAt)[0] ?? null
    : null;
  const latestCodex = [...codexHistory].reverse().find((event) => event.kind === "AgentMessage" && event.text?.trim());
  const reply: ExactReply | null = latestObservation && latestObservationProvider && selectedId ? {
    id: latestObservation.id, workstreamId: selectedId, provider: latestObservationProvider, title: selected?.name ?? `${latestObservationProvider === "CODEX" ? "Codex" : "ChatGPT"} 完整回复`,
    observedAt: latestObservation.observedAt, readState: latestObservation.handledAt ? "HANDLED" : latestObservation.readAt ? "READ" : "UNREAD",
    text: latestObservation.text, completeness: "COMPLETE", sourceLabel: `Router 已观察到的精确 ${latestObservationProvider === "CODEX" ? "Codex" : "ChatGPT"} 回复`,
    notificationState: latestObservation.pushState, notificationRenderedAt: latestObservation.pushRenderedAt,
  } : !focusedReplyId && latestCodex && snapshot?.activeCodexEndpoint && selectedId ? {
    id: latestCodex.id, workstreamId: selectedId, provider: "CODEX", title: selected?.name ?? "Codex 完整回复",
    observedAt: selected?.updatedAt ?? null, readState: "READ", text: latestCodex.text ?? "", completeness: "COMPLETE",
    sourceLabel: `精确线程 ${snapshot.activeCodexEndpoint.externalId.slice(0, 8)}…`,
  } : null;

  useEffect(() => {
    if (focusedReplyId && reply?.id === focusedReplyId) {
      document.getElementById(`reply-observation-${focusedReplyId}`)?.focus();
    }
  }, [focusedReplyId, reply?.id]);

  const markReply = async (replyId: string, handled: boolean) => {
    if (!selectedId) return;
    if (handled) await mobileApi.markReplyObservationHandled(selectedId, replyId);
    else await mobileApi.markReplyObservationRead(selectedId, replyId);
    await load(selectedId);
  };
  const changeLifecycle = async (workstreamId: string, lifecycle: WorkbenchLifecycle) => {
    if (lifecycle === "ARCHIVED") await mobileApi.archiveWorkstream(workstreamId);
    else if (lifecycle === "TRASHED") await mobileApi.trashWorkstream(workstreamId);
    else await mobileApi.restoreWorkstream(workstreamId);
    await refreshIndex();
    await load(workstreamId);
  };
  const changeGoal = async (_threadId: string, action: "PAUSE" | "RESUME" | "DELETE" | "STOP_TURN") => {
    if (!selectedId) return;
    if (action === "PAUSE") await mobileApi.pauseCodexGoal(selectedId);
    else if (action === "RESUME") await mobileApi.resumeCodexGoal(selectedId);
    else if (action === "DELETE") await mobileApi.clearCodexGoal(selectedId);
    else {
      if (!goal?.activeTurnId) throw new Error("Router 未观察到当前精确 Turn；没有发送中断。");
      await mobileApi.interruptCodexTurn(selectedId, goal.activeTurnId);
    }
    await load(selectedId);
  };
  const respondToCodexRequest = async (request: MobileCodexRequest, decision: "accept" | "decline") => {
    if (!selectedId || request.responseSent || codexRequestResponseRef.current) return;
    const available = new Set((request.choices ?? []).map((choice) => choice.id));
    if (!available.has(decision)) throw new Error("Router 没有提供该一次性回应选项；未发送任何选择。");
    codexRequestResponseRef.current = request.requestId;
    setCodexRequestSendingId(request.requestId);
    try {
      await mobileApi.respondToCodexRequest(selectedId, request.requestId, { revision: request.revision, decision });
      setCodexRequests((current) => current.map((item) => item.requestId === request.requestId ? { ...item, responseSent: true } : item));
      setCodexRequestDecisions((current) => ({ ...current, [request.requestId]: decision }));
    } finally {
      if (codexRequestResponseRef.current === request.requestId) codexRequestResponseRef.current = null;
      setCodexRequestSendingId(null);
    }
  };
  const showHandoff = (review: MobileReview, direction: HandoffPresentation["direction"], candidates: HandoffPresentation["candidates"] = [], origin: HandoffPresentation["origin"] = null, selectedAttachmentLabels: string[] = [], destination: HandoffPresentation["destination"] = direction === "CHATGPT_TO_CODEX"
    ? snapshot?.activeCodexEndpoint ? { provider: "CODEX", label: snapshot.activeCodexEndpoint.label, externalId: snapshot.activeCodexEndpoint.externalId } : null
    : snapshot?.activeChatgptEndpoint ? { provider: "CHATGPT", label: snapshot.activeChatgptEndpoint.label, externalId: snapshot.activeChatgptEndpoint.externalId } : null) => {
    const requiresManualDispatch = review.requiresManualDispatch === true;
    setHandoff({ id: review.actionId, handoffId: null, revision: review.revision, status: review.status as HandoffPresentation["status"], direction, message: review.message, candidates, selectedAttachmentLabels: review.attachments ?? selectedAttachmentLabels, attachmentOptions: review.attachmentOptions, canApprove: review.status === "READY", canSend: review.status === "APPROVED" && !requiresManualDispatch, requiresManualDispatch, origin, destination });
    if (direction === "CODEX_TO_CHATGPT" && review.codexOutboundIdentity?.workstreamId) {
      retainCodexHandoffLocation(review.codexOutboundIdentity.workstreamId, review.actionId);
    }
  };

  useEffect(() => {
    const actionId = notificationTarget.handoffId;
    if (!actionId || !index.length || restoredHandoffRef.current === actionId) return;
    restoredHandoffRef.current = actionId;
    let cancelled = false;
    void (async () => {
      try {
        let review: MobileReview;
        let direction: HandoffPresentation["direction"];
        let sourceLabel: string;
        let origin: HandoffPresentation["origin"];
        let destination: NonNullable<HandoffPresentation["destination"]>;
        let sourceId: string;
        let workstreamId: string;
        try {
          review = await mobileApi.codexHandoffReview(actionId);
          const identity = review.codexOutboundIdentity;
          if (!identity || !identity.workstreamId || !identity.sourceId || !identity.sourceEndpointId || !identity.destinationChatgptConversationId) throw new Error("此 Codex 审阅没有可核对的精确来源或接收端。");
          direction = "CODEX_TO_CHATGPT";
          sourceLabel = identity.sourceKind === "REPLY_OBSERVATION" ? "已保存的精确 Codex 外部回复" : "已保存的精确 Codex 执行结果";
          origin = { provider: "CODEX", sourceKind: identity.sourceKind };
          destination = { provider: "CHATGPT", label: "本次审阅保存的精确 ChatGPT 对话", externalId: identity.destinationChatgptConversationId };
          sourceId = identity.sourceId;
          workstreamId = identity.workstreamId;
        } catch (outboundCause) {
          review = await mobileApi.chatGptHandoffReview(actionId).catch(() => { throw outboundCause; });
          const identity = review.chatgptInboundIdentity;
          if (!identity || !identity.workstreamId || !identity.sourceId || !identity.sourceEndpointId || !identity.destinationCodexThreadId) throw new Error("此 ChatGPT 审阅没有可核对的精确来源或接收端。");
          direction = "CHATGPT_TO_CODEX";
          sourceLabel = identity.sourceKind === "REPLY_OBSERVATION" ? "已保存的精确 ChatGPT 外部回复" : "已保存的精确 ChatGPT 执行结果";
          origin = { provider: "CHATGPT", sourceKind: identity.sourceKind };
          destination = { provider: "CODEX", label: "本次审阅保存的精确 Codex 对话", externalId: identity.destinationCodexThreadId };
          sourceId = identity.sourceId;
          workstreamId = identity.workstreamId;
        }
        if (!index.some((item) => item.id === workstreamId)) throw new Error("此手机交接链接指定的工作区当前不可用，未切换到其他工作区。");
        const restoredSnapshot = await load(workstreamId);
        if (cancelled) return;
        const persistedReview = direction === "CODEX_TO_CHATGPT"
          ? restoredSnapshot.mobileCodexOutboundReviews?.find(item => item.actionId === actionId)
          : restoredSnapshot.mobileChatgptInboundReviews?.find(item => item.actionId === actionId);
        if (persistedReview?.handoffId && ["SENDING", "SENT", "FAILED"].includes(review.status)) {
          const delivery = handoffFromExactAttention(restoredSnapshot, workstreamId, persistedReview.handoffId);
          if (!delivery || delivery.direction !== direction || delivery.destination?.externalId !== destination.externalId) {
            throw new Error("此审阅的精确交付记录不可核对，未显示另一条交付。");
          }
          setHandoff({ ...delivery, selectedAttachmentLabels: review.attachments ?? [] });
          setSurface("HANDOFF_STATUS");
          return;
        }
        showHandoff(review, direction, [{ id: sourceId, label: sourceLabel, text: "", sourceLabel: `精确 Router 来源记录 · ${origin?.sourceKind}` }], origin, review.attachments ?? [], destination);
        setSurface("HANDOFF_REVIEW");
      } catch (cause) {
        if (cancelled) return;
        const detail = String(cause);
        const workstreamId = notificationTarget.workstreamId;
        // Older mobile pages could retain only an in-memory review action. If
        // that process restarted, the opaque action no longer exists. Recover
        // only one matching durable delivery record, never a merely recent
        // Handoff or a record from another binding.
        if (workstreamId && detail.includes("Handoff review is no longer available")) {
          try {
            const nextSnapshot = await mobileApi.workstream(workstreamId);
            const recovered = recoverOutboundHandoff(nextSnapshot, workstreamId);
            if (recovered) {
              setSnapshot(nextSnapshot);
              setHandoff(recovered);
              setSurface("HANDOFF_STATUS");
              setError("这个手机审阅链接来自旧版、未持久化的审阅会话，已不能再次操作。Router 已打开当前双端精确绑定下唯一的持久交付记录；没有重发任何内容。");
              return;
            }
          } catch {
            // The original error below is the only safe report when the
            // workstream snapshot itself cannot be read.
          }
        }
        setError(`无法恢复这条精确手机交接：${detail}`);
      }
    })();
    return () => {
      cancelled = true;
      // React StrictMode deliberately replays effects in development.  The
      // first replay must not permanently consume this exact action ID before
      // its asynchronous recovery can commit; otherwise the second effect
      // sees the stale guard and leaves a valid handoff URL at the ordinary
      // workspace.  Clearing only our own guard lets the replacement effect
      // perform the same read, while `cancelled` prevents the abandoned one
      // from changing the current view.
      if (restoredHandoffRef.current === actionId) restoredHandoffRef.current = null;
    };
  }, [index, load, notificationTarget.handoffId]);
  const prepareHandoff = async (replyId: string, initialMessage?: string) => {
    if (!selectedId) return;
    const observation = observations.find((item) => item.id === replyId && item.endpointId === snapshot?.activeChatgptEndpoint?.id) ?? null;
    showHandoff(await mobileApi.prepareHandoff(selectedId, replyId, initialMessage), "CHATGPT_TO_CODEX", observation ? [{ id: observation.id, label: "ChatGPT 完整原回复", text: observation.text, sourceLabel: "已选精确 ChatGPT 外部观察" }] : [], { provider: "CHATGPT", sourceKind: "REPLY_OBSERVATION", observedAt: observation?.observedAt ?? null, manualSelectionCount: initialMessage ? 1 : 0 });
  };
  const prepareCodexHandoff = async (result: MobileReviewResult, attachmentIdsOverride?: string[]) => {
    if (!selectedId || result.provider !== "CODEX") return;
    const attachmentIds = attachmentIdsOverride ?? selectedAttachmentIds[result.runId] ?? defaultAttachmentIds(result);
    const selectedLabels = (result.attachments ?? []).filter((attachment) => attachmentIds.includes(attachment.id)).map((attachment) => attachment.filename);
    showHandoff(await mobileApi.prepareCodexHandoff(selectedId, result.runId, attachmentIds), "CODEX_TO_CHATGPT", [{ id: result.runId, label: "Codex 完整结果", text: result.text, sourceLabel: "已选精确 Codex ProviderRun" }], { provider: "CODEX", sourceKind: "PROVIDER_RUN", observedAt: result.reviewedAt ?? null }, selectedLabels);
  };
  const prepareCodexObservationHandoff = async (observation: MobileReplyObservation) => {
    if (!selectedId || observation.endpointId !== snapshot?.activeCodexEndpoint?.id) return;
    const selectedIds = selectedAttachmentIds[observation.id] ?? [];
    const review = await mobileApi.prepareCodexHandoff(selectedId, observation.id, selectedIds);
    showHandoff(
      review,
      "CODEX_TO_CHATGPT",
      [{ id: observation.id, label: "外部 Codex 最新回复", text: observation.text, sourceLabel: "已选精确 Codex 外部观察" }],
      { provider: "CODEX", sourceKind: "REPLY_OBSERVATION", observedAt: observation.observedAt },
      (observation.attachments ?? []).filter(file => selectedIds.includes(file.id)).map(file => file.filename),
    );
  };
  const toggleAttachment = (runId: string, attachmentId: string) => {
    setSelectedAttachmentIds((current) => {
      const result = reviewResults.find((item) => item.runId === runId);
      const observation = observations.find(item => item.id === runId && item.endpointId === snapshot?.activeCodexEndpoint?.id);
      const attachment = (result?.attachments ?? observation?.attachments)?.find((item) => item.id === attachmentId);
      if (!attachment || !attachmentCanBeSelected(attachment.integrityStatus)) return current;
      const defaults = result ? defaultAttachmentIds(result) : [];
      const selected = current[runId] ?? defaults;
      return { ...current, [runId]: selected.includes(attachmentId) ? selected.filter((id) => id !== attachmentId) : [...selected, attachmentId] };
    });
  };
  const approveHandoff = async (id: string) => {
    if (!handoff || handoff.id !== id || handoff.revision == null) return;
    showHandoff(handoff.direction === "CHATGPT_TO_CODEX"
      ? await mobileApi.approveHandoff(id, handoff.revision, handoff.message)
      : await mobileApi.approveCodexHandoff(id, handoff.revision, handoff.message), handoff.direction, handoff.candidates, handoff.origin, handoff.selectedAttachmentLabels, handoff.destination);
  };
  const sendHandoff = async (id: string) => {
    if (!handoff || handoff.id !== id || handoff.revision == null || handoff.status !== "APPROVED" || !handoff.canSend || handoffSendRef.current) return;
    handoffSendRef.current = id;
    try {
      if (handoff.direction === "CHATGPT_TO_CODEX") {
        await mobileApi.sendHandoff(id, handoff.revision);
        setHandoff({ ...handoff, status: "SENT", canApprove: false, canSend: false, runStatus: "Codex Turn 已接受；执行完成状态独立刷新。" });
      } else {
        const delivery = await mobileApi.sendCodexHandoff(id, handoff.revision);
        setHandoff({ ...handoff, handoffId: delivery.handoffId ?? null, status: delivery.status, canApprove: false, canSend: false, deliveryDetail: delivery.detail ?? null });
        setSurface("HANDOFF_STATUS");
      }
    } finally {
      if (handoffSendRef.current === id) handoffSendRef.current = null;
    }
    // Keep the exact post-send Handoff projection visible. The normal selected
    // workstream load resets review state and must not hide delivery evidence.
  };
  const refreshHandoffStatus = async (id: string) => {
    if (!handoff || handoff.id !== id || handoff.direction !== "CODEX_TO_CHATGPT" || handoff.status !== "SENDING") return;
    // This is an exact, read-only reconciliation of the same persisted
    // Handoff. It cannot retry or create a second external dispatch.
    if (handoff.handoffId && selectedId) {
      const nextSnapshot = await mobileApi.workstream(selectedId);
      setSnapshot(nextSnapshot);
      setHandoff(recoverPendingHandoff(nextSnapshot, selectedId));
      return;
    }
    const review = await mobileApi.codexHandoffReview(id);
    setHandoff((current) => current?.id === id
      ? { ...current, revision: review.revision, status: review.status as HandoffPresentation["status"], message: review.message, canApprove: false, canSend: false }
      : current);
  };
  const resolveManualHandoffDestination = async (id: string) => {
    if (!handoff || handoff.id !== id || handoff.direction !== "CODEX_TO_CHATGPT" || handoff.status !== "APPROVED" || handoff.requiresManualDispatch !== true || handoff.destination?.provider !== "CHATGPT") return;
    try {
      const destination = await mobileApi.codexHandoffManualDestination(id);
      setHandoff((current) => current?.id === id
        && current.direction === "CODEX_TO_CHATGPT"
        && current.status === "APPROVED"
        && current.destination?.provider === "CHATGPT"
        && current.destination.externalId === destination.conversationId
        ? { ...current, unavailableReason: null, destination: { ...current.destination, canonicalUrl: destination.canonicalUrl } }
        : current);
    } catch (cause) {
      setHandoff((current) => current?.id === id
        ? { ...current, unavailableReason: `无法提供精确 ChatGPT 链接：${String(cause)}。Router 没有打开或替换任何对话。` }
        : current);
    }
  };
  const resolveManualDiscussionDestination = async () => {
    if (!selectedId || !snapshot?.activeChatgptEndpoint) {
      throw new Error("当前工作区没有精确绑定的 ChatGPT 对话。");
    }
    const destination = await mobileApi.manualChatGptDiscussionDestination(selectedId);
    if (destination.conversationId !== snapshot.activeChatgptEndpoint.externalId) {
      throw new Error("当前 ChatGPT 绑定在链接核对期间已变化；Router 没有显示或替换任何对话。");
    }
    return destination.canonicalUrl;
  };
  const pairEndpoints = async () => {
    if (!selected || !snapshot) return;
    const codex = codexThreadId.trim() ? { expectedActiveEndpointId: snapshot.activeCodexEndpoint?.id ?? null, externalId: codexThreadId.trim(), label: codexThreadLabel.trim() } : null;
    if (!codex) throw new Error("请选择一个精确 Codex 主对话后再保存配对。");
    if (!codex.label) throw new Error("已选 Codex 对话必须写明标题、日期和来源信息。");
    await mobileApi.pairWorkstreamEndpoints(selected.id, {
      expectedBindingRevision: selected.bindingRevision,
      codex,
    });
    setCodexThreadId("");
    setCodexThreadLabel("");
    await load(selected.id);
  };

  const addProjectLink = async () => {
    if (!selectedId) return;
    const raw = codexProjectId.trim();
    if (!raw) throw new Error("请输入已核验的 Codex 项目 ID。");
    const link = await mobileApi.upsertProjectLink(selectedId,
      { provider: "CODEX", externalProjectId: raw, label: raw, sourceKind: "user-confirmed-existing-project", sourceVersion: null, verifiedAt: null });
    const nextLinks = [...projectLinks.filter((item) => item.provider !== link.provider), link];
    setProjectLinks(nextLinks);
    setCodexProjectId("");
  };

  const prepareOwnerConfirmedChatGptBinding = async (input = explicitChatGptUrl) => {
    const workstreamId = selectedId;
    if (!workstreamId) throw new Error("请先选择一个工作区。");
    const candidate = await mobileApi.prepareOwnerConfirmedChatGptEndpointBinding(workstreamId, input);
    if (selectedWorkstreamRef.current !== workstreamId || candidate.workstreamId !== workstreamId) return;
    setExplicitChatGptCandidate(candidate);
    setExplicitChatGptConfirmation({ state: "IDLE", message: null });
    explicitChatGptConfirmationRef.current = false;
    setExplicitChatGptUrl("");
    setError(null);
  };
  const presentExplicitChatGptConfirmationFailure = (cause: unknown) => {
    const detail = String(cause);
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
    const workstreamId = selectedId;
    const candidate = explicitChatGptCandidate;
    if (!workstreamId || !candidate || candidate.workstreamId !== workstreamId) {
      setExplicitChatGptConfirmation({ state: "FAILED", message: "没有等待确认的 ChatGPT 候选绑定。当前绑定没有改变。" });
      return;
    }
    if (explicitChatGptConfirmationRef.current) return;
    explicitChatGptConfirmationRef.current = true;
    setExplicitChatGptConfirmation({ state: "CONFIRMING", message: null });
    const savesCurrentCanonicalUrl = snapshot?.activeChatgptEndpoint?.id === candidate.expectedOldEndpointId
      && snapshot?.activeChatgptEndpoint?.externalId === candidate.externalId;
    let confirmed: MobileConfirmedChatGptEndpoint;
    try {
      confirmed = await mobileApi.confirmExplicitChatGptEndpointBinding(workstreamId);
    } catch (cause) {
      explicitChatGptConfirmationRef.current = false;
      setExplicitChatGptConfirmation({ state: "FAILED", message: presentExplicitChatGptConfirmationFailure(cause) });
      return;
    }

    // The checked confirmation is authoritative. Presentation reads must not
    // alter a committed transaction's classification.
    if (!isExactConfirmedChatGptEndpoint(confirmed, workstreamId, candidate.externalId)) {
      setExplicitChatGptConfirmation({ state: "SUCCEEDED", message: "ChatGPT 对话已经绑定，但返回的 Endpoint 未得到一致确认。不要重复确认；请重新读取状态。" });
      return;
    }
    setExplicitChatGptConfirmation({ state: "SUCCEEDED", message: savesCurrentCanonicalUrl ? "已保存当前 ChatGPT 精确链接。" : "ChatGPT 对话已绑定。" });
    try {
      // This narrow reproof is intentionally independent of Codex history,
      // Goal, drafts, review results, observations, and Handoff reads.
      const refreshed = await mobileApi.workstream(workstreamId);
      if (!isExactRefreshedChatGptEndpoint(refreshed, workstreamId, confirmed)) {
        setExplicitChatGptConfirmation({ state: "SUCCEEDED", message: "绑定事务已完成，但刷新后的 ACTIVE Endpoint 尚未得到一致确认。不要重复确认；请重新读取状态。" });
        return;
      }
      setSnapshot(refreshed);
      await load(workstreamId, true);
      setExplicitChatGptCandidate(null);
      setSurface("WORKSPACE");
    } catch {
      setExplicitChatGptConfirmation({ state: "SUCCEEDED", message: "ChatGPT 对话已经绑定，但工作区状态刷新失败。不要重复确认；稍后重新读取状态。" });
    }
  };

  const activeCodexEndpoint=snapshot?.activeCodexEndpoint;
  const connectionChoices: ConnectionChoice[] = [
    ...(activeCodexEndpoint ? [{
      id: activeCodexEndpoint.externalId,
      provider: "CODEX" as const,
      title: activeCodexEndpoint.label || "当前已绑定 Codex 主对话",
      detail: "当前工作区已绑定 · 标题、日期和来源已核验",
      selected: codexThreadId === activeCodexEndpoint.externalId,
      onSelect: () => {
        setCodexThreadId(activeCodexEndpoint.externalId);
        setCodexThreadLabel(activeCodexEndpoint.label || "当前已绑定 Codex 主对话 · 当前工作区已绑定");
        setError(null);
      },
    }] : []),
  ];
  const requestBinding = () => {setSurface("WORKSPACE");setBindingRequest(value => value + 1);};
  const connectionPanel = surface === "PROJECT" ? <ProjectConnectionPanel
    guidedBindingEntry onManageBindings={selectedId ? requestBinding : undefined}
    projectName={selected ? snapshot?.projects.find((project) => project.id === selected.projectId)?.name : null} workstreamName={selected?.name} links={projectLinks} choices={connectionChoices} codexProjectId={codexProjectId}
    explicitChatGptUrl={explicitChatGptUrl} explicitChatGptCandidate={explicitChatGptCandidate}
    activeChatGptLabel={snapshot?.activeChatgptEndpoint?.label ?? null}
    activeChatGptId={snapshot?.activeChatgptEndpoint?.externalId ?? null}
    activeCodexLabel={snapshot?.activeCodexEndpoint?.label ?? null}
    activeCodexThreadId={snapshot?.activeCodexEndpoint?.externalId ?? null}
    codexThreadId={codexThreadId} codexThreadLabel={codexThreadLabel} error={error}
    unprojectedDetail={unprojectedThread ? `已取得精确对话 ID；目录：${unprojectedThread.directory}。尚未确认可在重启后重新打开；首次有效 Turn 前不要把它视为持久会话。` : null}
    unprojectedThreadId={unprojectedThread?.thread.id} unprojectedDirectory={unprojectedDirectory}
    onCodexProjectIdChange={setCodexProjectId}
    onSaveCodexProject={() => void addProjectLink().catch((cause) => setError(String(cause)))}
    onExplicitChatGptUrlChange={(value) => { setExplicitChatGptUrl(value); setExplicitChatGptCandidate(null); setExplicitChatGptConfirmation({ state: "IDLE", message: null }); explicitChatGptConfirmationRef.current = false; }}
    onPrepareOwnerConfirmedChatGptBinding={() => void prepareOwnerConfirmedChatGptBinding().catch((cause) => setError(`具体 ChatGPT 对话未准备：${String(cause)}`))}
    onConfirmExplicitChatGptBinding={() => void confirmExplicitChatGptBinding()}
    explicitChatGptConfirmationState={explicitChatGptConfirmation.state} explicitChatGptConfirmationMessage={explicitChatGptConfirmation.message}
    onCodexThreadIdChange={setCodexThreadId} onCodexThreadLabelChange={setCodexThreadLabel}
    onPair={() => void pairEndpoints().then(() => setSurface("WORKSPACE")).catch((cause) => setError(`配对未保存：${String(cause)}`))}
    onUnprojectedDirectoryChange={setUnprojectedDirectory}
    onCreateUnprojected={() => { if (!selectedId) return; void mobileApi.startUnprojectedCodexThread(selectedId, unprojectedDirectory).then((thread) => {
      setUnprojectedThread(thread);
      setUnprojectedDirectory("");
      setCodexThreadId(thread.thread.id);
      setCodexThreadLabel("新建且已验证的 Codex 对话");
    }).catch((cause) => setError(`无项目 Codex 对话未创建：${String(cause)}`)); }}
    onCopyUnprojectedThreadId={() => { if (unprojectedThread) void navigator.clipboard.writeText(unprojectedThread.thread.id); }}
    onBack={() => setSurface("WORKSPACE")}
  /> : null;
  const chatgptResults = reviewResults.filter((result) => result.provider === "CHATGPT");
  const codexResults = reviewResults.filter((result) => result.provider === "CODEX");
  const focusedChatGptResultId = attentionTarget?.workstreamId === selectedId && attentionTarget.kind === "CHATGPT_RESULT_READY"
    ? attentionTarget.sourceId ?? null : null;
  const focusedCodexResultId = attentionTarget?.workstreamId === selectedId && attentionTarget.kind === "CODEX_RESULT_READY"
    ? attentionTarget.sourceId ?? null : null;
  const focusedChatGptResult = focusedChatGptResultId
    ? chatgptResults.find((result) => result.runId === focusedChatGptResultId) ?? null : null;
  const focusedCodexResult = focusedCodexResultId
    ? codexResults.find((result) => result.runId === focusedCodexResultId) ?? null : null;
  const visibleChatGptResults = focusedChatGptResultId ? (focusedChatGptResult ? [focusedChatGptResult] : []) : chatgptResults;
  const visibleCodexResults = focusedCodexResultId ? (focusedCodexResult ? [focusedCodexResult] : []) : codexResults;
  const chatgptResultsPanel = (chatgptResults.length || focusedChatGptResultId) ? <section className="v3-codex-results-focus" aria-label="ChatGPT 历史结果">
    <header className="v3-mobile-codex-heading"><button type="button" onClick={() => setSurface("WORKSPACE")}>‹ 返回</button><h1>{selected?.name ?? "当前工作"}</h1><button type="button" aria-label="连接与详情" onClick={() => setSurface("PROJECT")}>⋯</button></header>
    <header><div><p>{focusedChatGptResultId ? "从收件箱打开的精确 ChatGPT 执行结果" : "Router 保留的执行结果"}</p><h1>ChatGPT 历史结果</h1><span>此页只用于回看历史执行结果，不代表当前对话的新回复，也不会发送或批准。</span>{focusedChatGptResultId ? <small>精确 Router 执行结果 ID：<code>{focusedChatGptResultId}</code></small> : null}</div>{focusedChatGptResultId ? <button type="button" onClick={() => setAttentionTarget(null)}>查看全部 ChatGPT 历史结果</button> : null}</header>
    {focusedChatGptResultId && !focusedChatGptResult ? <aside className="v3-reader-history-recovery" role="alert">这条精确 ChatGPT 历史结果当前不可读；Router 没有切换到其他结果。</aside> : null}
    {visibleChatGptResults.map((result) => <article key={result.runId}><p>{focusedChatGptResultId ? "收件箱指定的精确 Router 执行结果 · 只读回看" : "历史 Router 执行结果 · 只读回看"}</p><h2>{result.reviewedAt ? "已审阅的 ChatGPT 结果" : "ChatGPT 结果等待阅读"}</h2><p>若要交给 Codex，请回到当前 ChatGPT 回复并明确选择范围；这里不会把历史结果伪装成可直接发送的消息。</p><div className="v3-codex-result-body"><MarkdownMessage text={result.text} /></div>{result.markerText?.trim() ? <details><summary>当时识别到的 Codex 指令范围</summary><pre>{result.markerText}</pre></details> : null}</article>)}
  </section> : null;
  const openCodexFeedback = (runId: string) => {
    if (!selectedId || !snapshot?.activeCodexEndpoint) {
      setError("当前工作区没有可确认的 Codex 主对话，未打开修改草稿。");
      return;
    }
    if (codexFeedbackTarget) {
      codexFeedbackAutosave.flush(codexFeedbackDraftKey(codexFeedbackTarget.workstreamId, codexFeedbackTarget.runId));
    }
    const target = { workstreamId: selectedId, runId };
    setCodexFeedbackTarget(target);
    codexFeedbackAutosave.load(codexFeedbackDraftKey(target.workstreamId, target.runId));
    setCodexFeedbackError(null);
    setSurface("CODEX_FEEDBACK");
  };
  const sendCodexFeedback = async () => {
    const target = codexFeedbackTarget;
    const draftKey = target ? codexFeedbackDraftKey(target.workstreamId, target.runId) : null;
    const draft = draftKey ? codexFeedbackAutosave.draft(draftKey).value : "";
    if (!target || !draftKey || !draft.trim() || codexFeedbackSendRef.current) return;
    if (selectedId !== target.workstreamId || !snapshot?.activeCodexEndpoint) {
      setCodexFeedbackError("当前 Codex 主对话已变更；请重新打开结果后再发送，草稿仍会保留。");
      return;
    }
    codexFeedbackSendRef.current = true;
    setCodexFeedbackSending(true);
    setCodexFeedbackError(null);
    try {
      await mobileApi.sendCodexFeedback(target.workstreamId, target.runId, draft);
      codexFeedbackAutosave.change(draftKey, "");
      codexFeedbackAutosave.flush(draftKey);
      await load(target.workstreamId);
      setSurface("CODEX_RESULTS");
    } catch (cause) {
      setCodexFeedbackError(`修改意见没有发送：${String(cause)}`);
    } finally {
      codexFeedbackSendRef.current = false;
      setCodexFeedbackSending(false);
    }
  };
  const codexResultsPanel = (codexResults.length || focusedCodexResultId) ? <section className="v3-codex-results-focus" aria-label="Codex 结果与附件">
    <header className="v3-mobile-codex-heading"><button type="button" onClick={() => setSurface("WORKSPACE")}>‹ 返回</button><h1>{selected?.name ?? "当前工作"}</h1><button type="button" aria-label="连接与详情" onClick={() => setSurface("PROJECT")}>⋯</button></header>
    <header><div><h1>{selected?.name ?? "当前工作"} · Codex</h1><p>{focusedCodexResultId ? "从收件箱打开的精确 Codex 执行结果" : "AI Work Router 项目　/　精确主对话"}</p>{focusedCodexResultId ? <small>精确 Router 执行结果 ID：<code>{focusedCodexResultId}</code></small> : null}</div>{focusedCodexResultId ? <button type="button" onClick={() => setAttentionTarget(null)}>查看全部 Codex 历史结果</button> : null}</header>
    <nav className="v3-codex-result-tabs" aria-label="结果来源"><button type="button" onClick={() => setSurface("WORKSPACE")}>ChatGPT</button><span aria-current="page">Codex</span></nav>
    {focusedCodexResultId && !focusedCodexResult ? <aside className="v3-reader-history-recovery" role="alert">这条精确 Codex 历史结果当前不可读；Router 没有切换到其他结果。</aside> : null}
    <div className="v3-codex-result-layout"><section className="v3-codex-result-list">{visibleCodexResults.map((result) => { const attachments = result.attachments ?? []; const selectedIds = selectedAttachmentIds[result.runId] ?? defaultAttachmentIds(result); const relayableCount = attachments.filter((attachment) => attachmentCanBeSelected(attachment.integrityStatus)).length; return <div key={result.runId} className="v3-codex-result-block"><article className="v3-codex-result-card">
      <span className="v3-codex-result-badge">{focusedCodexResultId ? "收件箱指定的精确 Router 执行结果 · 可单独审阅" : "历史 Router 执行结果 · 可单独审阅"}</span><h2>{selected?.name ? `${selected.name} · 历史 Codex 执行结果` : "历史 Codex 执行结果"}</h2><p className="v3-detail">这不是当前 Codex 对话的新回复；它是 Router 过去一次执行保留的结果。</p><div className="v3-codex-result-body"><MarkdownMessage text={result.text} /></div>
    </article><section className="v3-codex-attachment-summary" aria-label="结论附件"><h3>{attachments.length} 个附件　·　{relayableCount} 项可安全转发</h3><div>{attachments.map((attachment) => { const relayable = attachmentCanBeSelected(attachment.integrityStatus); return <button key={attachment.id} type="button" disabled={!relayable} aria-label={`${relayable ? "查看并选择附件" : "附件不可转发"}：${attachment.filename}${selectedIds.includes(attachment.id) ? "，已选" : ""}`} className={relayable ? "" : "is-blocked"} onClick={() => { if (!relayable) return; setCodexAttachmentRunId(result.runId); setSurface("CODEX_ATTACHMENTS"); }}><strong>{selectedIds.includes(attachment.id) ? "✓" : "□"}</strong><span><b>{attachment.filename}</b><small>{attachmentRelayExplanation(attachment.integrityStatus, attachment.warnings ?? [])}</small></span></button>; })}</div></section><footer className="v3-codex-result-actions">{snapshot?.activeCodexEndpoint ? <button type="button" className="v3-codex-result-modify" onClick={() => openCodexFeedback(result.runId)}>要求 Codex 修改</button> : <span className="v3-codex-result-modify v3-codex-result-unavailable" role="status">当前没有可确认的 Codex 主对话</span>}<button type="button" className="v3-primary" onClick={() => void prepareCodexHandoff(result).catch((cause) => setError(`无法准备 Codex Handoff：${String(cause)}`))}>审阅此历史结果，准备转发给 ChatGPT</button></footer></div>; })}</section>
      <aside className="v3-codex-goal-overview" aria-label="当前目标概览"><p>当前目标</p><h2>{goal?.text ?? "当前没有可读取 Goal"}</h2><span className={`v3-codex-goal-status v3-codex-goal-status-${goal?.status?.toLowerCase() ?? "none"}`}>{goal?.status === "PAUSED" ? "已暂停 · 最近读取" : goal?.status === "ACTIVE" ? "目标进行中 · 最近读取" : goal ? `${goal.status} · 最近读取` : "状态未确认 · 最近读取"}</span><small>{goal?.usageLabel ? `Codex 报告用量：${goal.usageLabel}` : "用量或耗时尚未报告"}<br />目标状态不等于本轮执行状态。</small><button type="button" onClick={() => setSurface("GOAL")}>查看目标与控制</button></aside>
    </div>
    <footer className="v3-mobile-codex-dock"><button type="button" className="v3-primary" onClick={() => { const result = visibleCodexResults[0]; if (result) void prepareCodexHandoff(result).catch((cause) => setError(`无法准备 Codex Handoff：${String(cause)}`)); }} disabled={!visibleCodexResults[0]}>审阅历史结果，准备转发</button>{snapshot?.activeCodexEndpoint ? <button type="button" onClick={() => { const result = visibleCodexResults[0]; if (result) openCodexFeedback(result.runId); }} disabled={!visibleCodexResults[0]}>修改</button> : <span aria-label="修改当前不可用">修改当前不可用</span>}<button type="button" onClick={() => setSurface("GOAL")}>查看当前目标</button></footer>
    <button type="button" className="v3-codex-connection-link" onClick={() => setSurface("PROJECT")}>连接与对话 ID</button>
  </section> : null;
  const codexAttachmentResult = codexResults.find((result) => result.runId === codexAttachmentRunId) ?? null;
  const codexAttachmentsPanel = codexAttachmentResult ? (() => {
    const attachments = codexAttachmentResult.attachments ?? [];
    const selectedIds = selectedAttachmentIds[codexAttachmentResult.runId] ?? defaultAttachmentIds(codexAttachmentResult);
    const blockedCount = attachments.filter((attachment) => !attachmentCanBeSelected(attachment.integrityStatus)).length;
    const prepare = (attachmentIds: string[]) => void prepareCodexHandoff(codexAttachmentResult, attachmentIds).then(() => setSurface("HANDOFF_REVIEW")).catch((cause) => setError(`无法准备 Codex Handoff：${String(cause)}`));
    return <section className="v3-mobile-attachment-focus" aria-label="回传附件选择">
      <header className="v3-mobile-codex-heading"><button type="button" onClick={() => setSurface("CODEX_RESULTS")}>‹ 返回</button><h1>{selected?.name ?? "当前工作"}</h1><span aria-hidden="true" /></header>
      <main className="v3-mobile-attachment-content"><span>回传附件</span><h2>选好这次要回传的材料</h2><p>已选 {selectedIds.length} 项{blockedCount ? `；另有 ${blockedCount} 项校验失败，未选入。` : "。"}</p><div className="v3-mobile-attachment-list">{attachments.length ? attachments.map((attachment) => { const selectable = attachmentCanBeSelected(attachment.integrityStatus); const checked = selectedIds.includes(attachment.id); const status = attachment.integrityStatus === "MISMATCH" ? "校验不一致 · 不可发送" : attachment.integrityStatus === "ERROR" ? "校验失败 · 不可发送" : checked ? "✓ 已选 · 查看内容" : "未选入"; return <button key={attachment.id} type="button" aria-pressed={checked} disabled={!selectable} className={!selectable ? "is-blocked" : ""} onClick={() => toggleAttachment(codexAttachmentResult.runId, attachment.id)}><strong>{checked ? "✓" : "□"}</strong><span><b>{attachment.filename}</b><small>{status}{attachment.warnings?.length ? ` · ${attachment.warnings.join("；")}` : ""}</small></span></button>; }) : <p className="v3-mobile-attachment-empty">此完整结果没有可选择的本地附件。</p>}</div><p className="v3-mobile-attachment-note">发送前会再次校验。未选文件不会被静默补入。</p><button type="button" className="v3-mobile-attachment-conclusion" onClick={() => prepare([])}>不带附件，只回传结论</button></main>
      <footer className="v3-mobile-attachment-dock"><button type="button" className="v3-primary" onClick={() => prepare(selectedIds)}>继续 · 结论{selectedIds.length ? ` + ${selectedIds.length} 个附件` : ""}</button></footer>
    </section>;
  })() : null;
  const codexFeedbackDraftKeyForTarget = codexFeedbackTarget ? codexFeedbackDraftKey(codexFeedbackTarget.workstreamId, codexFeedbackTarget.runId) : null;
  const codexFeedbackDraft = codexFeedbackAutosave.draft(codexFeedbackDraftKeyForTarget);
  const changeCodexFeedbackDraft = (value: string) => {
    if (!codexFeedbackDraftKeyForTarget) return;
    codexFeedbackAutosave.change(codexFeedbackDraftKeyForTarget, value);
  };
  const closeCodexFeedback = () => {
    if (codexFeedbackDraftKeyForTarget) codexFeedbackAutosave.flush(codexFeedbackDraftKeyForTarget);
    setSurface("CODEX_RESULTS");
  };
  const codexFeedbackPanel = surface === "CODEX_FEEDBACK" ? <section className="v3-mobile-codex-feedback" aria-label="向 Codex 提出修改"><header className="v3-mobile-codex-heading"><button type="button" onClick={closeCodexFeedback}>‹ 返回</button><h1>{selected?.name ?? "当前工作"}</h1><span aria-hidden="true" /></header><main><span>回复给 Codex</span><h2>这次先不回传</h2><p>修改意见只发给当前 Codex 主对话。</p><textarea aria-label="修改意见" value={codexFeedbackDraft.value} disabled={codexFeedbackSending} onChange={(event) => changeCodexFeedbackDraft(event.target.value)} placeholder="说明需要补充或调整的内容…" />{codexFeedbackDraft.saveState === "FAILED" && <p className="v3-codex-feedback-error" role="alert">修改草稿没有保存；保留在当前页面，可稍后重试。</p>}{codexFeedbackError && <p className="v3-codex-feedback-error" role="alert">{codexFeedbackError}</p>}<button type="button" className="v3-mobile-codex-feedback-result-link" onClick={closeCodexFeedback}>查看完整结果与附件</button></main><footer><button type="button" className="v3-primary" disabled={codexFeedbackSending || !codexFeedbackDraft.value.trim()} onClick={() => void sendCodexFeedback()}>{codexFeedbackSending ? "正在发送修改意见…" : "发送修改意见"}</button><button type="button" onClick={closeCodexFeedback}>取消，保留草稿</button></footer></section> : null;
  const exactCodexRequestId = attentionTarget?.workstreamId === selectedId && attentionTarget.kind === "CODEX_STRUCTURED_REQUEST"
    ? attentionTarget.sourceId ?? null
    : null;
  const selectedCodexRequest = exactCodexRequestId
    ? codexRequests.find((request) => request.requestId === exactCodexRequestId) ?? null
    : codexRequests.find((request) => request.method.includes("commandExecution") && request.choices?.some((choice) => choice.id === "accept") && request.choices?.some((choice) => choice.id === "decline")) ?? codexRequests[0] ?? null;
  const selectedRequestDecision = selectedCodexRequest ? codexRequestDecisions[selectedCodexRequest.requestId] : undefined;
  const exactProviderRunAttention = attentionTarget?.workstreamId === selectedId
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
  const providerRunStatusPanel = exactProviderRunStatus ? <section className="v3-mobile-codex-request" aria-label="精确 Provider 执行详情"><header className="v3-mobile-codex-heading"><button type="button" onClick={() => setSurface("INBOX")}>‹ 收件箱</button><h1>{selected?.name ?? "当前工作"}</h1><span aria-hidden="true" /></header><main><span>收件箱指定的精确执行 · 只读</span><h2>{exactProviderRunStatus.provider === "CHATGPT" ? "ChatGPT" : "Codex"} 执行：{exactProviderRunStatus.status}</h2><p>{providerRunNextAction}</p><details open><summary>精确 Router 执行 ID</summary><code>{exactProviderRunStatus.runId}</code></details><section className="v3-mobile-codex-request-detail" aria-label="Router 记录的执行时间"><small>执行时间</small><p>开始：{providerRunTimestamp(exactProviderRunStatus.startedAt)}<br />终态确认：{providerRunTimestamp(exactProviderRunStatus.terminalAt)}<br />最后状态更新：{providerRunTimestamp(exactProviderRunStatus.updatedAt)}</p></section><p>可审阅结果：{exactProviderRunStatus.hasReviewableResult ? "有；不会在此页替代展示内容。" : "没有。"}</p>{exactProviderRunStatus.terminalCode ? <details><summary>稳定终态代码</summary><code>{exactProviderRunStatus.terminalCode}</code></details> : <p>Router 尚未记录稳定终态代码。</p>}{exactProviderRunStatus.originHandoffId ? <details><summary>关联交接记录</summary><code>{exactProviderRunStatus.originHandoffId}</code></details> : null}</main><footer><button type="button" className="v3-primary" onClick={() => setSurface("INBOX")}>返回收件箱</button></footer></section> : <section className="v3-mobile-codex-request" aria-label="精确 Provider 执行详情"><header className="v3-mobile-codex-heading"><button type="button" onClick={() => setSurface("INBOX")}>‹ 收件箱</button><h1>执行状态不可读</h1><span aria-hidden="true" /></header><main><p role="alert">收件箱指定的精确 Provider 执行状态当前不可读或已不存在；Router 没有显示其他执行。</p>{exactProviderRunAttention?.sourceId ? <details open><summary>收件箱指定的精确执行 ID</summary><code>{exactProviderRunAttention.sourceId}</code></details> : null}</main></section>;
  const codexRequestPanel = selectedCodexRequest ? <section className={`v3-mobile-codex-request${selectedCodexRequest.responseSent ? " is-responded" : ""}${selectedRequestDecision === "decline" ? " is-declined" : ""}`} aria-label="Codex 官方结构化请求">
    <header className="v3-mobile-codex-heading"><button type="button" onClick={() => setSurface("WORKSPACE")}>‹ 返回</button><h1>{selected?.name ?? "当前工作"}</h1><span aria-hidden="true" /></header>
    {!selectedCodexRequest.responseSent ? <main><span>Codex 需要一次授权</span>{exactCodexRequestId && <details open><summary>收件箱指定的精确请求 ID</summary><code>{selectedCodexRequest.requestId}</code></details>}<h2>允许这一次命令？</h2><section className="v3-mobile-codex-request-detail" aria-label="官方请求内容，只读"><small>请求说明</small><strong>{selectedCodexRequest.reason?.trim() || "官方请求未提供可显示说明"}</strong><pre>Router 当前未收到可显示的原始命令。</pre><p>工作目录：Router 未提供可显示目录<br />范围：仅这一次</p></section><p className="v3-mobile-codex-request-note">这是 Codex 的授权请求，不是跨系统交接。请求已失效时不会执行旧选择。</p><button type="button" className="v3-mobile-codex-request-link" onClick={() => setSurface("CODEX_RESULTS")}>查看完整结果与附件</button></main> : <main><span>已回应这次请求</span><h2>{selectedRequestDecision === "decline" ? "已拒绝这一次" : "已允许这一次"}</h2><p className="v3-mobile-codex-request-outcome">{selectedRequestDecision === "decline" ? "这项授权不会执行，也不能重复回应。Codex 后续结果会单独显示；拒绝请求不等于完成整个目标。" : "这项选择已提交，不能重复使用。Codex 后续执行状态会单独显示；允许命令不会自动继续一个暂停的 Goal。"}</p>{selectedRequestDecision !== "decline" && <section className="v3-mobile-codex-request-history"><pre>{selectedCodexRequest.reason?.trim() || "这一次官方请求"}</pre><p>已回应 · 仅一次</p></section>}</main>}
    <footer>{!selectedCodexRequest.responseSent ? <><button type="button" className="v3-primary" disabled={codexRequestSendingId === selectedCodexRequest.requestId} onClick={() => void respondToCodexRequest(selectedCodexRequest, "accept").catch((cause) => setError(`授权回应没有送达：${String(cause)}`))}>{codexRequestSendingId === selectedCodexRequest.requestId ? "正在回应…" : "允许这一次"}</button><button type="button" disabled={codexRequestSendingId === selectedCodexRequest.requestId} onClick={() => void respondToCodexRequest(selectedCodexRequest, "decline").catch((cause) => setError(`授权回应没有送达：${String(cause)}`))}>拒绝</button></> : <button type="button" className="v3-primary" onClick={() => setSurface("WORKSPACE")}>{selectedRequestDecision === "decline" ? "返回收件箱" : "返回工作区"}</button>}</footer>
  </section> : surface === "CODEX_REQUEST" && exactCodexRequestId ? <section className="v3-mobile-codex-request" aria-label="Codex 官方结构化请求"><header className="v3-mobile-codex-heading"><button type="button" onClick={() => setSurface("INBOX")}>‹ 收件箱</button><h1>请求当前不可读</h1><span aria-hidden="true" /></header><main><p role="alert">收件箱指定的精确 Codex 请求当前不可读或已不存在；Router 没有显示其他请求。</p><details open><summary>收件箱指定的精确请求 ID</summary><code>{exactCodexRequestId}</code></details></main></section> : null;

  const checkNewProviderReply = async (provider: "CHATGPT" | "CODEX") => {
    if (!selectedId) return;
    if (provider === "CHATGPT" && !CHATGPT_NORMAL_BROWSER_OBSERVER_ENABLED) return;
    setError(null);
    try {
      const result = provider === "CHATGPT"
        ? await mobileApi.checkNewChatGptReplies(selectedId)
        : await mobileApi.checkNewCodexReplies(selectedId);
      // Both provider readers materialize the exact result into the same
      // durable observation feed. Refresh it for either direction so a
      // successful Codex check is immediately visible and reviewable.
      setObservations(await mobileApi.replyObservations(selectedId));
      if (provider === "CODEX") {
        const history = await mobileApi.codexHistory(selectedId).catch(() => ({ history: [] }));
        setCodexHistory(history.history);
      }
      const detail = result.observationCreated
        ? "已记录 1 条新的完成回复。"
        : result.state === "OBSERVER_BASELINE_ESTABLISHED"
          ? "已建立观察起点；此前历史不会通知，后续新完成回复会自动提醒。"
        : result.state === "LATEST_TURN_INTERRUPTED"
          ? "最新任务已中断，没有新的完成回复。"
          : result.state === "LATEST_TURN_ACTIVE"
            ? "最新任务仍在运行，尚无新的完成回复。"
            : result.state === "OBSERVATION_MATERIALIZATION_PENDING"
              ? "最新完成回复正在落盘，可稍后再次检查。"
              : "没有新的完成回复。";
      setManualCheckNotice(`${provider === "CHATGPT" ? "ChatGPT" : "Codex"} 检查完成：${detail}`);
    } catch (cause) {
      const accountSecurityRequired = provider === "CHATGPT" && (String(cause).includes("CHATGPT_ACCOUNT_SECURITY_REQUIRED") || String(cause).includes("AUTH_REQUIRED"));
      if (accountSecurityRequired) setBrowserAuthenticationRequired(true);
      const normalBrowserUnavailable = provider === "CHATGPT" && String(cause).includes("CHATGPT_NORMAL_BROWSER");
      const normalBrowserMessage = normalChromeConnectionMessage(cause);
      setManualCheckNotice(accountSecurityRequired
        ? "检测到账户安全验证；Router 已停止读取、刷新和重新打开页面。"
        : normalBrowserUnavailable
          ? normalBrowserMessage
        : `${provider === "CHATGPT" ? "ChatGPT" : "Codex"} 检查未完成；没有发送消息或创建虚构回复。`);
      setError(normalBrowserUnavailable
        ? `检查 ChatGPT 新回复未执行：${normalBrowserMessage}`
        : `检查 ${provider === "CHATGPT" ? "ChatGPT" : "Codex"} 新回复失败：${String(cause)}`);
    }
  };

  const codexHistoryPanel = snapshot?.activeCodexEndpoint ? <section className="v3-codex-results-focus" aria-label="Codex 对话">
    <header className="v3-mobile-codex-heading"><button type="button" onClick={() => setSurface("WORKSPACE")}>‹ 返回</button><h1>{selected?.name ?? "当前工作"}</h1><button type="button" aria-label="连接与详情" onClick={() => setSurface("PROJECT")}>⋯</button></header>
    <section className="v3-codex-result-block"><article className="v3-codex-result-card"><p>当前已绑定 Codex 对话</p><h2>{snapshot.activeCodexEndpoint.label || "Codex 对话"}</h2><details><summary>精确绑定标识</summary><code>{snapshot.activeCodexEndpoint.externalId}</code></details><section aria-label="Codex 观察状态"><p>只读检查当前精确 Codex 线程</p><button type="button" onClick={() => void checkNewProviderReply("CODEX")}>检查 Codex 新回复</button><small>{manualCheckNotice ?? "检查不会恢复、发送或启动任务。"}</small></section>{selectedCodexObservationId && !exactCodexObservation ? <aside className="v3-reader-history-recovery" role="alert">收件箱指定的精确 Codex 回复当前不可读；Router 没有显示同一线程的较新回复。</aside> : null}{latestCodexObservation ? <section className="v3-codex-result-body" aria-label={exactCodexObservation ? "收件箱选定的 Codex 回复" : "最新外部 Codex 回复"}><p>{exactCodexObservation ? "从收件箱打开的精确 Codex 回复" : "当前可转交的外部 Codex 回复"}</p><small>下一步：审阅并编辑 → 批准此版本（不会发送） → 单独发送到精确绑定的 ChatGPT 对话。</small><MarkdownMessage text={latestCodexObservation.text} /><ObservationAttachmentSelection files={latestCodexObservation.attachments ?? []} selectedIds={selectedAttachmentIds[latestCodexObservation.id] ?? []} onToggle={id => toggleAttachment(latestCodexObservation.id, id)} /><button type="button" onClick={() => void markReply(latestCodexObservation.id, false).catch((cause) => setError(String(cause)))}>标为已读</button><button type="button" onClick={() => void markReply(latestCodexObservation.id, true).catch((cause) => setError(String(cause)))}>标记已处理</button><button type="button" className="v3-primary" onClick={() => void prepareCodexObservationHandoff(latestCodexObservation).then(() => setSurface("HANDOFF_REVIEW")).catch((cause) => setError(`无法准备外部 Codex 回复回传：${String(cause)}`))}>审阅此回复，准备转发给 ChatGPT</button></section> : <p>{selectedCodexObservationId ? "这条收件箱指定回复当前不可读；没有显示同一线程的其他回复。" : "当前没有新的可转交 Codex 回复。可以使用“检查 Codex 新回复”读取当前精确线程。"}</p>}<section className="v3-codex-result-body" aria-label="Codex 线程历史"><p>只读线程历史（用于核对；不等同于可转交的外部观察）</p>{codexHistory.filter((event) => event.text?.trim()).map((event) => <article key={event.id}><p>{event.kind === "AgentMessage" ? "线程中的 Codex 回复" : "Codex 线程事件"}</p><MarkdownMessage text={event.text ?? ""} /></article>)}</section></article></section>
  </section> : null;

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
    ? "正在核对这台手机与 Router 的通知订阅…"
    : phonePushReady
      ? "已开启：Router 会为未来新出现的完整 ChatGPT / Codex 回复提交通知。推送服务没有手机弹窗回执；可用下方“测试真实回复通知”核对同一展示路径。"
      : pushSetup === "DENIED"
        ? "此浏览器已拒绝通知权限。请在手机的站点设置中允许通知后再试。"
        : pushSetup === "UNSUPPORTED"
          ? "当前打开方式不支持网站通知。请在受支持的手机浏览器或已添加到主屏幕的 Web App 中打开。"
          : pushSetup === "UPDATING"
            ? "现有订阅已保留，但真实回复通知组件仍在更新。Router 不会把基础测试通知当成业务通知已就绪；请保持此 PWA 打开片刻后刷新状态。"
          : pushSetup === "SUBSCRIBED"
            ? "浏览器显示已订阅，但 Router 尚未收到有效订阅；不会把它当成已开启。请重新开启一次。"
            : "尚未开启。现在开启后，Router 只推送未来的新回复。";
  const phonePushPanel = <section className={`v3-mobile-push-card${phonePushReady ? " is-ready" : ""}`} aria-label="手机通知">
    <div><span>手机通知</span><h2>{phonePushReady ? "未来回复会通知你" : "开启未来回复通知"}</h2><p>{phonePushStatus}</p></div>
    <div className="v3-mobile-push-actions">
      {!phonePushReady && <button type="button" className="v3-primary" disabled={pushBusy || pushSetup === "CHECKING" || pushSetup === "UNSUPPORTED" || pushSetup === "DENIED" || pushSetup === "UPDATING"} onClick={() => void enablePhoneNotifications()}>{pushSetup === "UPDATING" ? "正在更新通知组件…" : pushBusy ? "正在开启…" : "开启本机通知"}</button>}
      {phonePushReady && <><button type="button" disabled={pushBusy} onClick={() => void mobileApi.testPush().catch((cause) => setError(`测试通知没有送达：${String(cause)}`))}>发送测试通知</button><button type="button" disabled={pushBusy} onClick={() => void mobileApi.testReplyPush().catch((cause) => setError(`真实回复通知测试没有送达：${String(cause)}`))}>测试真实回复通知</button><button type="button" disabled={pushBusy} onClick={() => void disablePhoneNotifications()}>{pushBusy ? "正在关闭…" : "关闭本机通知"}</button></>}
      <button type="button" disabled={pushBusy} onClick={() => void refreshPushSetup().catch((cause) => setError(`手机通知状态未刷新：${String(cause)}`))}>刷新状态</button>
      {phonePushReady && <button type="button" onClick={() => setPushPanelExpanded(false)}>收起通知设置</button>}
    </div>
  </section>;
  const mobileWorkspaceMenu = selectedId && (snapshot?.activeChatgptEndpoint || snapshot?.activeCodexEndpoint) ? <>
    <button type="button" onClick={() => setPushPanelExpanded(true)}>{phonePushReady ? "管理手机通知（已开启）" : "开启手机通知"}</button>
    {snapshot.activeChatgptEndpoint ? <button type="button" onClick={() => void checkNewProviderReply("CHATGPT")}>检查 ChatGPT 新回复</button> : null}
    {snapshot.activeCodexEndpoint ? <button type="button" onClick={() => void checkNewProviderReply("CODEX")}>检查 Codex 新回复</button> : null}
    {snapshot.activeChatgptEndpoint ? <button type="button" onClick={() => setObserverDetailsOpen((current) => !current)}>{observerDetailsOpen ? "收起回复观察说明" : "查看回复观察说明"}</button> : null}
    {observerDetailsOpen && <p className="v3-mobile-workspace-menu-note" role="status">Router 在电脑端读取精确绑定的 ChatGPT 对话。首次读取建立观察起点；之后的新终态回复创建通知。被动观察不可用时可手动检查。交接先审阅、批准，再单独发送。</p>}
  </> : null;

  // Notification setup is explicit. A transient capability check must never
  // consume the first phone viewport or push a focused exact reply below the
  // fixed Reader dock; the workspace menu remains the single clear entry.
  const showPhonePushPanel = pushPanelExpanded;
  return <>{error && <p role="alert" className="v3-mobile-error">{error}</p>}<UnifiedWorkbench notificationDetail={notificationDetail} notificationCount={notificationCount} notificationSettings={<><div data-expanded={showPhonePushPanel}>{phonePushPanel}</div>{webNotificationApi.delivery&&<NotificationDeliveryPanel api={webNotificationApi.delivery}/>}<NotificationAssistantSettings api={webNotificationApi}/></>} devicePanel={<WebSessionLogout/>} onRenameBridge={async(id,name)=>{await mobileApi.renameBridge(id,name);await refreshIndex();}} onManageBindings={requestBinding} onCreateBridge={async name=>{const id=await mobileApi.createBridge(name);await refreshIndex();await load(id);flushSync(()=>setSurface("WORKSPACE"));setBindingRequest(v=>v+1);}}
    items={items} selectedWorkstreamId={selectedId} reply={reply}
    roleCompatible={roleMode}
    globalActions={<CodexNotifications onDetailChange={setNotificationDetail} onCountChange={setNotificationCount} api={webNotificationApi} workbenchPage={{active:surface==="NOTIFICATIONS",open:()=>setSurface("NOTIFICATIONS")}}/>}
    criticalNotice={(browserAuthenticationRequired) ? <BridgeStatus recoveryOnly name={selected?.name || "AI Work Router"} recoveryError={error} decision={snapshot?.activeChatgptEndpoint} execution={snapshot?.activeCodexEndpoint} authenticationRequired={browserAuthenticationRequired} nextAction={items.find(item => item.id === selectedId)?.attentionItems?.[0]?.message} onManage={() => setSurface("PROJECT")} onOpenBrowser={() => mobileApi.openHostChatGptBrowserSetup().then(() => { setError(null); return true; }).catch(cause => { setError(securityRecoveryError(cause, "OPEN")); return false; })} onAuthenticationCompleted={() => mobileApi.confirmChatGptAuthenticationCompleted().then(() => setBrowserAuthenticationRequired(false)).catch(cause => setError(securityRecoveryError(cause, "COMPLETE")))} /> : null}
    bridgePanel={<>{!roleMode && (selected && !(browserAuthenticationRequired) ? <BridgeStatus name={selected?.name || "AI Work Router"} recoveryError={error} decision={snapshot?.activeChatgptEndpoint} execution={snapshot?.activeCodexEndpoint} authenticationRequired={browserAuthenticationRequired} nextAction={items.find(item => item.id === selectedId)?.attentionItems?.[0]?.message} onManage={requestBinding} onOpenBrowser={() => mobileApi.openHostChatGptBrowserSetup().then(() => { setError(null); return true; }).catch(cause => { setError(securityRecoveryError(cause, "OPEN")); return false; })} onAuthenticationCompleted={() => mobileApi.confirmChatGptAuthenticationCompleted().then(() => setBrowserAuthenticationRequired(false)).catch(cause => setError(securityRecoveryError(cause, "COMPLETE")))} /> : null)} {selectedId && roleApi && <RoleBridgePanel key={selectedId} workstreamId={selectedId} workstreamName={items.find(item => item.id === selectedId)?.name} api={roleApi} bindingRequest={bindingRequest} externalBindingEntry={Boolean(selected && !browserAuthenticationRequired)} onModeChange={setRoleMode} />}</>}
    draft={workbenchDraft}
    handoff={handoff}
    goal={goal}
    lifecycle={selected ? { canArchive: lifecycleOf(selected) === "ACTIVE", canTrash: lifecycleOf(selected) !== "TRASHED", canRestore: lifecycleOf(selected) !== "ACTIVE", canPurge: lifecycleOf(selected) === "TRASHED", trashCount: items.filter((item) => item.lifecycle === "TRASHED").length, backupNotice } : null}
    runtime={{ notice: runtimeAttentionNotice, checks: [{ id: "router-core", label: "Router Core", state: snapshot ? "READY" : "CHECKING", detail: snapshot ? "已通过受认证的手机语义 API 读取当前工作区" : "正在读取受认证的手机语义 API" }, { id: "providers", label: "Provider 连接", state: "WARNING", detail: "按选中精确工作区读取；手机端不会直接连接 Provider" }] }}
    surface={surface} onSurfaceChange={(next) => setSurface(next === "PROJECT_HOME" ? "PROJECT" : next)} projectPanel={connectionPanel} hasCodexEndpoint={Boolean(snapshot?.activeCodexEndpoint)} codexHistoryPanel={codexHistoryPanel} chatgptResultsPanel={chatgptResultsPanel} codexResultsPanel={codexResultsPanel} codexAttachmentsPanel={codexAttachmentsPanel} codexFeedbackPanel={codexFeedbackPanel} codexRequestPanel={codexRequestPanel} providerRunStatusPanel={providerRunStatusPanel} codexResultCount={codexResults.length} codexRequestCount={codexRequests.filter((request) => !request.responseSent).length} mobileWorkspaceMenu={<><WebSessionLogout/>{mobileWorkspaceMenu}</>} workspaceNotice={manualCheckNotice}
    handoffDestinationActionLabel="核对并显示精确 ChatGPT 链接" onOpenHandoffDestination={(id) => { void resolveManualHandoffDestination(id); }}
    onSelectWorkstream={(id) => { setAttentionTarget(null); setSurface("WORKSPACE"); void load(id).catch((cause) => setError(String(cause))); }}
    onSelectAttention={(workstreamId, attention) => {
      setAttentionTarget({ workstreamId, sourceId: attention.sourceId, kind: attention.kind });
      setProviderRunStatus(null);
      void load(workstreamId).then((nextSnapshot) => {
        if (attention.kind === "CHATGPT_RESULT_READY") setSurface("CHATGPT_RESULTS");
        else if (attention.kind === "CODEX_RESULT_READY") setSurface("CODEX_RESULTS");
        else if (attention.kind === "CODEX_REPLY_OBSERVED") setSurface("CODEX_HISTORY");
        else if (attention.kind === "CODEX_STRUCTURED_REQUEST") setSurface("CODEX_REQUEST");
        else if (attention.kind === "HANDOFF_FAILED" || attention.kind === "DELIVERY_UNCERTAIN") {
          const exact = attention.sourceId && nextSnapshot ? handoffFromExactAttention(nextSnapshot, workstreamId, attention.sourceId) : null;
          if (!exact) {
            setError("收件箱指定的精确交付记录当前不可读或不存在；Router 没有显示其他交付记录。");
            setSurface("INBOX");
            return;
          }
          setHandoff(exact);
          setSurface("HANDOFF_STATUS");
        }
        else if (attention.kind === "MISSING_CHATGPT_BINDING" || attention.kind === "MISSING_CODEX_BINDING") setSurface("PROJECT");
        else if (attention.kind === "PROVIDER_RUN_FAILED" || attention.kind === "PROVIDER_RUN_CANCELLED" || attention.kind === "UNKNOWN_RUN") {
          if (!attention.sourceId) {
            setError("收件箱指定的 Provider 执行没有精确 ID；Router 不会猜测或显示其他执行。");
            setSurface("PROVIDER_RUN_STATUS");
            return;
          }
          void mobileApi.providerRunStatus(workstreamId, attention.sourceId).then((status) => {
            setProviderRunStatus(status);
            setSurface("PROVIDER_RUN_STATUS");
          }).catch(() => {
            setProviderRunStatus(null);
            setSurface("PROVIDER_RUN_STATUS");
          });
        }
        else setSurface("INBOX");
      }).catch((cause) => setError(String(cause)));
    }} onDraftChange={changeDraft}
    onSelectRecycleWorkstream={(id) => { void load(id).catch((cause) => setError(String(cause))); }}
    onPinChange={(id, pinned) => void mobileApi.setWorkstreamPinned(id, pinned).then(refreshIndex).catch((cause) => setError(`置顶未保存：${String(cause)}`))}
    onSelectHandoffAttachments={async (id, filenames) => {
      if (!handoff || handoff.id !== id || handoff.revision == null) return;
      try {
        const review = await mobileApi.selectHandoffAttachments(id, handoff.revision, filenames);
        if (review.actionId !== id || review.status !== "READY") throw new Error("附件结果未匹配当前审阅");
        setHandoff(current => current?.id === id && current.status === "READY"
          ? { ...current, revision: review.revision, selectedAttachmentLabels: review.attachments, attachmentOptions: review.attachmentOptions }
          : current);
      } catch (cause) { setError(`本次附件未确认，尚未发送：${String(cause)}`); throw cause; }
    }}
    onSendDiscussion={async value => { if (!selectedId || browserAuthenticationRequired) return; try { await mobileApi.sendChatGptDiscussion(selectedId, value); } catch (cause) { if (!String(cause).includes("CHATGPT_ACCEPTED_PENDING_TERMINAL")) { setError(`送达未确认；请检查原记录，Router 不会自动重发：${String(cause)}`); throw cause; } } setManualCheckNotice("已发送，正在等待 ChatGPT 回复。送达与完成状态分别记录。"); await refreshIndex(); }}
    onOpenManualDiscussionDestination={resolveManualDiscussionDestination}
    manualDiscussionDestinationActionLabel="核对并显示当前精确 ChatGPT 链接"
    onReplyRead={(id) => void markReply(id, false).catch((cause) => setError(String(cause)))} onReplyHandled={(id) => void markReply(id, true).catch((cause) => setError(String(cause)))}
    onPrepareHandoff={reply?.provider === "CHATGPT" && Boolean(latestObservation?.markerText?.trim())
      ? (id) => prepareHandoff(id).catch((cause) => { setError(String(cause)); throw cause; })
      : reply?.provider === "CODEX" && latestObservation
        ? async () => { if ((latestObservation.attachments ?? []).length && selectedId) { setAttentionTarget({ workstreamId: selectedId, sourceId: latestObservation.id, kind: "CODEX_REPLY_OBSERVED" }); return "CODEX_HISTORY" as const; } await prepareCodexObservationHandoff(latestObservation).catch((cause) => { setError(String(cause)); throw cause; }); }
        : undefined}
    onAddSelectionToHandoff={reply?.provider === "CHATGPT" ? (id, text) => prepareHandoff(id, text).catch((cause) => { setError(String(cause)); throw cause; }) : undefined}
    onOpenChatGptOrigin={() => {
      const sourceId = handoff?.direction === "CHATGPT_TO_CODEX" ? handoff.candidates?.[0]?.id : null;
      const observation = sourceId ? observations.find((item) => item.id === sourceId && item.endpointId === snapshot?.activeChatgptEndpoint?.id) ?? null : null;
      if (observation && selectedId) {
        setAttentionTarget({ workstreamId: selectedId, sourceId: observation.id, kind: "CHATGPT_REPLY_OBSERVED" });
        setSurface("WORKSPACE");
        return;
      }
      setError("这条精确 ChatGPT 外部回复当前不可读；Router 没有切换到其他回复。");
    }}
    onHandoffMessageChange={(id, message) => setHandoff((current) => current?.id === id && current.status === "READY" ? { ...current, message } : current)} onOpenCodexAttachments={() => setSurface("CODEX_ATTACHMENTS")} onOpenCodexResults={() => { const sourceId = handoff?.direction === "CODEX_TO_CHATGPT" ? handoff.candidates?.[0]?.id : null; const observation = sourceId ? observations.find((item) => item.id === sourceId && item.endpointId === snapshot?.activeCodexEndpoint?.id) ?? null : null; if (observation && selectedId) { setAttentionTarget({ workstreamId: selectedId, sourceId: observation.id, kind: "CODEX_REPLY_OBSERVED" }); setSurface("CODEX_HISTORY"); return; } if (handoff?.origin?.sourceKind === "REPLY_OBSERVATION") { setError("这条精确 Codex 外部回复当前不可读；为避免把历史执行结果当成原回复，Router 没有切换页面。"); return; } if (sourceId && !reviewResults.some((item) => item.provider === "CODEX" && item.runId === sourceId)) { setError("这条精确 Codex 历史结果当前不可读；Router 没有切换到其他结果。"); return; } setSurface("CODEX_RESULTS"); }} onReopenReverseHandoff={() => { const sourceId = handoff?.direction === "CODEX_TO_CHATGPT" ? handoff.candidates?.[0]?.id : null; const observation = sourceId ? observations.find((item) => item.id === sourceId && item.endpointId === snapshot?.activeCodexEndpoint?.id) ?? null : null; if (observation) { void prepareCodexObservationHandoff(observation).then(() => setSurface("HANDOFF_REVIEW")).catch((cause) => setError(`无法重新准备外部 Codex 回复回传：${String(cause)}`)); return; } if (handoff?.origin?.sourceKind === "REPLY_OBSERVATION") { setError("这条精确 Codex 外部回复当前不可读；未用历史执行结果替代，也未创建新的编辑审阅。"); return; } const result = reviewResults.find((item) => item.runId === sourceId); if (!result) { setError("原 Codex 结果当前不可读，未创建新的编辑审阅。"); return; } const attachmentIds = selectedAttachmentIds[result.runId] ?? defaultAttachmentIds(result); void prepareCodexHandoff(result, attachmentIds).then(() => setSurface("HANDOFF_REVIEW")).catch((cause) => setError(`无法重新准备 Codex Handoff：${String(cause)}`)); }}
    onApproveHandoff={(id) => approveHandoff(id).catch((cause) => {
      setError(String(cause));
      throw cause;
    })} onSendHandoff={(id) => sendHandoff(id).catch((cause) => {
      setError(String(cause));
      throw cause;
    })}
    onRefreshHandoffStatus={(id) => refreshHandoffStatus(id).catch((cause) => {
      setError(`送达状态没有刷新：${String(cause)}`);
      throw cause;
    })}
    onLifecycleChange={(id, lifecycle) => changeLifecycle(id, lifecycle)}
    onPurgeTrashedWorkstream={(id) => { const target = snapshot?.workstreams.find((workstream) => workstream.id === id); if (!target) return; void mobileApi.purgeTrashedWorkstream(id, target.bindingRevision, "PURGE_LOCAL_WORKSTREAM").then(async () => { setSelectedId(null); await refreshIndex(); }).catch((cause) => setError(`永久清除未执行：${String(cause)}`)); }}
    onCreateVerifiedBackup={() => void mobileApi.createVerifiedLocalBackup().then((backup) => setBackupNotice(`已创建并验证本地备份：${backup.path} · ${backup.bytes} bytes · SHA-256 ${backup.sha256}`)).catch((cause) => setError(`一致性备份失败：${String(cause)}`))}
    onGoalAction={(threadId, action) => void changeGoal(threadId, action).catch((cause) => setError(String(cause)))}
  /></>;
}
