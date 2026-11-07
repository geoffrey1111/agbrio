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
  lastMessageAt?: number | null;
  lifecycle: WorkbenchLifecycle;
  pinned?: boolean;
  attentionCount?: number;
  attentionItems?: WorkbenchAttentionItem[];
  statusLabel?: string | null;
}


export interface WorkbenchDraft {
  value: string;
  revision?: number | null;
  savedAt?: number | null;
  saveState?: "SAVED" | "SAVING" | "FAILED";
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
