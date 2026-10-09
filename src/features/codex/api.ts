import {readModels} from "../workbench/readModelCache";
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import type { AttachmentCandidate } from "../../domain/attachmentCandidate";
import type { BackendStatus, BoundChatGptProviderSurfaceStatus, ChatGptConversationHistory, ChatGptManualRefreshResult, ChatGptProviderSurfaceSnapshot, CodexFeedbackDraft, CodexGoal, CodexManualRefreshResult, CodexStructuredRequest, CodexTurnInterruptResult, CompletedChatGptResponse, DashboardProjection, DeliveryRecoveryCheck, EndpointPairingInput, EndpointPairingResult, EndpointPairingSideInput, ExistingCodexThreadCandidate, ExistingCodexThreadCatalog, ExplicitChatGptBindingCandidate, ExternalProjectLink, ExternalProjectLinkInput, FeedEvent, HandoffDraft, HandoffReviewSession, HostEnvironmentStatus, OutboundHandoffResult, ProviderRunStatus, ReadHistoryResult, ReplyObservation, ReverseHandoffDraft, ResumeResult, RolloverCandidate, RouterEndpoint, RouterProject, RouterWorkstream, ThreadSummary, TurnStartResult, UnprojectedThreadStart, VerifiedBackup, WorkspaceSnapshot, WorkstreamDraft, WorkstreamReviewResult } from "./types";


function invoke<T>(command:string,args?:Record<string,unknown>):Promise<T>{
 const key=command==='workspace_snapshot'?'workspace:desktop':command==='dashboard_projection'?'directory:desktop':command==='workstream_snapshot'?`workstream:${args?.workstreamId}`:null;
 if(key)return readModels.read(key,()=>tauriInvoke<T>(command,args));
 return tauriInvoke<T>(command,args).then(result=>{
  if(/^(create_bridge|rename_bridge|create_project|create_workstream|archive_workstream|set_workstream_pinned|trash_workstream|restore_workstream|purge_trashed_workstream|bind_workspace_endpoint|pair_workstream_endpoints|confirm_explicit_chatgpt|confirm_rollover|send_|approve_|acknowledge_)/.test(command)){
   readModels.invalidate('directory:');readModels.invalidate('workspace:');readModels.invalidate('workstream:');
   if(/^(trash_|purge_|bind_|pair_|confirm_explicit|confirm_rollover)/.test(command))readModels.invalidate('bridge:',true);
  }
  return result;
 });
}

