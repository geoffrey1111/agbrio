import {readModels} from "../features/workbench/readModelCache";
import {beginPwaMutation} from './pwaUpdates';
export type MobileWorkstream = {
  id: string;
  name: string;
  bindingSummary?: string;
  sourceLabel?: string;
  projectName?: string;
  status?: string;
  trashedAt?: number | null;
  pinnedAt?: number | null;
  updatedAt?: number;
  lastActivityAt?:number|null;
  chatgptStatus?: string;
  codexStatus?: string;
  attentionCount: number;
  attentionItems: {
    /** Exact Router source record for this attention item, when applicable. */
    sourceId?: string;
    kind: string;
    priority: number;
    message?: string;
    activityAt?: number;
  }[];
};

export type MobileAttachment = {
  id: string;
  filename: string;
  integrityStatus: string;
  defaultSelected: boolean;
  warnings?: string[];
};

/** A bounded, complete ProviderRun result. Provider identities stay in Router. */
export type MobileReviewResult = {
  runId: string;
  provider: "CHATGPT" | "CODEX";
  resultIdentity: string;
  text: string;
  reviewedAt?: number | null;
  /** Exact, server-validated handoff interior when one is available. */
  markerText?: string | null;
  attachments?: MobileAttachment[];
  /** Router-owned handle for one exact, re-readable empty result, if unambiguous. */
  recoveryRunId?: string | null;
};

/** Status-only projection for one exact Inbox-selected ProviderRun. */
export type MobileProviderRunStatus = {
  runId: string;
  provider: "CHATGPT" | "CODEX";
  status: "STARTING" | "RUNNING" | "COMPLETED" | "FAILED" | "CANCELLED" | "UNKNOWN";
  terminalCode?: string | null;
  originHandoffId?: string | null;
  startedAt?: number | null;
  terminalAt?: number | null;
  updatedAt: number;
  hasReviewableResult: boolean;
};

/** Bounded exact ChatGPT assistant reply, independent of ProviderRun causality. */
export type MobileReplyObservation = {
  id: string;
  endpointId?: string;
  text: string;
  observedAt: number;
  readAt?: number | null;
  handledAt?: number | null;
  pushState: "PENDING" | "SENT" | "FAILED" | "NO_SUBSCRIPTION" | "NOT_ATTEMPTED";
  /** Service Worker rendered the notification; this is not an iOS banner receipt. */
  pushRenderedAt?: number | null;
  markerText?: string | null;
  attachments?: MobileAttachment[];
};

/** One explicit no-send read of the exact active provider endpoint. */
export type MobileManualReplyCheck = {
  state: string;
  observationCreated: boolean;
  lastSuccessfulCheckAt?: number | null;
};

export type MobileCodexRequestChoice = { id: string; label: string };
export type MobileCodexRequestQuestion = {
  options?: {label:string;description:string}[];
  isOther?: boolean;
  isSecret?: boolean;
  id: string;
  label: string;
  placeholder?: string;
  required?: boolean;
};

/** Sanitized live state only; raw app-server JSON-RPC is intentionally absent. */
export type MobileCodexRequest = {
  requestId: string;
  revision: number;
  method: string;
  kind: string;
  /** Official displayable reason only; never a substituted raw command or cwd. */
  reason?: string;
  choices?: MobileCodexRequestChoice[];
  questions?: MobileCodexRequestQuestion[];
  isBlocking: boolean;
  responseSent: boolean;
};

