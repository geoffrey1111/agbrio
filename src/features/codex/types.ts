import type { AttachmentCandidate } from "../../domain/attachmentCandidate";

export type FeedKind =
  | "UserMessage"
  | "AgentMessage"
  | "Progress"
  | "Command"
  | "FileChange"
  | "Approval"
  | "Warning"
  | "Completion"
  | "Unknown";

export interface FeedEvent {
  id: string;
  kind: FeedKind;
  method: string;
  itemId?: string;
  threadId?: string;
  turnId?: string;
  text?: string;
  detail?: string;
  raw?: string;
}

export interface ThreadSummary {
  id: string;
  name?: string;
  preview?: string;
}
/** Read-only app-server catalog entry. The ID remains routing data, while the
 * normal UI leads with provider-supplied human metadata. */
export interface ExistingCodexThreadCandidate {
  id: string;
  label: string;
  preview?: string | null;
  updatedAt?: string | null;
  projectProvenance?: string | null;
  /** Native recencyAt, Unix seconds; never inferred from creation or local scan time. */
  recencyAt?: number | null;
  projectId?: string | null;
  projectLabel?: string | null;
  projectStatus?: "PROJECT" | "PROJECTLESS" | "UNCONFIRMED";
}
export interface ExistingCodexThreadCatalog {
  threads: ExistingCodexThreadCandidate[];
  complete: boolean;
}

export interface ResumeResult {
  thread: ThreadSummary;
  history: FeedEvent[];
}

/** Exact-ID, read-only Codex inspection. It never grants writer ownership. */
export interface ReadHistoryResult {
  thread: ThreadSummary;
  history: FeedEvent[];
}

export interface TurnStartResult {
  turn_id: string;
}

export interface BackendStatus {
  connected: boolean;
  connecting?: boolean;
  detail?: string;
}

export interface HostEnvironmentStatus {
  host: string;
  mobile: string;
  browserRuntime: string;
  chatgptBrowserMode: string;
}
/** `thread/start` proves the returned exact ID only; fresh-server reopen
 * capability is deliberately not inferred from its non-ephemeral request. */
export interface UnprojectedThreadStart { thread: ThreadSummary; directory: string; }

export interface CodexGoal { threadId: string; objective: string; status: string; tokenBudget?: number | null; tokensUsed?: number | null; timeUsedSeconds?: number | null; createdAt?: unknown; updatedAt?: unknown; activeTurnId?: string | null; }
export interface CodexTurnInterruptResult { threadId: string; turnId: string; requested: boolean; }
/** Router-owned projection of one live official Codex server request. The
 * opaque request ID is intentionally not presentation data. */
export interface CodexStructuredRequest { requestId: string; revision: number; method: string; kind: string; reason?:string|null;detail?: string | null; choices: { id: string; label: string }[]; questions: { id: string; label: string; placeholder?: string | null; required: boolean }[]; isBlocking: boolean; responseSent: boolean; }

export interface RouterProject { id: string; name: string; description?: string | null; createdAt: number; updatedAt: number; }
export interface RouterWorkstream { id: string; projectId: string; name: string; status: "ACTIVE" | "ARCHIVED"; createdAt: number; updatedAt: number; bindingRevision: number; archivedAt?: number | null; trashedAt?: number | null; pinnedAt?: number | null; }
export interface ExternalProjectLink { id: string; projectId: string; provider: "CHATGPT" | "CODEX"; externalProjectId: string; canonicalUrl?: string | null; label: string; sourceKind: string; sourceVersion?: string | null; verifiedAt?: number | null; createdAt: number; updatedAt: number; }
export interface ExternalProjectLinkInput { provider: "CHATGPT" | "CODEX"; externalProjectId?: string | null; canonicalUrl?: string | null; label: string; sourceKind: string; sourceVersion?: string | null; verifiedAt?: number | null; }
export interface WorkstreamDraft { workstreamId: string; text: string; revision: number; updatedAt: number; }
export interface CodexFeedbackDraft { workstreamId: string; sourceRunId: string; text: string; revision: number; updatedAt: number; }
export interface VerifiedBackup { path: string; sha256: string; bytes: number; }
export interface WorkstreamReviewResult { runId: string; provider: "CHATGPT" | "CODEX"; resultIdentity: string; text: string; reviewedAt?: number | null; markerText?: string | null; attachments: { id: string; filename: string; integrityStatus: string; defaultSelected: boolean; warnings?: string[] }[]; }
/** Status-only projection of one exact Inbox-selected ProviderRun. It never includes a transcript. */
export interface ProviderRunStatus { runId: string; provider: "CHATGPT" | "CODEX"; status: "STARTING" | "RUNNING" | "COMPLETED" | "FAILED" | "CANCELLED" | "UNKNOWN"; terminalCode?: string | null; originHandoffId?: string | null; startedAt?: number | null; terminalAt?: number | null; updatedAt: number; hasReviewableResult: boolean; }
/** Bounded Router-observed reply for the exact currently active ChatGPT Endpoint. */
export interface ReplyObservation { id: string; endpointId?: string; text: string; observedAt: number; readAt?: number | null; handledAt?: number | null; pushState: string; pushRenderedAt?: number | null; markerText?: string | null; attachments?: WorkstreamReviewResult["attachments"]; }
/** Result of one user-invoked, read-only check against the exact active ChatGPT Endpoint. */
export interface ChatGptManualRefreshResult { state: string; observationCreated: boolean; }
/** Narrow result of a user-requested default-browser lookup. Router will not
 * open a possibly duplicate tab when the existing read-only Chrome connection
 * cannot confirm the exact background-tab identity. */
