/**
 * Presentation contracts for the V3 workbench.  IDs are carried through every
 * callback, but are intentionally not used as the primary user-facing label.
 * The host supplies these records from the Router authority; this feature does
 * not read provider state, browser state, or a local database.
 */
export type WorkbenchLifecycle = "ACTIVE" | "ARCHIVED" | "TRASHED";

/**
 * One Router-owned item requiring attention.  It is deliberately separate
 * from a Workstream: a Workstream can have several unrelated things waiting
 * for the owner (a reply, a failed delivery, or a structured Codex request).
 */
export interface WorkbenchAttentionItem {
  /** The exact Router record identity where one exists; never a title match. */
  sourceId?: string;
  kind: string;
  priority: number;
  /** Router-supplied, display-safe explanation of what is waiting. */
  message?: string;
  activityAt?: number | null;
}

export interface WorkbenchItem {
  id: string;
  name: string;
  projectName?: string | null;
  sourceLabel?: string | null;
  updatedAt?: number | null;
  lifecycle: WorkbenchLifecycle;
  pinned?: boolean;
  attentionCount?: number;
  attentionItems?: WorkbenchAttentionItem[];
  statusLabel?: string | null;
}

export interface ExactReply {
  id: string;
  workstreamId: string;
  provider: "CHATGPT" | "CODEX";
  title?: string | null;
  observedAt?: number | null;
  readState: "UNREAD" | "READ" | "HANDLED";
  text: string;
  completeness?: "COMPLETE" | "PARTIAL" | "UNAVAILABLE";
  sourceLabel?: string | null;
  /** Push delivery is distinct from PWA rendering and from an OS banner. */
  notificationState?: "PENDING" | "SENT" | "FAILED" | "NO_SUBSCRIPTION" | "NOT_ATTEMPTED" | null;
  /** A same-origin Service Worker completed notification rendering, not a banner receipt. */
  notificationRenderedAt?: number | null;
  /** Ephemeral exact-provider history is display-only, never a ReplyObservation. */
  readOnly?: boolean;
  /** A bounded presentation classification; never a raw provider error. */
  historyReadFailure?: "NORMAL_BROWSER_REMOTE_DEBUGGING_REQUIRED" | "NORMAL_BROWSER_CONNECT_TIMED_OUT" | "NORMAL_BROWSER_PERMISSION_DENIED" | "NORMAL_BROWSER_CONNECTION_FAILED" | "NORMAL_BROWSER_UNAVAILABLE" | "ACCOUNT_SECURITY_REQUIRED" | "PROFILE_IN_USE" | "MANUAL_READ_REQUIRED" | "AUTH_REQUIRED" | "CONTENT_NOT_READY" | "CONTENT_EPOCH_CHANGED" | "COMMAND_NOT_ACCEPTED" | "COMMAND_SOURCE_MISMATCH" | "EXACT_CLIENT_UNAVAILABLE" | "HOST_ACTIVATION_EXACT_CLIENT_UNAVAILABLE" | "HISTORY_INITIAL_EXACT_CLIENT_UNAVAILABLE" | "HISTORY_AFTER_EMPTY_EXACT_CLIENT_UNAVAILABLE" | "HISTORY_POST_READ_EXACT_CLIENT_UNAVAILABLE" | "EXACT_CLIENT_AMBIGUOUS" | "IDENTITY_MISMATCH" | "CONVERSATION_CHANGED" | "BRIDGE_UNAVAILABLE" | "OTHER";
}

export interface WorkbenchDraft {
  value: string;
  revision?: number | null;
  savedAt?: number | null;
  saveState?: "SAVED" | "SAVING" | "FAILED";
}

export interface HandoffCandidate {
  id: string;
  label: string;
  text: string;
  sourceLabel?: string | null;
}

export interface HandoffPresentation {
  id: string;
  /** Durable Router Handoff identity once a reverse dispatch record exists. */
  handoffId?: string | null;
  revision?: number;
  status: "READY" | "APPROVED" | "SENDING" | "SENT" | "FAILED" | "CANCELLED";
  direction: "CHATGPT_TO_CODEX" | "CODEX_TO_CHATGPT";
  message: string;
  candidates?: HandoffCandidate[];
  /** Exact attachment labels selected during this local review; never inferred. */
  selectedAttachmentLabels?: string[];
  /** The exact active Endpoint resolved when this review was prepared. It is
   * presentation evidence only; routing remains Router-owned by workstream
   * and Endpoint identity. */
  destination?: {
    provider: "CHATGPT" | "CODEX";
    label: string;
    externalId: string;
    /** Exact, Router-validated owner-navigation URL when one is explicitly resolved. */
    canonicalUrl?: string | null;
  } | null;
  /** Exact downloadable attachments exposed by the reviewed ChatGPT message. */
  attachmentOptions?: string[];
  canApprove: boolean;
  canSend: boolean;
  /** The Router safety boundary requires an owner-operated, manual ChatGPT send. */
  requiresManualDispatch?: boolean;
  unavailableReason?: string | null;
  deliveryDetail?: string | null;
  /** Stable Router failure category; never a browser diagnostic. */
  deliveryCode?: string | null;
  runStatus?: string | null;
  /** Router-observed origin shown in the review shell; never inferred from text. */
  origin?: {
    provider: "CHATGPT" | "CODEX";
    /**
     * Identifies the Router record class that supplied `candidates[0]`.
     * Presentation may use this only to return to that exact record; it must
     * never substitute a similarly named/current record of another class.
     */
    sourceKind?: "REPLY_OBSERVATION" | "PROVIDER_RUN";
    observedAt?: number | null;
    manualSelectionCount?: number;
  } | null;
}

export interface GoalPresentation {
  threadId: string;
  text?: string | null;
  status: "ACTIVE" | "PAUSED" | "BLOCKED" | "LIMITED" | "COMPLETE" | "UNKNOWN" | "NONE";
  reportedAt?: number | null;
  readAt?: number | null;
  usageLabel?: string | null;
  controllableActions: Array<"PAUSE" | "RESUME" | "DELETE" | "STOP_TURN">;
  unavailableReason?: string | null;
  /** Only present for the exact Router-observed active Turn. */
  activeTurnId?: string | null;
}

export interface LifecyclePresentation {
  canArchive: boolean;
  canTrash: boolean;
  canRestore: boolean;
  /** Available only for a local, already-trashed Workstream. */
  canPurge?: boolean;
  destructiveReason?: string | null;
  trashCount?: number;
  backupNotice?: string | null;
}

export interface RuntimeCheck {
  id: string;
  label: string;
  state: "READY" | "WARNING" | "UNAVAILABLE" | "CHECKING";
  detail: string;
  /** A capability-specific action supplied by the Host; never inferred from a label. */
  action?: () => void;
  /** Visible only when the action above is available to this Runtime row. */
  actionLabel?: string;
  /** A separate user-operated fallback; it must never be inferred from action. */
  secondaryAction?: () => void;
  secondaryActionLabel?: string;
}

export interface RuntimePresentation {
  checks: RuntimeCheck[];
  onOpenDiagnostics?: () => void;
  /** Outcome of a user-invoked runtime action. It must be visible on the
   * runtime surface itself, so an action never appears to have done nothing. */
  notice?: string | null;
}
