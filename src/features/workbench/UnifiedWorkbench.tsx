import {NativeWorkbench} from "./NativeWorkbench";
import type {ReactNode} from "react";
import type {RuntimePresentation, WorkbenchItem, WorkbenchLifecycle} from "./models";

export type WorkbenchSurface = "MANAGEMENT" | "BOARD" | "NOTIFICATIONS" | "SETTINGS" | "BRIDGES" | "NEW_BRIDGE" | "INBOX" | "WORKSPACE" | "DISCUSSION" | "PROJECT_HOME" | "PROJECT" | "NEW_WORK" | "GOAL" | "RUNTIME" | "PROVIDER_RUN_STATUS" | "EXIT_IMPACT" | "INSTALL" | "RECYCLE" | "PURGE_CONFIRM" | "TEST_CLEANUP" | "CODEX_HISTORY" | "CODEX_RESULTS" | "CODEX_ATTACHMENTS" | "CODEX_FEEDBACK" | "CODEX_REQUEST" | "NOTIFICATION_LOG";

export interface UnifiedWorkbenchProps {
  onRefreshConversationDirectory?:()=>Promise<unknown>;
  bridgeActivityApi?:import("./bridgeActivity").BridgeActivityApi;
  onManageBindings?:()=>void;
  onRenameBridge?:(id:string,name:string)=>Promise<void>;
  notificationSettings?:ReactNode;
  managementApi?:import("./managementZoneApi").ManagementApi;
  /** Watched-conversation projection shown in the unified conversation list. */
  watchApi?:import("./CodexNotifications").NotificationApi;
  notificationDetail?:boolean;
  devicePanel?:ReactNode;
  assistantPanel?:ReactNode;
  piSettingsPanel?:ReactNode;
  quotaPanel?:ReactNode;
  updatePanel?:ReactNode;
  updateAvailable?:boolean;
  /** Settings › 连接 › Codex 连接 (desktop). Falls back to runtimeAddon. */
  codexConnectionPanel?:ReactNode;
  /** Settings › 运行保护 (desktop). */
  runtimeProtectionPanel?:ReactNode;
  /** Settings › 概览 status reads over existing interfaces (desktop). */
  overviewApi?:import("./SettingsHome").OverviewApi;
  /** Phone settings computer card name (Transport T7). */
  computerIdentity?:()=>Promise<{hostName:string|null}>;
  onOpenWebAccess?:()=>void;
  onCreateBridge?: (name:string)=>Promise<void>;
  items: WorkbenchItem[];
  selectedWorkstreamId?: string | null;
  runtime?: RuntimePresentation | null;
  onSelectWorkstream: (workstreamId: string) => void;
  onPinChange?: (workstreamId: string, pinned: boolean) => void;
  onLifecycleChange?: (workstreamId: string, lifecycle: WorkbenchLifecycle) => void | Promise<void>;
  /** The host is the one navigation authority for every primary V3 surface. */
  surface?: WorkbenchSurface;
  onSurfaceChange?: (surface: WorkbenchSurface) => void;
  bridgePanel?: ReactNode;
  globalActions?: ReactNode;
  runtimeAddon?:ReactNode;
  criticalNotice?: ReactNode;
}

/** Shared responsive presentation. All effects and persistence remain in its host. */
export function UnifiedWorkbench(props: UnifiedWorkbenchProps) { return <NativeWorkbench {...props}/>; }