export const codexApi = {
  createBridge: (name:string)=>invoke<string>("create_bridge_workstream",{name}),
  renameBridge:(workstreamId:string,name:string)=>invoke<void>("rename_bridge_workstream",{workstreamId,name}),
  nativeWriterAcceptanceAvailable: () => invoke<boolean>("native_writer_acceptance_available"),
  debugAcceptNativeChatGptWriter: (workstreamId: string, expectedEndpointId: string) => invoke<Record<string, unknown>>("debug_accept_native_chatgpt_writer", { workstreamId, expectedEndpointId }),
  workspaceSnapshot: () => invoke<WorkspaceSnapshot>("workspace_snapshot"),
  workstreamSnapshot: (workstreamId: string) => invoke<WorkspaceSnapshot>("workstream_snapshot", { workstreamId }),
  dashboardProjection: () => invoke<DashboardProjection>("dashboard_projection"),
  markProviderRunReviewed: (runId: string) => invoke<void>("mark_provider_run_reviewed", { runId }),
  acknowledgeHandoffAttention: (handoffId: string) => invoke<void>("acknowledge_handoff_attention", { handoffId }),
  createProject: (name: string, description?: string) => invoke<RouterProject>("create_project", { name, description: description || null }),
  createWorkstream: (projectId: string, name: string) => invoke<RouterWorkstream>("create_workstream", { projectId, name }),
  upsertExternalProjectLink: (projectId: string, input: ExternalProjectLinkInput) => invoke<ExternalProjectLink>("upsert_external_project_link", { projectId, input }),
  listExternalProjectLinks: (projectId: string) => invoke<ExternalProjectLink[]>("list_external_project_links", { projectId }),
  pairWorkstreamEndpoints: (workstreamId: string, input: EndpointPairingInput) => invoke<EndpointPairingResult>("pair_workstream_endpoints", { workstreamId, input }),
  archiveWorkstream: (workstreamId: string) => invoke<RouterWorkstream>("archive_workstream", { workstreamId }),
  setWorkstreamPinned: (workstreamId: string, pinned: boolean) => invoke<RouterWorkstream>("set_workstream_pinned", { workstreamId, pinned }),
  trashWorkstream: (workstreamId: string) => invoke<RouterWorkstream>("trash_workstream", { workstreamId }),
  restoreWorkstream: (workstreamId: string) => invoke<RouterWorkstream>("restore_workstream", { workstreamId }),
  purgeTrashedWorkstream: (workstreamId: string, expectedBindingRevision: number, confirmation: string) => invoke<void>("purge_trashed_workstream", { workstreamId, expectedBindingRevision, confirmation }),
  createVerifiedLocalBackup: () => invoke<VerifiedBackup>("create_verified_local_backup"),
  readWorkstreamDraft: (workstreamId: string) => invoke<WorkstreamDraft | null>("read_workstream_draft", { workstreamId }),
  saveWorkstreamDraft: (workstreamId: string, text: string, expectedRevision?: number | null) => invoke<WorkstreamDraft>("save_workstream_draft", { workstreamId, text, expectedRevision: expectedRevision ?? null }),
  readCodexFeedbackDraft: (workstreamId: string, sourceRunId: string) => invoke<CodexFeedbackDraft | null>("read_codex_feedback_draft", { workstreamId, sourceRunId }),
  saveCodexFeedbackDraft: (workstreamId: string, sourceRunId: string, text: string, expectedRevision?: number | null) => invoke<CodexFeedbackDraft>("save_codex_feedback_draft", { workstreamId, sourceRunId, text, expectedRevision: expectedRevision ?? null }),
  selectWorkspace: (projectId: string, workstreamId?: string) => invoke<WorkspaceSnapshot>("select_workspace", { projectId, workstreamId: workstreamId || null }),
  bindWorkspaceEndpoint: (workstreamId: string, provider: "CHATGPT" | "CODEX", externalId: string, label: string, replace: boolean) => invoke<RouterEndpoint>("bind_workspace_endpoint", { workstreamId, provider, externalId, label, replace }),
  prepareCurrentChatGptEndpointBinding: (workstreamId: string) => invoke<ExplicitChatGptBindingCandidate>("prepare_current_chatgpt_endpoint_binding", { workstreamId }),
  confirmChatGptAuthenticationCompleted: () => invoke<void>("confirm_chatgpt_authentication_completed"),
  prepareExplicitChatGptEndpointBinding: (workstreamId: string, input: string) => invoke<ExplicitChatGptBindingCandidate>("prepare_explicit_chatgpt_endpoint_binding", { workstreamId, input }),
  prepareOwnerConfirmedChatGptEndpointBinding: (workstreamId: string, input: string) => invoke<ExplicitChatGptBindingCandidate>("prepare_owner_confirmed_chatgpt_endpoint_binding", { workstreamId, input }),
  confirmExplicitChatGptEndpointBinding: (workstreamId: string) => invoke<RouterEndpoint>("confirm_explicit_chatgpt_endpoint_binding", { workstreamId }),
  confirmExplicitChatGptCodexPairing: (workstreamId: string, codex: EndpointPairingSideInput) => invoke<EndpointPairingResult>("confirm_explicit_chatgpt_codex_pairing", { workstreamId, codex }),
  beginCodexRollover: (workstreamId: string) => invoke<RolloverCandidate>("begin_codex_rollover", { workstreamId }),
  initializeCodexRollover: (workstreamId: string, text: string) => invoke<RolloverCandidate>("initialize_codex_rollover", { workstreamId, text }),
  verifyCodexRollover: (workstreamId: string) => invoke<RolloverCandidate>("verify_codex_rollover", { workstreamId }),
  confirmRollover: (workstreamId: string) => invoke<RouterEndpoint>("confirm_rollover", { workstreamId }),
  cancelRollover: (workstreamId: string) => invoke<void>("cancel_rollover", { workstreamId }),
  status: () => invoke<BackendStatus>("codex_status"),
  hostEnvironmentStatus: () => invoke<HostEnvironmentStatus>("host_environment_status"),
  connect: () => invoke<BackendStatus>("connect_codex"),
  listExistingCodexThreads: () => invoke<ExistingCodexThreadCatalog>("list_existing_codex_threads"),
  verifyExistingCodexThread: (threadId: string) => invoke<ExistingCodexThreadCandidate>("verify_existing_codex_thread", { threadId }),
  createThread: () => invoke<ThreadSummary>("create_thread"),
  startUnprojectedCodexThread: (directory?: string) => invoke<UnprojectedThreadStart>("start_unprojected_codex_thread", { directory: directory || null }),
  readThreadHistory: (threadId: string) => invoke<ReadHistoryResult>("read_thread_history", { threadId }),
  resumeThread: (threadId: string) => invoke<ResumeResult>("resume_thread", { threadId }),
  sendTurn: (threadId: string, text: string) => invoke<TurnStartResult>("send_turn", { threadId, text }),
  /** Direct D23 feedback is pinned to the active Router endpoint; it is not a Handoff. */
  sendWorkstreamCodexFeedback: (workstreamId: string, expectedEndpointId: string, resultRunId: string, text: string) => invoke<TurnStartResult>("send_workstream_codex_feedback", { workstreamId, expectedEndpointId, resultRunId, text }),
  readCodexGoal: (workstreamId: string) => invoke<CodexGoal | null>("read_codex_goal", { workstreamId }),
  pauseCodexGoal: (workstreamId: string) => invoke<CodexGoal>("pause_codex_goal", { workstreamId, confirmed: true }),
  resumeCodexGoal: (workstreamId: string) => invoke<CodexGoal>("resume_codex_goal", { workstreamId, confirmed: true }),
  clearCodexGoal: (workstreamId: string) => invoke<void>("clear_codex_goal", { workstreamId, confirmed: true }),
  interruptCodexTurn: (workstreamId: string, turnId: string) => invoke<CodexTurnInterruptResult>("interrupt_codex_turn", { workstreamId, turnId, confirmed: true }),
  /** The same exact Workstream and revision-checked Core path used by mobile. */
  codexRequests: (workstreamId: string) => invoke<CodexStructuredRequest[]>("codex_requests", { workstreamId }),
  respondToCodexRequest: (workstreamId: string, requestId: string, input: { revision: number; decision?: string; answers?: Record<string, string> }) => invoke<void>("respond_codex_request", { workstreamId, requestId, input }),
  reviewWorkstreamResults: (workstreamId: string) => invoke<WorkstreamReviewResult[]>("review_workstream_results", { workstreamId }),
  providerRunStatus: (workstreamId: string, runId: string) => invoke<ProviderRunStatus>("provider_run_status", { workstreamId, runId }),
  replyObservations: (workstreamId: string) => invoke<ReplyObservation[]>("reply_observations", { workstreamId }),
  /** Desktop-only one-time Chrome consent setup. It reads no tab or page. */
  connectNormalChromeReadOnly: () => invoke<void>("connect_normal_chrome_read_only"),
  /** One explicit, exact-endpoint read. It never sends a ChatGPT message. */
  checkNewChatGptReplies: (workstreamId: string) => invoke<ChatGptManualRefreshResult>("check_new_chatgpt_replies", { workstreamId }),
  /** One explicit, exact Codex existing-thread read. It never writes or resumes. */
  checkNewCodexReplies: (workstreamId: string) => invoke<CodexManualRefreshResult>("check_new_codex_replies", { workstreamId }),
  markReplyObservationRead: (workstreamId: string, observationId: string) => invoke<void>("mark_reply_observation_read", { workstreamId, observationId }),
  markReplyObservationHandled: (workstreamId: string, observationId: string) => invoke<void>("mark_reply_observation_handled", { workstreamId, observationId }),
  prepareChatGptToCodexHandoff: (workstreamId: string, responseId: string, initialMessage?: string, attachmentFilenames: string[] = []) => invoke<HandoffReviewSession>("prepare_chatgpt_to_codex_handoff", { workstreamId, responseId, initialMessage: initialMessage || null, attachmentFilenames }),
  selectChatGptToCodexHandoffAttachments: (actionId: string, revision: number, attachmentFilenames: string[]) => invoke<HandoffReviewSession>("select_chatgpt_to_codex_handoff_attachments", { actionId, revision, attachmentFilenames }),
  approveChatGptToCodexHandoff: (actionId: string, revision: number, message: string) => invoke<HandoffReviewSession>("approve_chatgpt_to_codex_handoff", { actionId, revision, message }),
  sendChatGptToCodexHandoffReview: (actionId: string, revision: number) => invoke<TurnStartResult>("send_chatgpt_to_codex_handoff_review", { actionId, revision }),
  prepareCodexToChatGptHandoff: (workstreamId: string, runId: string, attachmentIds: string[]) => invoke<HandoffReviewSession>("prepare_codex_to_chatgpt_handoff", { workstreamId, runId, attachmentIds }),
  approveCodexToChatGptHandoff: (actionId: string, revision: number, message: string) => invoke<HandoffReviewSession>("approve_codex_to_chatgpt_handoff", { actionId, revision, message }),
  sendCodexToChatGptHandoffReview: (actionId: string, revision: number) => invoke<OutboundHandoffResult>("send_codex_to_chatgpt_handoff", { actionId, revision }),
  detectAttachments: (sourceMessageId: string, text: string) =>
    invoke<AttachmentCandidate[]>("detect_attachments", { sourceMessageId, text }),
  copyPath: (path: string) => navigator.clipboard.writeText(path),
  boundChatGptProviderSurfaceStatus: (workstreamId: string) => invoke<BoundChatGptProviderSurfaceStatus>("bound_chatgpt_provider_surface_status", { workstreamId }),
  readActiveChatGptProviderLatestSnapshot: (workstreamId: string) => invoke<ChatGptProviderSurfaceSnapshot>("read_active_chatgpt_provider_latest_snapshot", { workstreamId }),
  openBoundChatGptConversation: (workstreamId: string) => invoke<void>("open_bound_chatgpt_conversation", { workstreamId }),
  openBoundChatGptInDefaultBrowser: (workstreamId: string) => invoke<import("./types").DefaultBrowserOpenResult>("open_bound_chatgpt_in_default_browser", { workstreamId }),
  openHostChatGptBrowserSetup: () => invoke<void>("open_host_chatgpt_browser_setup"),
  sendChatGptRequest: (workstreamId: string, message: string) => invoke<CompletedChatGptResponse>("send_chatgpt_request", { workstreamId, message }),
};

export function isFinalAgentEvent(event: FeedEvent): boolean {
  return event.kind === "AgentMessage" && event.method === "item/completed" && Boolean(event.text);
}