export type ChatHistory = {
  title: string;
  completeness: string;
  messages: { id: string; role: string; text: string }[];
};
export type CodexHistory = { history: { id: string; kind: string; text?: string }[] };
export type MobileCodexGoal = { fingerprint?:string; threadId: string; objective: string; status: string; tokensUsed?: number | null; timeUsedSeconds?:number|null; tokenBudget?:number|null; createdAt?:number|null; updatedAt?: number | null; activeTurnId?: string | null };
export type MobileWorkstreamDraft = { workstreamId: string; text: string; revision: number; updatedAt: number };
export type MobileCodexFeedbackDraft = { workstreamId: string; sourceRunId: string; text: string; revision: number; updatedAt: number };
export type MobileReview = {
  actionId: string;
  revision: number;
  status: string;
  message: string;
  attachments?: string[];
  attachmentOptions?: string[];
  /** Router-authoritative safety capability; this is not a delivery result. */
  requiresManualDispatch?: boolean;
  /** Present only for a restart-safe Codex → ChatGPT review. All fields are
   * exact Router records, never text/title-based reconstruction. */
  codexOutboundIdentity?: {
    workstreamId: string;
    sourceId: string;
    sourceKind: "REPLY_OBSERVATION" | "PROVIDER_RUN";
    sourceEndpointId: string;
    sourceCodexThreadId: string;
    destinationChatgptConversationId: string;
  };
  /** Present only for a restart-safe ChatGPT → Codex review. */
  chatgptInboundIdentity?: {
    workstreamId: string;
    sourceId: string;
    sourceKind: "REPLY_OBSERVATION" | "PROVIDER_RUN";
    sourceEndpointId: string;
    sourceChatgptConversationId: string;
    destinationCodexThreadId: string;
  };
};
export type MobileOutboundHandoffResult = { status: "SENT" | "SENDING" | "FAILED"; handoffId?: string | null; detail?: string | null };
/** A read-only, exact owner-navigation target. It is not a provider send. */
export type MobileManualChatGptDestination = { canonicalUrl: string; conversationId: string };
export type MobileChatGptResultRecovery = { recovered: boolean; message: string };
export type MobileFeedbackProgress = {
  runId: string;
  phase: "WAITING_FOR_REVISED_RESULT" | "DELIVERY_UNCONFIRMED" | "DELIVERY_FAILED" | "REVISED_RESULT_READY";
  message: string;
};

export type MobileTurnStartResult = { turn_id: string };

export type MobileExternalProjectLink = {
  id: string;
  projectId: string;
  provider: "CHATGPT" | "CODEX";
  externalProjectId: string;
  canonicalUrl?: string | null;
  label: string;
  sourceKind: string;
  sourceVersion?: string | null;
  verifiedAt?: number | null;
  createdAt: number;updatedAt:number;
};

/** Read-only U10 projection; Core resolves the saved exact Project URL. */
export type MobileChatGptProjectDirectory = {
  projectUrl: string;
  conversations: { conversationId: string; canonicalUrl: string; title: string; occurredAt?: string | null; preview?: string | null }[];
  completeness: "COMPLETE" | "PARTIAL" | "EMPTY";
  hasMore: boolean;
  sourceKind: "PROJECT_CONTAINER";
  diagnostic?: string | null;
};

export type MobileUnprojectedThreadStart = {
  thread: { id: string; name?: string; preview?: string };
  directory: string;
};

/** Session-only result of the managed Host exact-conversation proof. */
export type MobileExplicitChatGptBindingCandidate = {
  workstreamId: string;
  externalId: string;
  label: string;
  verification: "PLAYWRIGHT_EXACT_ROUTE" | "OWNER_CONFIRMED_EXACT_URL";
  expectedOldEndpointId?: string | null;
  expectedBindingRevision: number;
};

/** Returned only by the checked, session-candidate-backed confirmation. */
export type MobileConfirmedChatGptEndpoint = {
  id: string;
  workstreamId: string;
  provider: "CHATGPT" | "CODEX";
  externalId: string;
  label: string;
  status: string;
  createdAt: number;
};

export type MobileWorkstreamSnapshot = {
  selectedWorkstreamId?: string | null;
  activeChatgptEndpoint?: MobileConfirmedChatGptEndpoint | null;
  activeCodexEndpoint?: MobileConfirmedChatGptEndpoint | null;
  workstreams: { id: string; projectId: string; name: string; status: string; updatedAt: number; bindingRevision: number; archivedAt?: number | null; trashedAt?: number | null;pinnedAt?:number|null }[];
  projects: { id: string; name: string }[];
  /** Existing Router-owned handoffs for this exact workstream only. */
  handoffs?: MobilePersistedHandoff[];
  /** Durable Codex → ChatGPT reviews. A review exists before any Handoff when
   * the owner has only prepared or approved the text. */
  mobileCodexOutboundReviews?: MobilePersistedCodexOutboundReview[];
  mobileChatgptInboundReviews?: MobilePersistedChatGptInboundReview[];
};