export interface DefaultBrowserOpenResult { state: "ALREADY_OPEN" | "OPEN_REQUESTED" | "CHECK_REQUIRED"; }
/** One manual exact existing-Codex observation; never a ProviderRun. */
export interface CodexManualRefreshResult { state: string; observationCreated: boolean; lastSuccessfulCheckAt?: number | null; }
export interface HandoffReviewSession { actionId: string; revision: number; status: "READY" | "APPROVED" | "SENDING" | "SENT" | "FAILED"; message: string; attachments: string[]; /** Exact downloadable files on the reviewed ChatGPT message; none is preselected. */ attachmentOptions?: string[]; /** Router-authoritative safety capability: approved content must be copied and manually sent, not dispatched by Router. */ requiresManualDispatch?: boolean; }
export interface EndpointPairingSideInput { expectedActiveEndpointId?: string | null; externalId: string; label: string; }
export interface EndpointPairingInput { expectedBindingRevision: number; chatgpt?: EndpointPairingSideInput | null; codex?: EndpointPairingSideInput | null; }
export interface EndpointPairingResult { bindingRevision: number; chatgptEndpoint?: RouterEndpoint | null; codexEndpoint?: RouterEndpoint | null; }
/** Session-only exact URL candidate; no Endpoint exists until confirmation. */
export interface ExplicitChatGptBindingCandidate { workstreamId: string; externalId: string; label: string; verification: "PLAYWRIGHT_EXACT_ROUTE" | "OWNER_CONFIRMED_EXACT_URL"; expectedOldEndpointId?: string | null; expectedBindingRevision: number; }
export interface RouterEndpoint { id: string; workstreamId: string; provider: "CHATGPT" | "CODEX"; externalId: string; label: string; status: "ACTIVE" | "SUPERSEDED" | "ARCHIVED"; replacesEndpointId?: string | null; createdAt: number; supersededAt?: number | null; }
export interface RolloverCandidate { workstreamId: string; provider: "CHATGPT" | "CODEX"; expectedOldEndpointId: string; expectedOldExternalId: string; candidateExternalId: string; label: string; state: "CREATED" | "INITIALIZING" | "READY_TO_REPLACE" | "FAILED"; initializationText?: string | null; initializationTurnId?: string | null; initializationResultIdentity?: string | null; initializationStartedAt?: number | null; initializationTerminalAt?: number | null; error?: string | null; }
export interface PersistedAttachment { id: string; handoffId: string; filename: string; originalPath: string; size?: number | null; sha256?: string | null; integrityStatus?: string | null; sendSha256?: string | null; sendVerifiedAt?: number | null; createdAt: number; }
export interface PersistedHandoff { id: string; workstreamId: string; direction: "CHATGPT_TO_CODEX" | "CODEX_TO_CHATGPT"; sourceResponseIdentity?: string | null; originalText: string; approvedText: string; status: "READY" | "APPROVED" | "SENDING" | "SENT" | "FAILED" | "CANCELLED"; payloadHash: string; createdAt: number; approvedAt?: number | null; sentAt?: number | null; failedAt?: number | null; errorCode?: string | null; errorMessage?: string | null; bridgeRequestId?: string | null; bridgeTurnKey?: string | null; bridgeCorrelationObservedAt?: number | null; sourceEndpoint: RouterEndpoint; destinationEndpoint: RouterEndpoint; attachments: PersistedAttachment[]; }
export interface WorkspaceSnapshot { projects: RouterProject[]; workstreams: RouterWorkstream[]; selectedProjectId?: string | null; selectedWorkstreamId?: string | null; activeChatgptEndpoint?: RouterEndpoint | null; activeCodexEndpoint?: RouterEndpoint | null; endpointLineage: RouterEndpoint[]; handoffs: PersistedHandoff[]; }
export interface ProviderRun { id: string; workstreamId: string; endpointId: string; provider: "CHATGPT" | "CODEX"; originHandoffId?: string | null; externalRunId?: string | null; status: "STARTING" | "RUNNING" | "COMPLETED" | "FAILED" | "CANCELLED" | "UNKNOWN"; reviewedAt?: number | null; terminalAt?: number | null; updatedAt: number; }
export interface AttentionItem { kind: "DELIVERY_UNCERTAIN" | "HANDOFF_FAILED" | "CODEX_REPLY_OBSERVED" | "CHATGPT_REPLY_OBSERVED" | "CODEX_RESULT_READY" | "CHATGPT_RESULT_READY" | "PROVIDER_RUN_FAILED" | "PROVIDER_RUN_CANCELLED" | "MISSING_CHATGPT_BINDING" | "MISSING_CODEX_BINDING" | "UNKNOWN_RUN"; priority: number; workstreamId: string; workstreamName: string; sourceId: string; message: string; activityAt: number; }
export interface WorkstreamDashboardItem { projectId: string; projectName: string; workstream: RouterWorkstream; chatgptEndpoint?: RouterEndpoint | null; codexEndpoint?: RouterEndpoint | null; chatgptRun?: ProviderRun | null; codexRun?: ProviderRun | null; attentionItems: AttentionItem[]; lastActivityAt: number; }
export interface DashboardProjection { workstreams: WorkstreamDashboardItem[]; attentionItems: AttentionItem[]; }

