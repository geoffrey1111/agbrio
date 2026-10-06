import { invoke } from "@tauri-apps/api/core";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";

export type RouterViewKind = "workspace" | "codex-workspace" | "decision-workspace" | "dashboard";

export type RouterWindowContext = {
  view: RouterViewKind;
  workstreamId?: string;
  responseIdentity?: string;
  /** Isolated local presentation route; never carries provider or Router identity. */
  visualFixture?: "workflow-acceptance";
};

export function currentRouterWindowContext(search = window.location.search): RouterWindowContext {
  const params = new URLSearchParams(search);
  const rawView = params.get("view");
  const view: RouterViewKind = rawView === "codex-workspace" || rawView === "decision-workspace" || rawView === "dashboard" ? rawView : "workspace";
  const workstreamId = params.get("workstream")?.trim() || undefined;
  const responseIdentity = params.get("response")?.trim() || undefined;
  const visualFixture = params.get("visualFixture") === "workflow-acceptance"
    ? "workflow-acceptance"
    : undefined;
  return {
    view,
    workstreamId: view === "workspace" ? workstreamId : undefined,
    responseIdentity: view === "workspace" ? responseIdentity : undefined,
    ...(visualFixture ? { visualFixture } : {}),
  };
}

function labelFor(context: RouterWindowContext) {
  if (context.visualFixture) return `${context.view}-${context.visualFixture}`;
  if (context.view === "codex-workspace" || context.view === "decision-workspace" || context.view === "dashboard") return context.view;
  return `workstream-${context.workstreamId ?? "new"}-${crypto.randomUUID()}`;
}

/** Opens a normal Tauri native window. Placement and lifecycle stay with Windows. */
export async function openRouterWindow(context: RouterWindowContext) {
  const label = labelFor(context);
  const existing = await WebviewWindow.getByLabel(label);
  if (existing) {
    await existing.show();
    await existing.setFocus();
    return label;
  }
  const params = new URLSearchParams({ view: context.view });
  if (context.workstreamId) params.set("workstream", context.workstreamId);
  if (context.responseIdentity) params.set("response", context.responseIdentity);
  if (context.visualFixture) params.set("visualFixture", context.visualFixture);
  new WebviewWindow(label, {
    title: context.view === "codex-workspace" ? "AI Work Router — Codex Workspace" : context.view === "decision-workspace" ? "AI Work Router — AI Decision Workspace" : context.view === "dashboard" ? "AI Work Router — Router Board" : "AI Work Router — Workstream",
    url: `${window.location.origin}${window.location.pathname}?${params.toString()}`,
    width: context.view === "workspace" ? 1180 : 980,
    height: 760,
    minWidth: 680,
    minHeight: 540,
  });
  return label;
}

export type SanitizedMonitor = {
  name: string;
  width: number;
  height: number;
  scaleFactor: number;
};

export type NativeWindowEvidence = {
  windowLabel?: string;
  currentMonitor?: SanitizedMonitor;
  availableMonitors: SanitizedMonitor[];
  unavailable?: string;
};

/** Runtime evidence only; monitor identity never participates in provider routing. */
export async function readNativeWindowEvidence(): Promise<NativeWindowEvidence> {
  try {
    return await invoke<NativeWindowEvidence>("native_window_monitor_evidence");
  } catch (error) {
    return { availableMonitors: [], unavailable: String(error) };
  }
}