export type MobilePersistedCodexOutboundReview = {
  actionId: string;
  workstreamId: string;
  sourceRunId?: string | null;
  sourceReferenceId: string;
  sourceEndpointId: string;
  sourceCodexThreadId: string;
  destinationChatgptConversationId: string;
  originalText: string;
  approvedText?: string | null;
  revision: number;
  status: "READY" | "APPROVED" | "SENDING" | "SENT" | "FAILED";
  handoffId?: string | null;
  createdAt: number;
  updatedAt: number;
};

export type MobilePersistedChatGptInboundReview = {
  actionId: string;
  workstreamId: string;
  sourceRunId?: string | null;
  sourceReferenceId: string;
  sourceEndpointId: string;
  sourceChatgptConversationId: string;
  sourceResponseIdentity: string;
  destinationCodexThreadId: string;
  originalText: string;
  approvedText?: string | null;
  revision: number;
  status: "READY" | "APPROVED" | "SENDING" | "SENT" | "FAILED";
  handoffId?: string | null;
  createdAt: number;
  updatedAt: number;
};

export type MobilePersistedHandoff = {
  id: string;
  workstreamId: string;
  direction: "CHATGPT_TO_CODEX" | "CODEX_TO_CHATGPT";
  approvedText: string;
  createdAt: number;
  status: "READY" | "APPROVED" | "SENDING" | "SENT" | "FAILED";
  errorCode?: string | null;
  errorMessage?: string | null;
  sourceKind: "ENDPOINT" | "CONTROL_CONTEXT";
  sourceEndpoint?: { id: string; externalId: string };
  destinationEndpoint: { id: string; externalId: string };
};

export class MobileApiError extends Error {
  constructor(
    readonly status: number,
    message: string,
  ) {
    super(message);
    this.name = "MobileApiError";
  }
}

export async function request<T>(path:string,init?:RequestInit):Promise<T>{
 const reading=!init?.method||init.method.toUpperCase()==="GET";
 const key=path==="/workstreams"?"directory:mobile":/^\/workstreams\/[^/]+$/.test(path)?`workstream:${decodeURIComponent(path.split("/")[2])}`:null;
 if(reading&&key)return readModels.read(key,()=>liveRequest<T>(path,init),5000);
 const result=await liveRequest<T>(path,init);
 const action=typeof init?.body==="string"?(()=>{try{return JSON.parse(init.body).action;}catch{return null;}})():null;
 const passive=path==="/bridge-activity"||(/\/role-bridge$/.test(path)&&["SYNC","READ","THREADS","BLOCKS","ATTACHMENTS"].includes(action));
 if(!reading&&!passive){readModels.invalidate("bridge-activity:");readModels.invalidate("directory:");if(/\/(trash|purge|endpoints)$/.test(path)||(/\/role-bridge$/.test(path)&&action==="BIND")){readModels.invalidate("bridge:",true);readModels.invalidate("bridge-activity:",true);}}
 return result;
}
export async function liveRequest<T>(path: string, init?: RequestInit): Promise<T> {
  const end=init?.method&&!['GET','HEAD'].includes(init.method.toUpperCase())?beginPwaMutation():()=>{};
  try{
  const response = await fetch(`/v1/mobile${path}`, {
    ...init,
    cache: "no-store",
    headers: { "content-type": "application/json", ...(init?.headers ?? {}) },
  });
  if (!response.ok) {
    if(response.status===401)window.dispatchEvent(new Event("aiwr-login-required"));
    const body = await response.json().catch(() => null) as { error?: unknown } | null;
    const detail =
      typeof body?.error === "string" && body.error.trim()
        ? body.error.trim()
        : response.statusText || `HTTP ${response.status}`;
    throw new MobileApiError(response.status, `${detail} (Router HTTP ${response.status})`);
  }
  if (response.status === 204) return undefined as T;
  return await response.json() as T;
  }finally{end();}
}