export interface MessageAttachments {
  source_message_id: string;
  candidates: AttachmentCandidate[];
}

export interface ChatGptStatus {
  connected: boolean;
  detail?: string;
  clients: number;
  needsSelection: boolean;
}

/** Session-only fact from the Router-owned Playwright carrier. A requested URL
 * is never a binding proof; only its actual current URL may be EXACT_BOUND. */
export interface BoundChatGptProviderSurfaceStatus {
  state: "NOT_OPEN" | "EXACT_BOUND" | "IDENTITY_MISMATCH";
  currentConversationId?: string | null;
}

export interface ChatGptProviderSurfaceSnapshot {
  href: string;
  conversationId: string;
  turns: Array<{ id?: string | null; role: "USER" | "ASSISTANT" | "UNKNOWN"; text: string }>;
}

export interface BoundChatGptConversation {
  id: string;
  title?: string;
}

export interface ChatGptConversationHistory {
  id: string;
  title: string;
  branchScope: "CURRENT_VISIBLE_BRANCH";
  completeness: "COMPLETE_VISIBLE_BRANCH" | "PARTIAL" | "FAILED";
  messages: ChatGptConversationHistoryMessage[];
  diagnostics: {
    beginningReached: boolean;
    endReached: boolean;
    beginningSteps: number;
    endSteps: number;
    initialDomMessageCount: number;
    scrollTarget: "TURN_ANCESTOR" | "ROOT" | "DOCUMENT" | "WINDOW";
    viewportHeight: number;
    scrollableHeight: number;
    endLastScrollTop: number;
    endLastScrollableHeight: number;
    endStalledSteps: number;
    scrollRestored: boolean;
  };
}

export interface ChatGptConversationHistoryMessage {
  id: string;
  role: "USER" | "ASSISTANT";
  text: string;
  blocks: unknown[];
  codeBlocks: unknown[];
  resources: unknown[];
}

export interface ChatGptStreamEvent {
  eventType: "answer.delta" | "answer.snapshot" | "artifact.snapshot" | "request.result" | string;
  text?: string;
  detail?: string;
  artifacts?: unknown;
  conversationId?: string;
  responseIdentity?: string;
  completed: boolean;
  relayCandidates?: RelayCandidate[];
}

export interface HandoffAttachment {
  id: string;
  path: string;
  filename: string;
  actualSha256?: string | null;
  integrityStatus?: AttachmentCandidate["integrity_status"] | null;
}

export interface HandoffDraft {
  workstreamId: string;
  sourceCodexThreadId: string;
  destinationChatgptConversationId: string;
  originalText: string;
  message: string;
  attachments: HandoffAttachment[];
  status: "READY" | "APPROVED" | "SENDING" | "SENT" | "FAILED";
}

export interface OutboundHandoffResult { status: "SENT" | "SENDING"; finalText?: string | null; detail?: string | null; }

export interface DeliveryRecoveryCheck { handoffId: string; requestId: string; conversationId: string; terminalCode?: string | null; outcome: "PENDING" | "SUCCESS" | "ERROR" | "INSUFFICIENT"; message: string; }

export interface ReverseHandoffDraft {
  workstreamId: string;
  sourceChatgptConversationId: string;
  sourceResponseIdentity: string;
  sourceCandidateId?: string | null;
  destinationCodexThreadId: string;
  message: string;
  status: "READY" | "APPROVED" | "SENDING" | "SENT" | "FAILED";
}

export interface CompletedChatGptResponse {
  conversationId: string;
  responseIdentity: string;
  text: string;
  /** Present only when a bounded result is re-opened from Router process state. */
  finalText?: string;
  artifacts?: unknown;
  relayCandidates: RelayCandidate[];
}

export interface RelayCandidate {
  id: string;
  kind: "EXPLICIT_MARKER" | "CONTEXTUAL_CODE_BLOCK" | "GENERIC_CODE_BLOCK";
  confidence: "HIGH" | "MANUAL";
  text: string;
}