export const mobileApi = {
  pushConfig: () => request<{ publicKey: string }>("/push/config"),
  pushStatus: () => request<{ activeSubscriptionCount: number }>("/push/status"),
  upsertPushSubscription: (subscription: PushSubscriptionJSON) => request<void>("/push/subscription", { method: "POST", body: JSON.stringify(subscription) }),
  removePushSubscription: (endpoint: string) => request<void>("/push/subscription", { method: "DELETE", body: JSON.stringify({ endpoint }) }),
  testPush: () => request<void>("/push/test", { method: "POST" }),
  testReplyPush: () => request<void>("/push/test-reply", { method: "POST" }),
  createBridge: (name:string)=>request<string>("/bridges",{method:"POST",body:JSON.stringify({name})}),
  renameBridge:(id:string,name:string)=>request<void>(`/bridges/${encodeURIComponent(id)}/rename`,{method:"POST",body:JSON.stringify({name})}),
  workstreams: () => request<MobileWorkstream[]>("/workstreams"),
  workstream: (workstreamId: string) => request<MobileWorkstreamSnapshot>(`/workstreams/${encodeURIComponent(workstreamId)}`),
  projectLinks: (workstreamId: string) => request<MobileExternalProjectLink[]>(`/workstreams/${encodeURIComponent(workstreamId)}/project-links`),
  confirmChatGptAuthenticationCompleted: () => request<void>("/chatgpt-browser/authentication-completed", { method: "POST" }),
  openHostChatGptBrowserSetup: () => request<void>("/chatgpt-browser/setup", { method: "POST" }),
  upsertProjectLink: (workstreamId: string, input: { provider: "CHATGPT" | "CODEX"; externalProjectId?: string | null; canonicalUrl?: string | null; label: string; sourceKind: string; sourceVersion?: string | null; verifiedAt?: number | null }) => request<MobileExternalProjectLink>(`/workstreams/${encodeURIComponent(workstreamId)}/project-links`, { method: "POST", body: JSON.stringify(input) }),
  startUnprojectedCodexThread: (workstreamId: string, directory?: string) => request<MobileUnprojectedThreadStart>(`/workstreams/${encodeURIComponent(workstreamId)}/no-project-codex-thread`, { method: "POST", body: JSON.stringify({ directory: directory?.trim() || null }) }),
  createVerifiedLocalBackup: () => request<{ path: string; sha256: string; bytes: number }>("/backup", { method: "POST" }),
  workstreamDraft: (workstreamId: string) => request<MobileWorkstreamDraft | null>(`/workstreams/${encodeURIComponent(workstreamId)}/draft`),
  saveWorkstreamDraft: (workstreamId: string, text: string, expectedRevision?: number | null) => request<MobileWorkstreamDraft>(`/workstreams/${encodeURIComponent(workstreamId)}/draft`, { method: "POST", body: JSON.stringify({ text, expectedRevision: expectedRevision ?? null }) }),
  codexFeedbackDraft: (workstreamId: string, sourceRunId: string) => request<MobileCodexFeedbackDraft | null>(`/workstreams/${encodeURIComponent(workstreamId)}/codex-feedback-drafts/${encodeURIComponent(sourceRunId)}`),
  saveCodexFeedbackDraft: (workstreamId: string, sourceRunId: string, text: string, expectedRevision?: number | null) => request<MobileCodexFeedbackDraft>(`/workstreams/${encodeURIComponent(workstreamId)}/codex-feedback-drafts/${encodeURIComponent(sourceRunId)}`, { method: "POST", body: JSON.stringify({ text, expectedRevision: expectedRevision ?? null }) }),
  archiveWorkstream: (workstreamId: string) => request<void>(`/workstreams/${encodeURIComponent(workstreamId)}/lifecycle/archive`, { method: "POST" }),
  setWorkstreamPinned: (workstreamId: string, pinned: boolean) => request<void>(`/workstreams/${encodeURIComponent(workstreamId)}/lifecycle/pin`, { method: "POST", body: JSON.stringify({ pinned }) }),
  trashWorkstream: (workstreamId: string) => request<void>(`/workstreams/${encodeURIComponent(workstreamId)}/lifecycle/trash`, { method: "POST" }),
  restoreWorkstream: (workstreamId: string) => request<void>(`/workstreams/${encodeURIComponent(workstreamId)}/lifecycle/restore`, { method: "POST" }),
  purgeTrashedWorkstream: (workstreamId: string, expectedBindingRevision: number, confirmation: string) => request<void>(`/workstreams/${encodeURIComponent(workstreamId)}/lifecycle/purge`, { method: "POST", body: JSON.stringify({ expectedBindingRevision, confirmation }) }),
  pairWorkstreamEndpoints: (workstreamId: string, input: { expectedBindingRevision: number; chatgpt?: { expectedActiveEndpointId?: string | null; externalId: string; label: string } | null; codex?: { expectedActiveEndpointId?: string | null; externalId: string; label: string } | null }) => request(`/workstreams/${encodeURIComponent(workstreamId)}/endpoint-pairing`, { method: "POST", body: JSON.stringify(input) }),
  prepareOwnerConfirmedChatGptEndpointBinding: (workstreamId: string, input: string) => request<MobileExplicitChatGptBindingCandidate>(`/workstreams/${encodeURIComponent(workstreamId)}/chatgpt-binding/prepare`, { method: "POST", body: JSON.stringify({ input }) }),
  confirmExplicitChatGptEndpointBinding: (workstreamId: string) => request<MobileConfirmedChatGptEndpoint>(`/workstreams/${encodeURIComponent(workstreamId)}/chatgpt-binding/confirm`, { method: "POST" }),
  reviewResults: (workstreamId: string) => request<MobileReviewResult[]>(`/workstreams/${encodeURIComponent(workstreamId)}/review-results`),
  providerRunStatus: (workstreamId: string, runId: string) => request<MobileProviderRunStatus>(`/workstreams/${encodeURIComponent(workstreamId)}/provider-runs/${encodeURIComponent(runId)}`),
  replyObservations: (workstreamId: string) => request<MobileReplyObservation[]>(`/workstreams/${encodeURIComponent(workstreamId)}/reply-observations`),
  checkNewChatGptReplies: (workstreamId: string) => request<MobileManualReplyCheck>(`/workstreams/${encodeURIComponent(workstreamId)}/chatgpt-replies/check`, { method: "POST" }),
  checkNewCodexReplies: (workstreamId: string) => request<MobileManualReplyCheck>(`/workstreams/${encodeURIComponent(workstreamId)}/codex-replies/check`, { method: "POST" }),
  markReplyObservationRead: (workstreamId: string, observationId: string) => request<void>(`/workstreams/${encodeURIComponent(workstreamId)}/reply-observations/${encodeURIComponent(observationId)}/read`, { method: "POST" }),
  markReplyObservationHandled: (workstreamId: string, observationId: string) => request<void>(`/workstreams/${encodeURIComponent(workstreamId)}/reply-observations/${encodeURIComponent(observationId)}/handled`, { method: "POST" }),
  recoverChatGptResult: (workstreamId: string) => request<MobileChatGptResultRecovery>(`/workstreams/${encodeURIComponent(workstreamId)}/chatgpt-result-recovery`, { method: "POST" }),
  recoverChatGptFeedbackResult: (workstreamId: string, runId: string) => request<MobileChatGptResultRecovery>(`/workstreams/${encodeURIComponent(workstreamId)}/chatgpt-feedback/${encodeURIComponent(runId)}/result-recovery`, { method: "POST" }),
  codexRequests: (workstreamId: string) => request<MobileCodexRequest[]>(`/workstreams/${encodeURIComponent(workstreamId)}/codex-requests`),
  chatgptHistory: (workstreamId: string) => request<ChatHistory>(`/workstreams/${encodeURIComponent(workstreamId)}/chatgpt-history`),
  codexHistory: (workstreamId: string) => request<CodexHistory>(`/workstreams/${encodeURIComponent(workstreamId)}/codex-history`),
  codexGoal: (workstreamId: string) => request<MobileCodexGoal | null>(`/workstreams/${encodeURIComponent(workstreamId)}/codex-goal`),
  pauseCodexGoal: (workstreamId: string) => request<MobileCodexGoal>(`/workstreams/${encodeURIComponent(workstreamId)}/codex-goal/pause`, { method: "POST", body: JSON.stringify({ confirmed: true }) }),
  resumeCodexGoal: (workstreamId: string) => request<MobileCodexGoal>(`/workstreams/${encodeURIComponent(workstreamId)}/codex-goal/resume`, { method: "POST", body: JSON.stringify({ confirmed: true }) }),
  clearCodexGoal: (workstreamId: string) => request<void>(`/workstreams/${encodeURIComponent(workstreamId)}/codex-goal/clear`, { method: "POST", body: JSON.stringify({ confirmed: true }) }),
  interruptCodexTurn: (workstreamId: string, turnId: string) => request<{ threadId: string; turnId: string; requested: boolean }>(`/workstreams/${encodeURIComponent(workstreamId)}/codex-turns/${encodeURIComponent(turnId)}/interrupt`, { method: "POST", body: JSON.stringify({ confirmed: true }) }),
  prepareHandoff: (workstreamId: string, responseId: string, initialMessage?: string) => request<MobileReview>(`/workstreams/${encodeURIComponent(workstreamId)}/handoff-review`, { method: "POST", body: JSON.stringify(initialMessage?.trim() ? { responseId, initialMessage } : { responseId }) }),
  selectHandoffAttachments: (actionId: string, revision: number, attachmentFilenames: string[]) => request<MobileReview>(`/handoff-review/${encodeURIComponent(actionId)}/attachments`, { method: "POST", body: JSON.stringify({ revision, attachmentFilenames }) }),
  approveHandoff: (actionId: string, revision: number, message: string) => request<MobileReview>(`/handoff-review/${encodeURIComponent(actionId)}/approve`, { method: "POST", body: JSON.stringify({ revision, message }) }),
  sendHandoff: (actionId: string, revision: number) => request(`/handoff-review/${encodeURIComponent(actionId)}/send`, { method: "POST", body: JSON.stringify({ revision }) }),
  prepareCodexHandoff: (workstreamId: string, runId: string, attachmentIds: string[]) => request<MobileReview>(`/workstreams/${encodeURIComponent(workstreamId)}/codex-handoff-review`, { method: "POST", body: JSON.stringify({ runId, attachmentIds }) }),
  approveCodexHandoff: (actionId: string, revision: number, message: string) => request<MobileReview>(`/codex-handoff-review/${encodeURIComponent(actionId)}/approve`, { method: "POST", body: JSON.stringify({ revision, message }) }),
  codexHandoffReview: (actionId: string) => request<MobileReview>(`/codex-handoff-review/${encodeURIComponent(actionId)}`),
  codexHandoffManualDestination: (actionId: string) => request<MobileManualChatGptDestination>(`/codex-handoff-review/${encodeURIComponent(actionId)}/manual-destination`),
  chatGptHandoffReview: (actionId: string) => request<MobileReview>(`/chatgpt-handoff-review/${encodeURIComponent(actionId)}`),
  sendCodexHandoff: (actionId: string, revision: number) => request<MobileOutboundHandoffResult>(`/codex-handoff-review/${encodeURIComponent(actionId)}/send`, { method: "POST", body: JSON.stringify({ revision }) }),
  sendChatGptFeedback: (workstreamId: string, runId: string, feedback: string) => request<MobileFeedbackProgress>(`/workstreams/${encodeURIComponent(workstreamId)}/chatgpt-feedback`, { method: "POST", body: JSON.stringify({ runId, feedback }) }),
  chatgptFeedbackProgress: (workstreamId: string, runId: string) => request<MobileFeedbackProgress>(`/workstreams/${encodeURIComponent(workstreamId)}/chatgpt-feedback/${encodeURIComponent(runId)}`),
  sendCodexFeedback: (workstreamId: string, runId: string, feedback: string) => request<MobileTurnStartResult>(`/workstreams/${encodeURIComponent(workstreamId)}/codex-feedback`, { method: "POST", body: JSON.stringify({ runId, feedback }) }),
  sendChatGptDiscussion: (workstreamId: string, message: string) => request<unknown>(`/workstreams/${encodeURIComponent(workstreamId)}/chatgpt-discussion`, { method: "POST", body: JSON.stringify({ message }) }),
  /** Owner-led, read-only exact destination for a manually sent discussion. */
  manualChatGptDiscussionDestination: (workstreamId: string) => request<MobileManualChatGptDestination>(`/workstreams/${encodeURIComponent(workstreamId)}/manual-chatgpt-destination`),
  respondToCodexRequest: (workstreamId: string, requestId: string, input: { revision: number; decision?: string; answers?: Record<string, string> }) => request<void>(`/workstreams/${encodeURIComponent(workstreamId)}/codex-requests/${encodeURIComponent(requestId)}/respond`, { method: "POST", body: JSON.stringify(input) }),
};
