import { StrictMode } from "react";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const composition=vi.hoisted(()=>({last:null as import("../features/workbench/UnifiedWorkbench").UnifiedWorkbenchProps|null}));
const api = vi.hoisted(() => ({
  createBridge:vi.fn(),
  nativeWriterAcceptanceAvailable: vi.fn(), debugAcceptNativeChatGptWriter: vi.fn(),
  prepareCurrentChatGptEndpointBinding: vi.fn(), confirmChatGptAuthenticationCompleted: vi.fn(), sendChatGptRequest: vi.fn(),
  workspaceSnapshot: vi.fn(), workstreamSnapshot: vi.fn(), dashboardProjection: vi.fn(), selectWorkspace: vi.fn(), readWorkstreamDraft: vi.fn(), saveWorkstreamDraft: vi.fn(), readCodexFeedbackDraft: vi.fn(), saveCodexFeedbackDraft: vi.fn(),
  reviewWorkstreamResults: vi.fn(), providerRunStatus: vi.fn(), status: vi.fn(), connect: vi.fn(), hostEnvironmentStatus: vi.fn(), openHostChatGptBrowserSetup: vi.fn(),
  listExternalProjectLinks: vi.fn(), upsertExternalProjectLink: vi.fn(), pairWorkstreamEndpoints: vi.fn(), prepareExplicitChatGptEndpointBinding: vi.fn(), prepareOwnerConfirmedChatGptEndpointBinding: vi.fn(), confirmExplicitChatGptEndpointBinding: vi.fn(), confirmExplicitChatGptCodexPairing: vi.fn(),
  readActiveChatGptProviderSurfaceSnapshot: vi.fn(), getChatGptProviderListenerStatus: vi.fn(), disableChatGptProviderSurfaceListener: vi.fn(), openBoundChatGptConversation: vi.fn(), openBoundChatGptInDefaultBrowser: vi.fn(),
  readChatGptProjectDirectorySnapshot: vi.fn(), readChatGptProjectDirectory: vi.fn(), startChatGptProjectNewChat: vi.fn(), prepareChatGptProjectNewChatBinding: vi.fn(),
  replyObservations: vi.fn(), connectNormalChromeReadOnly: vi.fn(), checkNewChatGptReplies: vi.fn(), markReplyObservationRead: vi.fn(), markReplyObservationHandled: vi.fn(), startUnprojectedCodexThread: vi.fn(), boundChatGptProviderSurfaceStatus: vi.fn(), readActiveChatGptProviderLatestSnapshot: vi.fn(), enableChatGptProviderSurfaceListener: vi.fn(), observeChatGptProviderSurfaceListener: vi.fn(), setChatGptProviderUnattendedMode: vi.fn(),
  readThreadHistory: vi.fn(), readCodexGoal: vi.fn(), codexRequests: vi.fn(), respondToCodexRequest: vi.fn(), sendWorkstreamCodexFeedback: vi.fn(), prepareChatGptToCodexHandoff: vi.fn(), approveChatGptToCodexHandoff: vi.fn(), sendChatGptToCodexHandoffReview: vi.fn(), prepareCodexToChatGptHandoff: vi.fn(), approveCodexToChatGptHandoff: vi.fn(), sendCodexToChatGptHandoffReview: vi.fn(), createProject: vi.fn(), createWorkstream: vi.fn(), listExistingCodexThreads: vi.fn(), verifyExistingCodexThread: vi.fn(),
}));
vi.mock("../features/codex/api", () => ({ codexApi: api }));
const eventHandlers = vi.hoisted(() => new Map<string, Set<(event: { payload: string }) => void>>());
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async (name: string, handler: (event: { payload: string }) => void) => {
  const handlers = eventHandlers.get(name) ?? new Set();
  handlers.add(handler);
  eventHandlers.set(name, handlers);
  return () => { handlers.delete(handler); };
}) }));
function emitSurface(workstreamId: string) {
  eventHandlers.get("chatgpt-provider-surface-observed")?.forEach((handler) => handler({ payload: workstreamId }));
}
import { UnifiedWorkbenchHost } from "./UnifiedWorkbenchHost";

const workstreams = [
  { id: "work-a", projectId: "project-1", name: "A 工作区", status: "ACTIVE", createdAt: 1, updatedAt: 1, bindingRevision: 3 },
  { id: "work-b", projectId: "project-1", name: "B 工作区", status: "ACTIVE", createdAt: 1, updatedAt: 1, bindingRevision: 4 },
];
const snapshot = { projects: [{ id: "project-1", name: "项目", createdAt: 1, updatedAt: 1 }], workstreams, selectedProjectId: "project-1", selectedWorkstreamId: "work-a", activeChatgptEndpoint: null, activeCodexEndpoint: null, endpointLineage: [], handoffs: [] };
const exactConfirmedSnapshot = {
  ...snapshot,
  activeChatgptEndpoint: { id: "endpoint-chatgpt-a", workstreamId: "work-a", provider: "CHATGPT" as const, externalId: "conversation-a", label: "ChatGPT conversation", status: "ACTIVE" as const, createdAt: 1 },
  activeCodexEndpoint: { id: "endpoint-codex-a", workstreamId: "work-a", provider: "CODEX" as const, externalId: "thread-codex-a", label: "当前 Codex 主对话", status: "ACTIVE" as const, createdAt: 1 },
};
const linkedChatGptProject = { id: "project-link-chatgpt", projectId: "project-1", provider: "CHATGPT" as const, externalProjectId: "g-p-router", canonicalUrl: "https://chatgpt.com/g/g-p-router/project", label: "Router 项目", sourceKind: "user-provided-canonical-url", createdAt: 1, updatedAt: 1 };


async function enterAdvancedConnection() {
  await waitFor(() => {
    const entry=screen.queryByRole("button",{name:"项目链接与高级连接 ›"});
    if(entry){fireEvent.click(entry);return;}
    if(screen.queryByLabelText("具体 ChatGPT 对话链接") || screen.queryByRole("heading",{name:"当前对话已绑定"})) return;
    throw new Error("Waiting for connection entry");
  });
}

beforeEach(() => {
  api.nativeWriterAcceptanceAvailable.mockResolvedValue(false);
  eventHandlers.clear();
  api.workspaceSnapshot.mockResolvedValue(snapshot);
  api.workstreamSnapshot.mockResolvedValue(snapshot);
  api.dashboardProjection.mockResolvedValue({ workstreams: [], attentionItems: [] });
  api.selectWorkspace.mockResolvedValue(snapshot);
  api.readWorkstreamDraft.mockResolvedValue(null);
  api.saveWorkstreamDraft.mockResolvedValue({ workstreamId: "work-a", text: "", revision: 1, updatedAt: 1 });
  api.readCodexFeedbackDraft.mockResolvedValue(null);
  api.saveCodexFeedbackDraft.mockResolvedValue({ workstreamId: "work-a", sourceRunId: "result-codex-a", text: "", revision: 1, updatedAt: 1 });
  api.reviewWorkstreamResults.mockResolvedValue([]);
  api.providerRunStatus.mockResolvedValue({ runId: "run-target", provider: "CODEX", status: "FAILED", terminalCode: "TEST_FAILURE", startedAt: 5, terminalAt: 9, updatedAt: 10, hasReviewableResult: false });
  api.status.mockResolvedValue({ connected: false });
  api.connect.mockResolvedValue({ connected: false, connecting: false, detail: "Codex app-server 未建立连接" });
  api.boundChatGptProviderSurfaceStatus.mockResolvedValue({ state: "NOT_OPEN", currentConversationId: null });
  api.getChatGptProviderListenerStatus.mockResolvedValue({ enabled: false, unattendedMode: false });
  api.disableChatGptProviderSurfaceListener.mockResolvedValue({ enabled: false, unattendedMode: false });
  api.openBoundChatGptConversation.mockResolvedValue(undefined);
  api.openBoundChatGptInDefaultBrowser.mockResolvedValue({ state: "OPEN_REQUESTED" });
  api.enableChatGptProviderSurfaceListener.mockResolvedValue({ enabled: true, unattendedMode: false });
  api.observeChatGptProviderSurfaceListener.mockResolvedValue(undefined);
  api.setChatGptProviderUnattendedMode.mockResolvedValue({ enabled: false, unattendedMode: false });
  api.readActiveChatGptProviderLatestSnapshot.mockResolvedValue({ href: "https://chatgpt.com/c/conversation-a", conversationId: "conversation-a", turns: [{ id: "history-assistant-a", role: "ASSISTANT", text: "当前对话的只读内容" }] });
  api.hostEnvironmentStatus.mockResolvedValue({ host: "OFFLINE", mobile: "UNAVAILABLE", browserRuntime: "OFFICIAL_NON_BRANDED_CHROMIUM", chatgptBrowserMode: "UNAVAILABLE" });
  api.openHostChatGptBrowserSetup.mockResolvedValue(undefined);
  api.listExternalProjectLinks.mockResolvedValue([]);
  api.readChatGptProjectDirectorySnapshot.mockResolvedValue({ projectUrl: "https://chatgpt.com/g/g-p-router/project", conversations: [], completeness: "EMPTY", hasMore: false, sourceKind: "PROJECT_CONTAINER" });
  api.readChatGptProjectDirectory.mockResolvedValue({ projectUrl: "https://chatgpt.com/g/g-p-router/project", conversations: [], completeness: "EMPTY", hasMore: false, sourceKind: "PROJECT_CONTAINER" });
  api.upsertExternalProjectLink.mockResolvedValue({ id: "link-1", provider: "CHATGPT", label: "saved" });
  api.prepareExplicitChatGptEndpointBinding.mockResolvedValue({ workstreamId: "work-a", externalId: "conversation-a", label: "ChatGPT conversation", verification: "PLAYWRIGHT_EXACT_ROUTE", expectedOldEndpointId: null, expectedBindingRevision: 3 });
  api.prepareOwnerConfirmedChatGptEndpointBinding.mockResolvedValue({ workstreamId: "work-a", externalId: "conversation-a", label: "用户已在默认浏览器核对的 ChatGPT 对话", verification: "OWNER_CONFIRMED_EXACT_URL", expectedOldEndpointId: null, expectedBindingRevision: 3 });
  api.confirmExplicitChatGptEndpointBinding.mockResolvedValue({ id: "endpoint-chatgpt-a", workstreamId: "work-a", provider: "CHATGPT", externalId: "conversation-a", label: "ChatGPT conversation", status: "ACTIVE", createdAt: 1 });
  api.confirmExplicitChatGptCodexPairing.mockResolvedValue({ bindingRevision: 4, chatgptEndpoint: { id: "endpoint-chatgpt-a", workstreamId: "work-a", provider: "CHATGPT", externalId: "conversation-a", label: "ChatGPT conversation", status: "ACTIVE", createdAt: 1 }, codexEndpoint: { id: "endpoint-codex-a", workstreamId: "work-a", provider: "CODEX", externalId: "thread-existing-a", label: "已有 Codex 对话", status: "ACTIVE", createdAt: 1 } });
  api.replyObservations.mockResolvedValue([]);
  api.connectNormalChromeReadOnly.mockResolvedValue(undefined);
  api.checkNewChatGptReplies.mockResolvedValue({ state: "NO_NEW_TERMINAL_REPLY", observationCreated: false });
  api.markReplyObservationRead.mockResolvedValue(undefined);
  api.markReplyObservationHandled.mockResolvedValue(undefined);
  api.startChatGptProjectNewChat.mockResolvedValue(undefined);
  api.prepareChatGptProjectNewChatBinding.mockResolvedValue({ workstreamId: "work-a", externalId: "conversation-new", label: "新对话候选", expectedOldEndpointId: null, expectedBindingRevision: 3 });
  api.startUnprojectedCodexThread.mockResolvedValue({ thread: { id: "thread-new" }, directory: "D:/router-scratch" });
  api.createWorkstream.mockResolvedValue({ id: "work-clean", projectId: "project-1", name: "干净验收工作区", status: "ACTIVE", createdAt: 2, updatedAt: 2, bindingRevision: 0 });
  api.createProject.mockResolvedValue({ id: "project-clean", name: "游戏开发", createdAt: 2, updatedAt: 2 });
  api.listExistingCodexThreads.mockResolvedValue({ complete: true, threads: [{ id: "thread-existing-a", label: "已有 Codex 对话", preview: "只读候选", updatedAt: "today", projectProvenance: null }] });
  api.verifyExistingCodexThread.mockResolvedValue({ id: "thread-existing-a", label: "已有 Codex 对话", preview: "只读候选", updatedAt: "today", projectProvenance: null });
  api.readThreadHistory.mockResolvedValue({ thread: { id: "thread-codex" }, history: [] });
  api.readCodexGoal.mockResolvedValue(null);
  api.codexRequests.mockResolvedValue([]);
  api.respondToCodexRequest.mockResolvedValue(undefined);
  api.sendWorkstreamCodexFeedback.mockResolvedValue({ turn_id: "turn-feedback" });
  api.prepareChatGptToCodexHandoff.mockResolvedValue({ actionId: "chatgpt-observation-review", revision: 1, status: "READY", message: "可编辑的 ChatGPT 转交" });
  api.approveChatGptToCodexHandoff.mockResolvedValue({ actionId: "chatgpt-observation-review", revision: 2, status: "APPROVED", message: "可编辑的 ChatGPT 转交" });
  api.sendChatGptToCodexHandoffReview.mockResolvedValue({ turn_id: "codex-turn-after-owner-send" });
  api.prepareCodexToChatGptHandoff.mockResolvedValue({ actionId: "codex-observation-review", revision: 1, status: "READY", message: "可编辑的 Codex 回传" });
  api.approveCodexToChatGptHandoff.mockResolvedValue({ actionId: "codex-observation-review", revision: 2, status: "APPROVED", message: "可编辑的 Codex 回传" });
  api.sendCodexToChatGptHandoffReview.mockResolvedValue({ status: "SENT", detail: "测试送达" });
  window.history.replaceState(null, "", "/");
});
afterEach(() => { cleanup(); vi.clearAllMocks(); vi.useRealTimers(); });

describe("UnifiedWorkbenchHost V3 project and pairing inputs", () => {
  it("distinguishes same-named Workstreams by readable binding and persists the exact selection", async () => {
    const oldWorkstream = { ...workstreams[0], name: "PRE-FLIGHT" };
    const requestedWorkstream = { ...workstreams[1], id: "work-requested", name: "PRE-FLIGHT" };
    const requestedSnapshot = {
      ...snapshot,
      workstreams: [oldWorkstream, requestedWorkstream],
      selectedWorkstreamId: "work-requested",
      activeChatgptEndpoint: { id: "endpoint-chatgpt-requested", workstreamId: "work-requested", provider: "CHATGPT" as const, externalId: "conversation-requested", label: "ChatGPT conversation", status: "ACTIVE" as const, createdAt: 1 },
      activeCodexEndpoint: { id: "endpoint-codex-requested", workstreamId: "work-requested", provider: "CODEX" as const, externalId: "thread-requested", label: "Complete Desktop Relay V0.1", status: "ACTIVE" as const, createdAt: 1 },
    };
    api.workspaceSnapshot.mockResolvedValue({ ...snapshot, workstreams: [oldWorkstream, requestedWorkstream], selectedWorkstreamId: "work-a" });
    api.workstreamSnapshot.mockImplementation(async (id: string) => id === "work-requested" ? requestedSnapshot : snapshot);
    api.dashboardProjection.mockResolvedValue({
      workstreams: [
        { projectId: "project-1", projectName: "项目", workstream: oldWorkstream, chatgptEndpoint: null, codexEndpoint: { id: "endpoint-old", workstreamId: "work-a", provider: "CODEX", externalId: "thread-old", label: "旧 Codex 候选", status: "ACTIVE", createdAt: 1 }, chatgptRun: null, codexRun: null, attentionItems: [], lastActivityAt: 1 },
        { projectId: "project-1", projectName: "项目", workstream: requestedWorkstream, chatgptEndpoint: requestedSnapshot.activeChatgptEndpoint, codexEndpoint: requestedSnapshot.activeCodexEndpoint, chatgptRun: null, codexRun: null, attentionItems: [], lastActivityAt: 2 },
      ],
      attentionItems: [],
    });
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    const requested = await screen.findByRole("button", { name: "PRE-FLIGHT · ChatGPT · ChatGPT conversation　/　Codex · Complete Desktop Relay V0.1" });
    fireEvent.click(requested);
    await waitFor(() => expect(api.selectWorkspace).toHaveBeenCalledWith("project-1", "work-requested"));
    expect(screen.getByRole("button", { name: "PRE-FLIGHT · ChatGPT · ChatGPT conversation　/　Codex · Complete Desktop Relay V0.1" })).toHaveAttribute("aria-current", "page");
  });

  it.each(["snapshot", "results", "history"])("keeps B presentation when A resolves late at the %s boundary", async (boundary) => {
    const workA = { ...exactConfirmedSnapshot, activeChatgptEndpoint: null };
    const workB = { ...workA, selectedWorkstreamId: "work-b", activeCodexEndpoint: { ...workA.activeCodexEndpoint, id: "endpoint-codex-b", workstreamId: "work-b", externalId: "thread-codex-b" } };
    let resolveLate!: (value: unknown) => void;
    let reached = false;
    const late = () => { reached = true; return new Promise((resolve) => { resolveLate = resolve; }); };
    const read = (id: string) => ({ thread: { id }, history: [{ id: `${id}-reply`, kind: "AgentMessage", text: id === "thread-codex-a" ? "Late A history" : "Current B history" }] });
    api.workspaceSnapshot.mockResolvedValue(workA);
    api.workstreamSnapshot.mockImplementation(async (id: string) => id === "work-a" ? boundary === "snapshot" ? late() : workA : workB);
    api.reviewWorkstreamResults.mockImplementation(async (id: string) => id === "work-a" && boundary === "results" ? late() : []);
    api.readThreadHistory.mockImplementation(async (id: string) => id === "thread-codex-a" && boundary === "history" ? late() : read(id));
    api.readCodexGoal.mockImplementation(async (id: string) => ({ threadId: id === "work-a" ? "thread-codex-a" : "thread-codex-b", objective: id === "work-a" ? "Late A goal" : "Current B goal", status: "active" }));
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    await waitFor(() => expect(reached).toBe(true));
    fireEvent.click(screen.getByRole("button", { name: "B 工作区" }));
    expect(await screen.findByText("Current B history")).toBeVisible();
    await act(async () => resolveLate(boundary === "snapshot" ? workA : boundary === "results" ? [{ runId: "late-a-result", provider: "CODEX", resultIdentity: "late-a", text: "Late A result", attachments: [] }] : read("thread-codex-a")));
    expect(screen.getByText("Current B history")).toBeVisible();
    expect(screen.queryByText("Late A history")).toBeNull();
    expect(screen.queryByText("Late A result")).toBeNull();
    expect(screen.queryByRole("button", { name: "Codex 结果与附件 1" })).toBeNull();
    expect(screen.getByRole("button", { name: "B 工作区" })).toHaveAttribute("aria-current", "page");
  });

  it("does not navigate or send to a provider under React StrictMode", async () => {
    api.workspaceSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    api.workstreamSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    render(<StrictMode><UnifiedWorkbenchHost initialSurface="WORKSPACE" /></StrictMode>);
    expect(api.readActiveChatGptProviderLatestSnapshot).not.toHaveBeenCalled();
    fireEvent.click(await screen.findByRole("button", { name: "运行环境" }));
    expect(await screen.findByText("AI Work Router Browser")).toBeVisible();
    expect(screen.queryByRole("button", { name: "重新读取当前对话" })).toBeNull();
    expect(screen.queryByRole("button", { name: "检查新回复" })).toBeNull();
  });

  it("renders D14 as an official one-time request and submits only its exact revision", async () => {
    const codexSnapshot = {
      ...snapshot,
      activeCodexEndpoint: { id: "endpoint-codex-a", workstreamId: "work-a", provider: "CODEX" as const, externalId: "thread-codex-a", label: "实施主对话", status: "ACTIVE" as const, createdAt: 1 },
    };
    api.workspaceSnapshot.mockResolvedValue(codexSnapshot);
    api.workstreamSnapshot.mockResolvedValue(codexSnapshot);
    api.codexRequests.mockResolvedValue([{ requestId: "opaque-request-1", revision: 9, method: "item/commandExecution/requestApproval", kind: "COMMAND_APPROVAL", reason: "输出一个本地测试标记。", choices: [{ id: "accept", label: "Allow once" }, { id: "decline", label: "Decline" }], questions: [], isBlocking: true, responseSent: false }]);
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "Codex 授权请求" }));
    expect(await screen.findByRole("heading", { name: "允许执行这一次命令？" })).toBeVisible();
    expect(screen.getByText(/输出一个本地测试标记/)).toBeVisible();
    expect(screen.getByText("Router 当前未收到可显示的原始命令。")).toBeVisible();
    expect(screen.queryByText("opaque-request-1")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "允许这一次" }));
    await waitFor(() => expect(api.respondToCodexRequest).toHaveBeenCalledWith("work-a", "opaque-request-1", { revision: 9, decision: "accept" }));
    expect(await screen.findByRole("heading", { name: "已允许这一次" })).toBeVisible();
  });

  it("locks a pending D14 response so opposite decisions cannot race", async () => {
    const codexSnapshot = { ...snapshot, activeCodexEndpoint: { id: "endpoint-codex-a", workstreamId: "work-a", provider: "CODEX" as const, externalId: "thread-codex-a", label: "实施主对话", status: "ACTIVE" as const, createdAt: 1 } };
    api.workspaceSnapshot.mockResolvedValue(codexSnapshot);
    api.workstreamSnapshot.mockResolvedValue(codexSnapshot);
    api.codexRequests.mockResolvedValue([{ requestId: "opaque-request-1", revision: 9, method: "item/commandExecution/requestApproval", kind: "COMMAND_APPROVAL", reason: "一次性选择", choices: [{ id: "accept", label: "Allow once" }, { id: "decline", label: "Decline" }], questions: [], isBlocking: true, responseSent: false }]);
    let resolveResponse: () => void = () => undefined;
    api.respondToCodexRequest.mockImplementationOnce(() => new Promise<void>((resolve) => { resolveResponse = resolve; }));
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "Codex 授权请求" }));
    fireEvent.click(screen.getByRole("button", { name: "允许这一次" }));
    fireEvent.click(screen.getByRole("button", { name: "拒绝" }));
    expect(api.respondToCodexRequest).toHaveBeenCalledTimes(1);
    expect(api.respondToCodexRequest).toHaveBeenCalledWith("work-a", "opaque-request-1", { revision: 9, decision: "accept" });
    resolveResponse();
    expect(await screen.findByRole("heading", { name: "已允许这一次" })).toBeVisible();
  });

  it("opens D23 as a direct exact-thread feedback draft and sends only after the user action", async () => {
    const codexSnapshot = {
      ...snapshot,
      activeCodexEndpoint: { id: "endpoint-codex-a", workstreamId: "work-a", provider: "CODEX" as const, externalId: "thread-codex-a", label: "实施主对话", status: "ACTIVE" as const, createdAt: 1 },
    };
    api.workspaceSnapshot.mockResolvedValue(codexSnapshot);
    api.workstreamSnapshot.mockResolvedValue(codexSnapshot);
    api.reviewWorkstreamResults.mockResolvedValue([{ runId: "result-codex-a", provider: "CODEX", resultIdentity: "Codex 完整结果", text: "需要修改的完整结果", attachments: [] }]);
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "Codex 结果与附件 1" }));
    fireEvent.click(await screen.findByRole("button", { name: "要求 Codex 修改" }));
    const draft = await screen.findByLabelText("修改意见");
    fireEvent.change(draft, { target: { value: "请补充边界情况验证。" } });
    expect(api.sendWorkstreamCodexFeedback).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "发送修改意见" }));
    await waitFor(() => expect(api.sendWorkstreamCodexFeedback).toHaveBeenCalledWith("work-a", "endpoint-codex-a", "result-codex-a", "请补充边界情况验证。"));
  });

  it("locks a pending D23 send against double click and retains the exact draft after failure", async () => {
    const codexSnapshot = { ...snapshot, activeCodexEndpoint: { id: "endpoint-codex-a", workstreamId: "work-a", provider: "CODEX" as const, externalId: "thread-codex-a", label: "实施主对话", status: "ACTIVE" as const, createdAt: 1 } };
    api.workspaceSnapshot.mockResolvedValue(codexSnapshot);
    api.workstreamSnapshot.mockResolvedValue(codexSnapshot);
    api.reviewWorkstreamResults.mockResolvedValue([{ runId: "result-codex-a", provider: "CODEX", resultIdentity: "Codex 完整结果", text: "需要修改的完整结果", attachments: [] }]);
    let rejectSend: (reason?: unknown) => void = () => undefined;
    api.sendWorkstreamCodexFeedback.mockImplementationOnce(() => new Promise((_resolve, reject) => { rejectSend = reject; }));
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "Codex 结果与附件 1" }));
    fireEvent.click(await screen.findByRole("button", { name: "要求 Codex 修改" }));
    fireEvent.change(await screen.findByLabelText("修改意见"), { target: { value: "保留这条精确修改草稿。" } });
    const send = screen.getByRole("button", { name: "发送修改意见" });
    fireEvent.click(send);
    fireEvent.click(send);
    expect(api.sendWorkstreamCodexFeedback).toHaveBeenCalledTimes(1);
    expect(send).toBeDisabled();
    rejectSend(new Error("exact thread unavailable"));
    await waitFor(() => expect(send).toBeEnabled());
    expect(screen.getByLabelText("修改意见")).toHaveValue("保留这条精确修改草稿。");
  });

  it("keeps D23 drafts isolated by exact result run and preserves them on cancel", async () => {
    const codexSnapshot = {
      ...snapshot,
      activeCodexEndpoint: { id: "endpoint-codex-a", workstreamId: "work-a", provider: "CODEX" as const, externalId: "thread-codex-a", label: "实施主对话", status: "ACTIVE" as const, createdAt: 1 },
    };
    api.workspaceSnapshot.mockResolvedValue(codexSnapshot);
    api.workstreamSnapshot.mockResolvedValue(codexSnapshot);
    api.reviewWorkstreamResults.mockResolvedValue([
      { runId: "result-codex-a", provider: "CODEX", resultIdentity: "结果 A", text: "A", attachments: [] },
      { runId: "result-codex-b", provider: "CODEX", resultIdentity: "结果 B", text: "B", attachments: [] },
    ]);
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "Codex 结果与附件 2" }));
    fireEvent.click(screen.getAllByRole("button", { name: "要求 Codex 修改" })[0]);
    fireEvent.change(await screen.findByLabelText("修改意见"), { target: { value: "只属于结果 A 的草稿" } });
    fireEvent.click(screen.getByRole("button", { name: "取消，保留草稿" }));
    fireEvent.click(screen.getAllByRole("button", { name: "要求 Codex 修改" })[1]);
    expect(await screen.findByLabelText("修改意见")).toHaveValue("");
    fireEvent.click(screen.getByRole("button", { name: "取消，保留草稿" }));
    fireEvent.click(screen.getAllByRole("button", { name: "要求 Codex 修改" })[0]);
    expect(await screen.findByLabelText("修改意见")).toHaveValue("只属于结果 A 的草稿");
    fireEvent.click(screen.getByRole("button", { name: "发送修改意见" }));
    await waitFor(() => expect(api.sendWorkstreamCodexFeedback).toHaveBeenCalledWith("work-a", "endpoint-codex-a", "result-codex-a", "只属于结果 A 的草稿"));
  });

  it("autosaves a D23 draft through its exact persisted result scope", async () => {
    const codexSnapshot = {
      ...snapshot,
      activeCodexEndpoint: { id: "endpoint-codex-a", workstreamId: "work-a", provider: "CODEX" as const, externalId: "thread-codex-a", label: "实施主对话", status: "ACTIVE" as const, createdAt: 1 },
    };
    api.workspaceSnapshot.mockResolvedValue(codexSnapshot);
    api.workstreamSnapshot.mockResolvedValue(codexSnapshot);
    api.reviewWorkstreamResults.mockResolvedValue([{ runId: "result-codex-a", provider: "CODEX", resultIdentity: "Codex 完整结果", text: "需要修改的完整结果", attachments: [] }]);
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "Codex 结果与附件 1" }));
    fireEvent.click(screen.getByRole("button", { name: "要求 Codex 修改" }));
    const textarea = await screen.findByLabelText("修改意见");
    vi.useFakeTimers();
    fireEvent.change(textarea, { target: { value: "刷新后也要保留" } });
    await act(async () => { await vi.advanceTimersByTimeAsync(500); });
    expect(api.saveCodexFeedbackDraft).toHaveBeenCalledWith("work-a", "result-codex-a", "刷新后也要保留", null);
  });

  it("restores a persisted D23 draft only for the opened exact result", async () => {
    const codexSnapshot = {
      ...snapshot,
      activeCodexEndpoint: { id: "endpoint-codex-a", workstreamId: "work-a", provider: "CODEX" as const, externalId: "thread-codex-a", label: "实施主对话", status: "ACTIVE" as const, createdAt: 1 },
    };
    api.workspaceSnapshot.mockResolvedValue(codexSnapshot);
    api.workstreamSnapshot.mockResolvedValue(codexSnapshot);
    api.reviewWorkstreamResults.mockResolvedValue([{ runId: "result-codex-a", provider: "CODEX", resultIdentity: "Codex 完整结果", text: "需要修改的完整结果", attachments: [] }]);
    api.readCodexFeedbackDraft.mockResolvedValue({ workstreamId: "work-a", sourceRunId: "result-codex-a", text: "已从 Router 草稿恢复", revision: 4, updatedAt: 10 });
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "Codex 结果与附件 1" }));
    fireEvent.click(screen.getByRole("button", { name: "要求 Codex 修改" }));
    await waitFor(() => expect(screen.getByLabelText("修改意见")).toHaveValue("已从 Router 草稿恢复"));
    expect(api.readCodexFeedbackDraft).toHaveBeenCalledWith("work-a", "result-codex-a");
  });

  it("clears A pairing input before B can submit it", async () => {
    api.listExternalProjectLinks.mockResolvedValue([linkedChatGptProject]);
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "项目" }));
    await screen.findByText("ChatGPT 项目链接已保存");
    fireEvent.click(screen.getByRole("button", { name: "选择对话，开始工作" }));
    await enterAdvancedConnection();
    await enterAdvancedConnection();
    await screen.findByRole("heading", { name: "选择两端对话" });
    const surface = screen.getByRole("region", { name: "项目与对话连接" });
    const chatgpt = within(surface).getByLabelText("具体 ChatGPT 对话链接");
    fireEvent.change(chatgpt, { target: { value: "conversation-a" } });
    fireEvent.click(screen.getByRole("button", { name: /B 工作区/ }));
    fireEvent.click(screen.getByRole("button", { name: "项目" }));
    await screen.findByText("ChatGPT 项目链接已保存");
    fireEvent.click(screen.getByRole("button", { name: "选择对话，开始工作" }));
    await enterAdvancedConnection();
    const reselectedSurface = await screen.findByRole("region", { name: "项目与对话连接" });
    await waitFor(() => expect(within(reselectedSurface).getByLabelText("具体 ChatGPT 对话链接")).toHaveValue(""));
    expect(within(reselectedSurface).getByRole("button", { name: "核对这组配对" })).toBeDisabled();
    expect(api.pairWorkstreamEndpoints).not.toHaveBeenCalled();
  });

  it("uses the Figma project-selection surface before revealing exact-ID detail", async () => {
    api.listExternalProjectLinks.mockResolvedValue([linkedChatGptProject]);
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "项目" }));
    await screen.findByText("ChatGPT 项目链接已保存");
    fireEvent.click(screen.getByRole("button", { name: "选择对话，开始工作" }));
    await enterAdvancedConnection();
    await enterAdvancedConnection();
    expect(await screen.findByRole("heading", { name: "选择两端对话" })).toBeVisible();
    const surface = screen.getByRole("region", { name: "项目与对话连接" });
    expect(within(surface).getByLabelText("具体 ChatGPT 对话链接")).toBeVisible();
    expect(within(surface).queryByLabelText("ChatGPT conversation ID")).toBeNull();
  });

  it("makes current-workstream ChatGPT replacement a direct project-home action", async () => {
    api.workspaceSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    api.workstreamSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "项目" }));
    const action = await screen.findByRole("button", { name: "更换当前 A 工作区 的 ChatGPT 对话" });
    fireEvent.click(action);
    expect(await screen.findByRole("heading", { name: "更换当前 ChatGPT 对话" })).toBeVisible();
    expect(screen.getByText("当前工作：A 工作区")).toBeVisible();
    expect(screen.getByText(/项目关联和已有 Codex 对话均不会改动/)).toBeVisible();
    expect(screen.getByLabelText("具体 ChatGPT 对话链接")).toBeVisible();
    expect(screen.queryByRole("button", { name: "绑定已有 Codex 对话" })).toBeNull();
  });

  it("keeps explicit Project-scoped ChatGPT URL binding session-only until the user confirms", async () => {
    api.listExternalProjectLinks.mockResolvedValue([linkedChatGptProject]);
    api.prepareExplicitChatGptEndpointBinding.mockResolvedValue({ workstreamId: "work-a", externalId: "conversation-project-a", label: "已验证对话", verification: "PLAYWRIGHT_EXACT_ROUTE", expectedOldEndpointId: "endpoint-old", expectedBindingRevision: 3 });
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "项目" }));
    await screen.findByText("ChatGPT 项目链接已保存");
    fireEvent.click(screen.getByRole("button", { name: "选择对话，开始工作" }));
    await enterAdvancedConnection();
    const surface = await screen.findByRole("region", { name: "项目与对话连接" });
    fireEvent.change(within(surface).getByLabelText("具体 ChatGPT 对话链接"), { target: { value: "https://chatgpt.com/g/g-p-router/c/conversation-project-a" } });
    fireEvent.click(within(surface).getByRole("button", { name: "我已在默认浏览器核对，准备绑定" }));
    await waitFor(() => expect(api.prepareOwnerConfirmedChatGptEndpointBinding).toHaveBeenCalledWith("work-a", "https://chatgpt.com/g/g-p-router/c/conversation-project-a"));
    expect(api.confirmExplicitChatGptEndpointBinding).not.toHaveBeenCalled();
    expect(within(surface).getByText("确认前不会改变当前绑定。")).toBeVisible();
    fireEvent.click(within(surface).getByRole("button", { name: "确认绑定" }));
    await waitFor(() => expect(api.confirmExplicitChatGptEndpointBinding).toHaveBeenCalledWith("work-a"));
  });

  it("prepares an owner-confirmed default-browser URL without calling the dedicated carrier, then requires confirmation", async () => {
    api.prepareOwnerConfirmedChatGptEndpointBinding.mockResolvedValue({ workstreamId: "work-a", externalId: "6ab86566-a260-83ec-a603-a930845e22c6", label: "用户已在默认浏览器核对的 ChatGPT 对话", verification: "OWNER_CONFIRMED_EXACT_URL", expectedOldEndpointId: "endpoint-old", expectedBindingRevision: 3 });
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "项目" }));
    fireEvent.click(await screen.findByRole("button", { name: "选择对话，开始工作" }));
    await enterAdvancedConnection();
    const surface = await screen.findByRole("region", { name: "项目与对话连接" });
    fireEvent.change(within(surface).getByLabelText("具体 ChatGPT 对话链接"), { target: { value: "https://chatgpt.com/g/g-p-6a8da71a6ca08191b221d6751e030a54-ai-work-router/c/6ab86566-a260-83ec-a603-a930845e22c6" } });
    fireEvent.click(within(surface).getByRole("button", { name: "我已在默认浏览器核对，准备绑定" }));
    await waitFor(() => expect(api.prepareOwnerConfirmedChatGptEndpointBinding).toHaveBeenCalledWith("work-a", "https://chatgpt.com/g/g-p-6a8da71a6ca08191b221d6751e030a54-ai-work-router/c/6ab86566-a260-83ec-a603-a930845e22c6"));
    expect(api.prepareExplicitChatGptEndpointBinding).not.toHaveBeenCalled();
    expect(within(surface).getByText("你已人工核对的具体 ChatGPT 对话")).toBeVisible();
    expect(within(surface).getByText("6ab86566-a260-83ec-a603-a930845e22c6")).toBeVisible();
    fireEvent.click(within(surface).getByRole("button", { name: "确认绑定" }));
    await waitFor(() => expect(api.confirmExplicitChatGptEndpointBinding).toHaveBeenCalledWith("work-a"));
  });

  it("atomically saves fresh exact ChatGPT and existing Codex candidates only after one Pair Review", async () => {
    api.listExternalProjectLinks.mockResolvedValue([linkedChatGptProject]);
    api.status.mockResolvedValue({ connected: true });
    api.prepareOwnerConfirmedChatGptEndpointBinding.mockResolvedValue({ workstreamId: "work-a", externalId: "conversation-clean", label: "已验证 ChatGPT", verification: "OWNER_CONFIRMED_EXACT_URL", expectedOldEndpointId: null, expectedBindingRevision: 3 });
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "项目" }));
    fireEvent.click(await screen.findByRole("button", { name: "选择对话，开始工作" }));
    await enterAdvancedConnection();
    const surface = await screen.findByRole("region", { name: "项目与对话连接" });
    fireEvent.click(within(surface).getByRole("button", { name: "绑定已有 Codex 对话" }));
    fireEvent.change(await screen.findByLabelText("绑定已有 Codex 对话时间范围"), {target:{value:"ALL"}});
    const thread = await screen.findByRole("button", { name: /已有 Codex 对话/ });
    fireEvent.click(thread);
    expect(await screen.findByRole("heading", { name: "确认这一组对话" })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "返回选择" }));
    const conversations = await screen.findByRole("region", { name: "项目与对话连接" });
    fireEvent.change(within(conversations).getByLabelText("具体 ChatGPT 对话链接"), { target: { value: "https://chatgpt.com/c/conversation-clean" } });
    fireEvent.click(within(conversations).getByRole("button", { name: "我已在默认浏览器核对，准备绑定" }));
    const review = await within(conversations).findByRole("button", { name: "进入配对复核" });
    expect(api.confirmExplicitChatGptEndpointBinding).not.toHaveBeenCalled();
    fireEvent.click(review);
    expect(await screen.findByText("原子保存 ChatGPT 与 Codex 精确配对")).toBeVisible();
    expect(screen.getByText("已验证 ChatGPT")).toBeVisible();
    expect(screen.getByText("已有 Codex 对话")).toBeVisible();
    const pairedSnapshot = { ...snapshot, activeChatgptEndpoint: { id: "endpoint-chat-clean", workstreamId: "work-a", provider: "CHATGPT" as const, externalId: "conversation-clean", label: "已验证 ChatGPT", status: "ACTIVE" as const, createdAt: 2 }, activeCodexEndpoint: { id: "endpoint-codex-existing", workstreamId: "work-a", provider: "CODEX" as const, externalId: "thread-existing-a", label: "已有 Codex 对话", status: "ACTIVE" as const, createdAt: 2 } };
    api.workstreamSnapshot.mockResolvedValue(pairedSnapshot);
    api.confirmExplicitChatGptCodexPairing.mockResolvedValue({ bindingRevision: 4, chatgptEndpoint: pairedSnapshot.activeChatgptEndpoint, codexEndpoint: pairedSnapshot.activeCodexEndpoint });
    const dashboardReadsBeforePair = api.dashboardProjection.mock.calls.length;
    api.dashboardProjection.mockResolvedValue({ workstreams: [{
      projectId: "project-1", projectName: "项目", workstream: workstreams[0],
      chatgptEndpoint: pairedSnapshot.activeChatgptEndpoint,
      codexEndpoint: pairedSnapshot.activeCodexEndpoint,
      chatgptRun: null, codexRun: null, attentionItems: [], lastActivityAt: 2,
    }], attentionItems: [] });
    fireEvent.click(screen.getByRole("button", { name: "保存并进入工作" }));
    await waitFor(() => expect(api.confirmExplicitChatGptCodexPairing).toHaveBeenCalledWith("work-a", { expectedActiveEndpointId: null, externalId: "thread-existing-a", label: "已有 Codex 对话" }));
    expect(api.pairWorkstreamEndpoints).not.toHaveBeenCalled();
    expect(api.confirmExplicitChatGptEndpointBinding).not.toHaveBeenCalled();
    await waitFor(() => expect(api.dashboardProjection.mock.calls.length).toBeGreaterThan(dashboardReadsBeforePair));
    expect(await screen.findByRole("button", { name: "A 工作区 · ChatGPT · 已验证 ChatGPT　/　Codex · 已有 Codex 对话" })).toBeVisible();
    expect(api.openHostChatGptBrowserSetup).not.toHaveBeenCalled();
    expect(api.confirmChatGptAuthenticationCompleted).not.toHaveBeenCalled();
  });

  it("shows immediate inline confirmation progress and blocks a duplicate explicit binding click", async () => {
    api.listExternalProjectLinks.mockResolvedValue([linkedChatGptProject]);
    let resolveConfirm: () => void = () => undefined;
    api.confirmExplicitChatGptEndpointBinding.mockImplementationOnce(() => new Promise<void>((resolve) => { resolveConfirm = resolve; }));
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "项目" }));
    fireEvent.click(await screen.findByRole("button", { name: "选择对话，开始工作" }));
    await enterAdvancedConnection();
    const surface = await screen.findByRole("region", { name: "项目与对话连接" });
    fireEvent.change(within(surface).getByLabelText("具体 ChatGPT 对话链接"), { target: { value: "https://chatgpt.com/c/conversation-a" } });
    fireEvent.click(within(surface).getByRole("button", { name: "我已在默认浏览器核对，准备绑定" }));
    await screen.findByRole("button", { name: "确认绑定" });
    const confirm = within(surface).getByRole("button", { name: "确认绑定" });
    fireEvent.click(confirm);
    fireEvent.click(confirm);
    expect(api.confirmExplicitChatGptEndpointBinding).toHaveBeenCalledTimes(1);
    expect(await within(surface).findByRole("status")).toHaveTextContent("正在确认绑定");
    expect(within(surface).getByRole("button", { name: "正在确认绑定…" })).toBeDisabled();
    resolveConfirm();
  });

  it("keeps a failed explicit candidate visible and puts its safe error inside the candidate card", async () => {
    api.listExternalProjectLinks.mockResolvedValue([linkedChatGptProject]);
    api.confirmExplicitChatGptEndpointBinding.mockRejectedValueOnce(new Error("No validated ChatGPT binding is awaiting confirmation"));
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "项目" }));
    fireEvent.click(await screen.findByRole("button", { name: "选择对话，开始工作" }));
    await enterAdvancedConnection();
    const surface = await screen.findByRole("region", { name: "项目与对话连接" });
    fireEvent.change(within(surface).getByLabelText("具体 ChatGPT 对话链接"), { target: { value: "https://chatgpt.com/c/conversation-a" } });
    fireEvent.click(within(surface).getByRole("button", { name: "我已在默认浏览器核对，准备绑定" }));
    fireEvent.click(await within(surface).findByRole("button", { name: "确认绑定" }));
    const candidate = await within(surface).findByText("用户已在默认浏览器核对的 ChatGPT 对话");
    const candidateCard = candidate.closest("article");
    expect(candidateCard).not.toBeNull();
    expect(within(candidateCard!).getByRole("alert")).toHaveTextContent("验证候选已不在当前 Router 会话中");
    expect(within(candidateCard!).getByRole("button", { name: "确认绑定" })).toBeEnabled();
    expect(screen.queryByText(/绑定未保存：/)).toBeNull();
  });

  it("refreshes the exact Workstream after successful explicit confirmation", async () => {
    api.listExternalProjectLinks.mockResolvedValue([linkedChatGptProject]);
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "项目" }));
    fireEvent.click(await screen.findByRole("button", { name: "选择对话，开始工作" }));
    await enterAdvancedConnection();
    const surface = await screen.findByRole("region", { name: "项目与对话连接" });
    fireEvent.change(within(surface).getByLabelText("具体 ChatGPT 对话链接"), { target: { value: "https://chatgpt.com/c/conversation-a" } });
    fireEvent.click(within(surface).getByRole("button", { name: "我已在默认浏览器核对，准备绑定" }));
    const snapshotCalls = api.workstreamSnapshot.mock.calls.length;
    fireEvent.click(await within(surface).findByRole("button", { name: "确认绑定" }));
    await waitFor(() => expect(api.confirmExplicitChatGptEndpointBinding).toHaveBeenCalledWith("work-a"));
    await waitFor(() => expect(api.workstreamSnapshot.mock.calls.length).toBeGreaterThan(snapshotCalls));
    expect(api.pairWorkstreamEndpoints).not.toHaveBeenCalled();
  });

  it("keeps an authoritative confirmation successful when the exact snapshot refresh fails", async () => {
    api.listExternalProjectLinks.mockResolvedValue([linkedChatGptProject]);
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "项目" }));
    fireEvent.click(await screen.findByRole("button", { name: "选择对话，开始工作" }));
    await enterAdvancedConnection();
    const surface = await screen.findByRole("region", { name: "项目与对话连接" });
    fireEvent.change(within(surface).getByLabelText("具体 ChatGPT 对话链接"), { target: { value: "https://chatgpt.com/c/conversation-a" } });
    fireEvent.click(within(surface).getByRole("button", { name: "我已在默认浏览器核对，准备绑定" }));
    api.workstreamSnapshot.mockRejectedValueOnce(new Error("snapshot unavailable"));
    fireEvent.click(await within(surface).findByRole("button", { name: "确认绑定" }));
    const candidate = (await within(surface).findByText("用户已在默认浏览器核对的 ChatGPT 对话")).closest("article");
    expect(candidate).not.toBeNull();
    expect(within(candidate!).getByRole("status")).toHaveTextContent("ChatGPT 对话已经绑定，但工作区状态刷新失败");
    expect(within(candidate!).getByRole("button", { name: "ChatGPT 对话已绑定" })).toBeDisabled();
    expect(within(candidate!).queryByText("绑定没有保存。当前绑定没有改变。")).toBeNull();
    expect(api.confirmExplicitChatGptEndpointBinding).toHaveBeenCalledTimes(1);
  });

  it("does not make an explicit ChatGPT confirmation depend on Codex history or Goal reads", async () => {
    api.listExternalProjectLinks.mockResolvedValue([linkedChatGptProject]);
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "项目" }));
    fireEvent.click(await screen.findByRole("button", { name: "选择对话，开始工作" }));
    await enterAdvancedConnection();
    const surface = await screen.findByRole("region", { name: "项目与对话连接" });
    fireEvent.change(within(surface).getByLabelText("具体 ChatGPT 对话链接"), { target: { value: "https://chatgpt.com/c/conversation-a" } });
    fireEvent.click(within(surface).getByRole("button", { name: "我已在默认浏览器核对，准备绑定" }));
    const historyCalls = api.readThreadHistory.mock.calls.length;
    const goalCalls = api.readCodexGoal.mock.calls.length;
    api.workstreamSnapshot.mockResolvedValueOnce(exactConfirmedSnapshot);
    api.readThreadHistory.mockRejectedValueOnce(new Error("Codex history unavailable"));
    api.readCodexGoal.mockRejectedValueOnce(new Error("Codex Goal unavailable"));
    fireEvent.click(await within(surface).findByRole("button", { name: "确认绑定" }));
    await waitFor(() => expect(screen.queryByRole("region", { name: "项目与对话连接" })).toBeNull());
    expect(api.readThreadHistory).toHaveBeenCalledTimes(historyCalls);
    expect(api.readCodexGoal).toHaveBeenCalledTimes(goalCalls);
    expect(screen.queryByText("绑定没有保存。当前绑定没有改变。")).toBeNull();
  });

  it("treats a malformed confirmation response as a success warning rather than an unsaved bind", async () => {
    api.listExternalProjectLinks.mockResolvedValue([linkedChatGptProject]);
    api.confirmExplicitChatGptEndpointBinding.mockResolvedValueOnce({ ...exactConfirmedSnapshot.activeChatgptEndpoint, externalId: "conversation-other" });
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "项目" }));
    fireEvent.click(await screen.findByRole("button", { name: "选择对话，开始工作" }));
    await enterAdvancedConnection();
    const surface = await screen.findByRole("region", { name: "项目与对话连接" });
    fireEvent.change(within(surface).getByLabelText("具体 ChatGPT 对话链接"), { target: { value: "https://chatgpt.com/c/conversation-a" } });
    fireEvent.click(within(surface).getByRole("button", { name: "我已在默认浏览器核对，准备绑定" }));
    const snapshotCalls = api.workstreamSnapshot.mock.calls.length;
    fireEvent.click(await within(surface).findByRole("button", { name: "确认绑定" }));
    const candidate = (await within(surface).findByText("用户已在默认浏览器核对的 ChatGPT 对话")).closest("article");
    expect(within(candidate!).getByRole("status")).toHaveTextContent("返回的 Endpoint 未得到一致确认");
    expect(within(candidate!).getByRole("button", { name: "ChatGPT 对话已绑定" })).toBeDisabled();
    expect(api.workstreamSnapshot.mock.calls.length).toBe(snapshotCalls);
  });

  it("warns on a refreshed ACTIVE Endpoint mismatch without reclassifying the transaction", async () => {
    api.listExternalProjectLinks.mockResolvedValue([linkedChatGptProject]);
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "项目" }));
    fireEvent.click(await screen.findByRole("button", { name: "选择对话，开始工作" }));
    await enterAdvancedConnection();
    const surface = await screen.findByRole("region", { name: "项目与对话连接" });
    fireEvent.change(within(surface).getByLabelText("具体 ChatGPT 对话链接"), { target: { value: "https://chatgpt.com/c/conversation-a" } });
    fireEvent.click(within(surface).getByRole("button", { name: "我已在默认浏览器核对，准备绑定" }));
    api.workstreamSnapshot.mockResolvedValueOnce({ ...exactConfirmedSnapshot, activeChatgptEndpoint: { ...exactConfirmedSnapshot.activeChatgptEndpoint, externalId: "conversation-other" } });
    fireEvent.click(await within(surface).findByRole("button", { name: "确认绑定" }));
    const candidate = (await within(surface).findByText("用户已在默认浏览器核对的 ChatGPT 对话")).closest("article");
    expect(within(candidate!).getByRole("status")).toHaveTextContent("刷新后的 ACTIVE Endpoint 尚未得到一致确认");
    expect(within(candidate!).getByRole("button", { name: "ChatGPT 对话已绑定" })).toBeDisabled();
  });

  it("returns to the Workbench only after an exact refreshed ACTIVE Endpoint reproof", async () => {
    api.listExternalProjectLinks.mockResolvedValue([linkedChatGptProject]);
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "项目" }));
    fireEvent.click(await screen.findByRole("button", { name: "选择对话，开始工作" }));
    await enterAdvancedConnection();
    const surface = await screen.findByRole("region", { name: "项目与对话连接" });
    fireEvent.change(within(surface).getByLabelText("具体 ChatGPT 对话链接"), { target: { value: "https://chatgpt.com/c/conversation-a" } });
    fireEvent.click(within(surface).getByRole("button", { name: "我已在默认浏览器核对，准备绑定" }));
    api.workstreamSnapshot.mockResolvedValueOnce(exactConfirmedSnapshot);
    fireEvent.click(await within(surface).findByRole("button", { name: "确认绑定" }));
    await waitFor(() => expect(screen.queryByRole("region", { name: "项目与对话连接" })).toBeNull());
    expect(screen.queryByText("绑定没有保存。当前绑定没有改变。")).toBeNull();
  });

  it("keeps an accidental Codex card selection local and does not make it part of ChatGPT confirmation", async () => {
    const withCodex = { ...snapshot, activeCodexEndpoint: { id: "endpoint-codex-a", workstreamId: "work-a", provider: "CODEX" as const, externalId: "thread-codex-a", label: "当前 Codex 主对话", status: "ACTIVE" as const, createdAt: 1 } };
    api.workspaceSnapshot.mockResolvedValue(withCodex);
    api.workstreamSnapshot.mockResolvedValue(withCodex);
    api.listExternalProjectLinks.mockResolvedValue([linkedChatGptProject]);
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "项目" }));
    fireEvent.click(await screen.findByRole("button", { name: "选择对话，开始工作" }));
    await enterAdvancedConnection();
    const surface = await screen.findByRole("region", { name: "项目与对话连接" });
    fireEvent.change(within(surface).getByLabelText("具体 ChatGPT 对话链接"), { target: { value: "https://chatgpt.com/c/conversation-a" } });
    fireEvent.click(within(surface).getByRole("button", { name: "我已在默认浏览器核对，准备绑定" }));
    fireEvent.click(await within(surface).findByRole("button", { name: /当前 Codex 主对话/ }));
    expect(api.pairWorkstreamEndpoints).not.toHaveBeenCalled();
    fireEvent.click(within(surface).getByRole("button", { name: "确认绑定" }));
    await waitFor(() => expect(api.confirmExplicitChatGptEndpointBinding).toHaveBeenCalledWith("work-a"));
    expect(api.pairWorkstreamEndpoints).not.toHaveBeenCalled();
  });

  it("does not expose Host browser setup while automated ChatGPT access is paused", async () => {
    api.listExternalProjectLinks.mockResolvedValue([linkedChatGptProject]);
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "项目" }));
    await screen.findByText("ChatGPT 项目链接已保存");
    fireEvent.click(screen.getByRole("button", { name: "选择对话，开始工作" }));
    await enterAdvancedConnection();
    const surface = await screen.findByRole("region", { name: "项目与对话连接" });
    expect(within(surface).queryByRole("button", { name: "打开专用浏览器登录" })).toBeNull();
    expect(api.openHostChatGptBrowserSetup).not.toHaveBeenCalled();
    expect(api.prepareExplicitChatGptEndpointBinding).not.toHaveBeenCalled();
    expect(api.confirmExplicitChatGptEndpointBinding).not.toHaveBeenCalled();
  });

  it.each([
    ["选择对话，开始工作", "选择两端对话"],
    ["更换关联项目", "选择已有 Codex 项目"],
    ["现在关联 Codex", "选择已有 Codex 项目"],
    ["新建无项目 Codex 对话", "无项目 Codex 对话"],
  ])("routes D06 project action %s into its explicit existing flow", async (action, expectedHeading) => {
    api.listExternalProjectLinks.mockResolvedValue([linkedChatGptProject]);
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "项目" }));
    await screen.findByText("ChatGPT 项目链接已保存");
    fireEvent.click(screen.getByRole("button", { name: action }));
    if(action === "选择对话，开始工作") await enterAdvancedConnection();
    expect(await screen.findByRole("heading", { name: expectedHeading })).toBeVisible();
    expect(api.startUnprojectedCodexThread).not.toHaveBeenCalled();
  });

  it("opens a reviewed Codex result and its attachments from the shared reader shell", async () => {
    api.reviewWorkstreamResults.mockResolvedValue([{ runId: "codex-run-1", provider: "CODEX", resultIdentity: "result-exact-1", text: "完整 Codex 结果", reviewedAt: 1, attachments: [{ id: "attachment-1", filename: "result.md", integrityStatus: "VERIFIED", defaultSelected: true }] }]);
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "Codex 结果与附件 1" }));
    expect(await screen.findByRole("heading", { name: "A 工作区 · Codex" })).toBeVisible();
    const resultHeading = screen.getByRole("heading", { name: "A 工作区 · 历史 Codex 执行结果" });
    expect(resultHeading).toBeVisible();
    const resultCard = resultHeading.closest("article");
    expect(resultCard).not.toBeNull();
    expect(within(resultCard!).getByText("这不是当前 Codex 对话的新回复；它是 Router 过去一次执行保留的结果。可单独审阅后转发给 ChatGPT。")).toBeVisible();
    expect(within(resultCard!).getByText("精确 Router 执行结果 ID")).toBeVisible();
    expect(within(resultCard!).getByText("codex-run-1", { exact: false })).toBeInTheDocument();
    expect(screen.queryByText("result-exact-1", { exact: false })).toBeNull();
    expect(within(resultCard!).getByRole("region", { name: "结论附件" }).textContent).toContain("result.md");
    const attachment = within(resultCard!).getByRole("button", { name: /result\.md/ });
    expect(attachment).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(attachment);
    expect(attachment).toHaveAttribute("aria-pressed", "false");
    const review = within(resultCard!).getByRole("button", { name: "审阅此历史结果，准备转发给 ChatGPT" });
    expect(review).toBeEnabled();
    fireEvent.click(review);
    await waitFor(() => expect(api.prepareCodexToChatGptHandoff).toHaveBeenCalledWith("work-a", "codex-run-1", []));
  });

  it("labels a missing historical attachment as unavailable instead of offering it for relay", async () => {
    api.reviewWorkstreamResults.mockResolvedValue([{ runId: "codex-run-missing", provider: "CODEX", resultIdentity: "result-missing", text: "历史结果", attachments: [{ id: "attachment-missing", filename: ".aiwr-attachment-fixture.txt", integrityStatus: "UNAVAILABLE", defaultSelected: false, warnings: ["Local path was not found; it cannot be relayed."] }] }]);
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "Codex 结果与附件 1" }));
    const attachment = screen.getByRole("button", { name: /\.aiwr-attachment-fixture\.txt/ });
    expect(attachment).toBeDisabled();
    expect(attachment).toHaveTextContent("本地文件已不存在，不能随这次交接转发。");
    expect(screen.getByRole("button", { name: "审阅此历史结果，准备转发给 ChatGPT" })).toBeEnabled();
  });

  it("opens the exact ChatGPT result named by inbox attention and only shows history after an explicit request", async () => {
    const attention = { sourceId: "chat-target", kind: "CHATGPT_RESULT_READY", priority: 3, message: "ChatGPT 结果等待审阅", activityAt: 10 };
    api.dashboardProjection.mockResolvedValue({
      workstreams: [{ projectId: "project-1", projectName: "项目", workstream: workstreams[0], chatgptEndpoint: null, codexEndpoint: null, chatgptRun: null, codexRun: null, attentionItems: [attention], lastActivityAt: 10 }],
      attentionItems: [],
    });
    api.reviewWorkstreamResults.mockResolvedValue([
      { runId: "chat-newer", provider: "CHATGPT", resultIdentity: "chat-newer", text: "newer ChatGPT result must not replace the inbox target", attachments: [] },
      { runId: "chat-target", provider: "CHATGPT", resultIdentity: "chat-target", text: "targeted ChatGPT result", attachments: [] },
    ]);
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);

    fireEvent.click(await screen.findByRole("button", { name: /^收件箱/ }));
    fireEvent.click(screen.getByRole("button", { name: /审阅 ChatGPT 结果.*A 工作区/ }));
    expect(await screen.findByRole("region", { name: "ChatGPT 历史结果" })).toBeVisible();
    expect(screen.getByText("从收件箱打开的精确 ChatGPT 执行结果")).toBeVisible();
    expect(screen.getByText("targeted ChatGPT result")).toBeVisible();
    expect(screen.getByText("chat-target")).toBeVisible();
    expect(screen.queryByText("newer ChatGPT result must not replace the inbox target")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "查看全部 ChatGPT 历史结果" }));
    expect(await screen.findByText("newer ChatGPT result must not replace the inbox target")).toBeVisible();
  });

  it("fails closed when the exact Codex result named by inbox attention is unavailable", async () => {
    const attention = { sourceId: "codex-missing", kind: "CODEX_RESULT_READY", priority: 3, message: "Codex 结果等待审阅", activityAt: 10 };
    api.dashboardProjection.mockResolvedValue({
      workstreams: [{ projectId: "project-1", projectName: "项目", workstream: workstreams[0], chatgptEndpoint: null, codexEndpoint: null, chatgptRun: null, codexRun: null, attentionItems: [attention], lastActivityAt: 10 }],
      attentionItems: [],
    });
    api.reviewWorkstreamResults.mockResolvedValue([{ runId: "codex-other", provider: "CODEX", resultIdentity: "codex-other", text: "another Codex result must not replace the inbox target", attachments: [] }]);
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);

    fireEvent.click(await screen.findByRole("button", { name: /^收件箱/ }));
    fireEvent.click(screen.getByRole("button", { name: /审阅 Codex 结果.*A 工作区/ }));
    expect(await screen.findByRole("region", { name: "Codex 结果与附件" })).toBeVisible();
    expect(screen.getByRole("alert")).toHaveTextContent("这条精确 Codex 历史结果当前不可读；Router 没有切换到其他结果。");
    expect(screen.queryByText("another Codex result must not replace the inbox target")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "查看全部 Codex 历史结果" }));
    expect(await screen.findByText("another Codex result must not replace the inbox target")).toBeVisible();
  });

  it("opens only the exact Codex structured request selected from the desktop Inbox", async () => {
    const codexSnapshot = {
      ...snapshot,
      activeCodexEndpoint: { id: "endpoint-codex-a", workstreamId: "work-a", provider: "CODEX" as const, externalId: "thread-codex-a", label: "实施主对话", status: "ACTIVE" as const, createdAt: 1 },
    };
    const attention = { sourceId: "request-target", kind: "CODEX_STRUCTURED_REQUEST", priority: 3, message: "需要一次授权", activityAt: 10 };
    api.workspaceSnapshot.mockResolvedValue(codexSnapshot);
    api.workstreamSnapshot.mockResolvedValue(codexSnapshot);
    api.selectWorkspace.mockResolvedValue(codexSnapshot);
    api.dashboardProjection.mockResolvedValue({
      workstreams: [{ projectId: "project-1", projectName: "项目", workstream: workstreams[0], chatgptEndpoint: null, codexEndpoint: codexSnapshot.activeCodexEndpoint, chatgptRun: null, codexRun: null, attentionItems: [attention], lastActivityAt: 10 }],
      attentionItems: [],
    });
    api.codexRequests.mockResolvedValue([
      { requestId: "request-other", revision: 1, method: "item/commandExecution/requestApproval", kind: "COMMAND_APPROVAL", reason: "较早的请求，不能替代收件箱选择", choices: [{ id: "accept", label: "Allow once" }, { id: "decline", label: "Decline" }], questions: [], isBlocking: true, responseSent: false },
      { requestId: "request-target", revision: 2, method: "item/commandExecution/requestApproval", kind: "COMMAND_APPROVAL", reason: "收件箱点击的精确请求", choices: [{ id: "accept", label: "Allow once" }, { id: "decline", label: "Decline" }], questions: [], isBlocking: true, responseSent: false },
    ]);
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);

    fireEvent.click(await screen.findByRole("button", { name: /^收件箱/ }));
    fireEvent.click(screen.getByRole("button", { name: /处理 Codex 请求.*A 工作区/ }));

    expect(await screen.findByRole("region", { name: "Codex 官方结构化请求" })).toBeVisible();
    expect(screen.getByText((_, element) => element?.textContent === "请求原因：收件箱点击的精确请求")).toBeVisible();
    expect(screen.queryByText("较早的请求，不能替代收件箱选择")).toBeNull();
    expect(screen.getByText("request-target")).toBeVisible();
  });

  it("fails closed when the desktop Inbox request target is no longer readable", async () => {
    const codexSnapshot = {
      ...snapshot,
      activeCodexEndpoint: { id: "endpoint-codex-a", workstreamId: "work-a", provider: "CODEX" as const, externalId: "thread-codex-a", label: "实施主对话", status: "ACTIVE" as const, createdAt: 1 },
    };
    const attention = { sourceId: "request-missing", kind: "CODEX_STRUCTURED_REQUEST", priority: 3, message: "需要一次授权", activityAt: 10 };
    api.workspaceSnapshot.mockResolvedValue(codexSnapshot);
    api.workstreamSnapshot.mockResolvedValue(codexSnapshot);
    api.selectWorkspace.mockResolvedValue(codexSnapshot);
    api.dashboardProjection.mockResolvedValue({
      workstreams: [{ projectId: "project-1", projectName: "项目", workstream: workstreams[0], chatgptEndpoint: null, codexEndpoint: codexSnapshot.activeCodexEndpoint, chatgptRun: null, codexRun: null, attentionItems: [attention], lastActivityAt: 10 }],
      attentionItems: [],
    });
    api.codexRequests.mockResolvedValue([{ requestId: "request-other", revision: 1, method: "item/commandExecution/requestApproval", kind: "COMMAND_APPROVAL", reason: "其他请求不能替代已失效目标", choices: [{ id: "accept", label: "Allow once" }, { id: "decline", label: "Decline" }], questions: [], isBlocking: true, responseSent: false }]);
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);

    fireEvent.click(await screen.findByRole("button", { name: /^收件箱/ }));
    fireEvent.click(screen.getByRole("button", { name: /处理 Codex 请求.*A 工作区/ }));

    expect(await screen.findByRole("alert")).toHaveTextContent("收件箱指定的精确 Codex 请求当前不可读或已不存在；Router 没有显示其他请求。");
    expect(screen.getByText("request-missing")).toBeVisible();
    expect(screen.queryByText("其他请求不能替代已失效目标")).toBeNull();
  });

  it("opens the exact Provider run status after a desktop Inbox status action", async () => {
    const attention = { sourceId: "run-target", kind: "PROVIDER_RUN_FAILED", priority: 3, message: "执行失败", activityAt: 10 };
    api.dashboardProjection.mockResolvedValue({
      workstreams: [{ projectId: "project-1", projectName: "项目", workstream: workstreams[0], chatgptEndpoint: null, codexEndpoint: null, chatgptRun: null, codexRun: null, attentionItems: [attention], lastActivityAt: 10 }],
      attentionItems: [],
    });
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);

    fireEvent.click(await screen.findByRole("button", { name: /^收件箱/ }));
    fireEvent.click(screen.getByRole("button", { name: /查看失败执行状态.*A 工作区/ }));

    expect(await screen.findByText("Codex 执行：FAILED")).toBeVisible();
    expect(screen.getByText("run-target")).toBeVisible();
    expect(screen.getByLabelText("Router 记录的执行时间")).toHaveTextContent("开始：");
    expect(screen.getByLabelText("Router 记录的执行时间")).toHaveTextContent("终态确认：");
    expect(screen.getByLabelText("Router 记录的执行时间")).toHaveTextContent("最后状态更新：");
    expect(screen.getByText(/不会自动重试、发送或替换为另一条结果/)).toBeVisible();
    expect(api.providerRunStatus).toHaveBeenCalledWith("work-a", "run-target");
  });

  it("reads the targeted exact ChatGPT observation in a ChatGPT-only workstream without replacing it with a newer reply", async () => {
    const chatOnlySnapshot = { ...snapshot, activeChatgptEndpoint: { id: "endpoint-chat", workstreamId: "work-a", provider: "CHATGPT", externalId: "conversation-exact", label: "精确 ChatGPT 对话", status: "ACTIVE", createdAt: 1 }, activeCodexEndpoint: null };
    api.workspaceSnapshot.mockResolvedValue(chatOnlySnapshot);
    api.workstreamSnapshot.mockResolvedValue(chatOnlySnapshot);
    api.replyObservations.mockResolvedValue([
      { id: "reply-new", text: "newer reply must not replace the requested reply", observedAt: 20, pushState: "SENT" },
      { id: "reply-old", text: "targeted exact ChatGPT reply", observedAt: 10, pushState: "SENT" },
    ]);
    window.history.replaceState(null, "", "/?workstream=work-a&reply=reply-old");
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    expect(await screen.findByText("targeted exact ChatGPT reply")).toBeVisible();
    expect(screen.queryByText("newer reply must not replace the requested reply")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "标为已读" }));
    await waitFor(() => expect(api.markReplyObservationRead).toHaveBeenCalledWith("work-a", "reply-old"));
    expect(await screen.findByText("targeted exact ChatGPT reply")).toBeVisible();
    expect(screen.queryByText("newer reply must not replace the requested reply")).toBeNull();
  });

  it("fails closed when the exact ChatGPT inbox observation is unavailable instead of showing another reply or current-page content", async () => {
    const attention = { sourceId: "chat-missing", kind: "CHATGPT_REPLY_OBSERVED", priority: 3, message: "ChatGPT 有新回复", activityAt: 10 };
    api.workspaceSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    api.workstreamSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    api.dashboardProjection.mockResolvedValue({
      workstreams: [{ projectId: "project-1", projectName: "项目", workstream: workstreams[0], chatgptEndpoint: exactConfirmedSnapshot.activeChatgptEndpoint, codexEndpoint: exactConfirmedSnapshot.activeCodexEndpoint, chatgptRun: null, codexRun: null, attentionItems: [attention], lastActivityAt: 10 }],
      attentionItems: [],
    });
    api.replyObservations.mockResolvedValue([{ id: "chat-new", endpointId: "endpoint-chatgpt-a", text: "another ChatGPT reply must not replace an unavailable inbox target", observedAt: 20, pushState: "SENT" }]);
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);

    fireEvent.click(await screen.findByRole("button", { name: /^收件箱/ }));
    const missingAttention = screen.getAllByRole("button", { name: /查看 ChatGPT 新回复.*A 工作区/ })
      .find((button) => button.textContent?.includes("chat-missing"));
    expect(missingAttention).toBeDefined();
    fireEvent.click(missingAttention!);
    expect(await screen.findByText("收件箱指定的精确 ChatGPT 回复当前不可读；Router 没有显示当前页内容或其他回复。")).toBeVisible();
    expect(screen.queryByText("another ChatGPT reply must not replace an unavailable inbox target")).toBeNull();
    expect(screen.queryByRole("button", { name: "选择/编辑交给 Codex 的范围" })).toBeNull();
  });

  it("fails closed when the exact Codex inbox observation is unavailable instead of showing a newer thread reply", async () => {
    const attention = { sourceId: "codex-missing", kind: "CODEX_REPLY_OBSERVED", priority: 3, message: "Codex 有新回复", activityAt: 10 };
    api.workspaceSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    api.workstreamSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    api.dashboardProjection.mockResolvedValue({
      workstreams: [{ projectId: "project-1", projectName: "项目", workstream: workstreams[0], chatgptEndpoint: exactConfirmedSnapshot.activeChatgptEndpoint, codexEndpoint: exactConfirmedSnapshot.activeCodexEndpoint, chatgptRun: null, codexRun: null, attentionItems: [attention], lastActivityAt: 10 }],
      attentionItems: [],
    });
    api.replyObservations.mockResolvedValue([{ id: "codex-new", endpointId: "endpoint-codex-a", text: "newer Codex reply must not replace an unavailable inbox target", observedAt: 20, pushState: "SENT" }]);
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);

    fireEvent.click(await screen.findByRole("button", { name: /^收件箱/ }));
    const missingAttention = screen.getAllByRole("button", { name: /查看 Codex 新回复.*A 工作区/ })
      .find((button) => button.textContent?.includes("codex-missing"));
    expect(missingAttention).toBeDefined();
    fireEvent.click(missingAttention!);
    expect(await screen.findByRole("region", { name: "Codex 对话" })).toBeVisible();
    expect(screen.getByRole("alert")).toHaveTextContent("收件箱指定的精确 Codex 回复当前不可读；Router 没有显示同一线程的较新回复。");
    expect(screen.queryByText("newer Codex reply must not replace an unavailable inbox target")).toBeNull();
    expect(screen.queryByRole("button", { name: "审阅此回复，准备转发给 ChatGPT" })).toBeNull();
  });

  it("opens and reviews the exact Codex inbox observation instead of substituting the newest reply", async () => {
    api.prepareCodexToChatGptHandoff.mockResolvedValue({ actionId: "codex-observation-review", revision: 1, status: "READY", message: "可编辑的 Codex 回传", attachments: ["selected.txt"] });
    const attention = { sourceId: "codex-old", kind: "CODEX_REPLY_OBSERVED", priority: 3, message: "Codex 有新回复", activityAt: 10 };
    api.workspaceSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    api.workstreamSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    api.dashboardProjection.mockResolvedValue({
      workstreams: [{ projectId: "project-1", projectName: "项目", workstream: workstreams[0], chatgptEndpoint: exactConfirmedSnapshot.activeChatgptEndpoint, codexEndpoint: exactConfirmedSnapshot.activeCodexEndpoint, chatgptRun: null, codexRun: null, attentionItems: [attention], lastActivityAt: 10 }],
      attentionItems: [],
    });
    api.replyObservations.mockResolvedValue([
      { id: "codex-new", endpointId: "endpoint-codex-a", text: "newer Codex reply must not replace the requested reply", observedAt: 20, pushState: "SENT" },
      { id: "codex-old", endpointId: "endpoint-codex-a", text: "targeted exact Codex reply", observedAt: 10, pushState: "SENT", attachments: [
        { id: "selected-file", filename: "selected.txt", integrityStatus: "VERIFIED", defaultSelected: true },
        { id: "blocked-file", filename: "changed.txt", integrityStatus: "MISMATCH", defaultSelected: false },
      ] },
    ]);
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);

    fireEvent.click(await screen.findByRole("button", { name: /^收件箱/ }));
    const attentionList = await screen.findByRole("region", { name: "待处理列表" });
    const exactAttention = within(attentionList).getAllByRole("button", { name: /查看 Codex 新回复.*A 工作区/ })
      .find((button) => button.textContent?.includes("Codex 有新回复"));
    expect(exactAttention).toBeDefined();
    fireEvent.click(exactAttention!);
    expect(await screen.findByRole("region", { name: "Codex 对话" })).toBeVisible();
    expect(screen.getByText("从收件箱打开的精确 Codex 回复")).toBeVisible();
    expect(screen.getByText("targeted exact Codex reply")).toBeVisible();
    expect(screen.queryByText("newer Codex reply must not replace the requested reply")).toBeNull();
    const file = screen.getByRole("checkbox", { name: /selected.txt/ });
    expect(file).not.toBeChecked();
    expect(screen.getByRole("checkbox", { name: /changed.txt/ })).toBeDisabled();
    expect(api.prepareCodexToChatGptHandoff).not.toHaveBeenCalled();
    fireEvent.click(file);
    fireEvent.click(screen.getByRole("button", { name: "审阅此回复，准备转发给 ChatGPT" }));
    await waitFor(() => expect(api.prepareCodexToChatGptHandoff).toHaveBeenCalledWith("work-a", "codex-old", ["selected-file"]));
    expect(await screen.findByRole("heading", { name: "审阅后批准，再单独发送给同工作区的 ChatGPT" })).toBeVisible();
    expect(screen.getByText("selected.txt")).toBeVisible();
    expect(api.approveCodexToChatGptHandoff).not.toHaveBeenCalled();
    expect(api.sendCodexToChatGptHandoffReview).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "查看 Codex 完整原回复" }));
    expect(await screen.findByRole("region", { name: "Codex 对话" })).toBeVisible();
    expect(screen.getByText("targeted exact Codex reply")).toBeVisible();
    expect(screen.queryByText("newer Codex reply must not replace the requested reply")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "审阅此回复，准备转发给 ChatGPT" }));
    await waitFor(() => expect(api.prepareCodexToChatGptHandoff).toHaveBeenCalledTimes(2));
    fireEvent.click(await screen.findByRole("button", { name: "批准此版本（不会发送）" }));
    expect(await screen.findByRole("button", { name: "返回编辑（需重新批准）" })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "返回编辑（需重新批准）" }));
    await waitFor(() => expect(api.prepareCodexToChatGptHandoff).toHaveBeenCalledTimes(3));
    expect(await screen.findByRole("heading", { name: "审阅后批准，再单独发送给同工作区的 ChatGPT" })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "批准此版本（不会发送）" }));
    fireEvent.click(await screen.findByRole("button", { name: "单独发送给 ChatGPT" }));
    await waitFor(() => expect(api.sendCodexToChatGptHandoffReview).toHaveBeenCalledWith("codex-observation-review", 2));
    expect(await screen.findByText("已发送，执行状态独立")).toBeVisible();
  });

  it("opens only the current exact ChatGPT destination for an approved manual Codex handoff", async () => {
    const attention = { sourceId: "codex-manual", kind: "CODEX_REPLY_OBSERVED", priority: 3, message: "Codex 有新回复", activityAt: 10 };
    api.workspaceSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    api.workstreamSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    api.dashboardProjection.mockResolvedValue({
      workstreams: [{ projectId: "project-1", projectName: "项目", workstream: workstreams[0], chatgptEndpoint: exactConfirmedSnapshot.activeChatgptEndpoint, codexEndpoint: exactConfirmedSnapshot.activeCodexEndpoint, chatgptRun: null, codexRun: null, attentionItems: [attention], lastActivityAt: 10 }],
      attentionItems: [],
    });
    api.replyObservations.mockResolvedValue([{ id: "codex-manual", endpointId: "endpoint-codex-a", text: "exact Codex reply for a manual handoff", observedAt: 10, pushState: "SENT" }]);
    api.prepareCodexToChatGptHandoff.mockResolvedValue({ actionId: "manual-codex-review", revision: 1, status: "READY", message: "approved exact payload", attachments: [], requiresManualDispatch: true });
    api.approveCodexToChatGptHandoff.mockResolvedValue({ actionId: "manual-codex-review", revision: 2, status: "APPROVED", message: "approved exact payload", attachments: [], requiresManualDispatch: true });
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);

    fireEvent.click(await screen.findByRole("button", { name: /^收件箱/ }));
    const exactAttention = (await screen.findAllByRole("button", { name: /查看 Codex 新回复.*A 工作区/ }))
      .find((button) => button.textContent?.includes("Codex 有新回复"));
    expect(exactAttention).toBeDefined();
    fireEvent.click(exactAttention!);
    fireEvent.click(await screen.findByRole("button", { name: "审阅此回复，准备转发给 ChatGPT" }));
    fireEvent.click(await screen.findByRole("button", { name: "批准此版本（不会发送）" }));
    const openExactDestination = await screen.findByRole("button", { name: "在默认浏览器查看这个精确 ChatGPT 对话" });
    fireEvent.click(openExactDestination);
    await waitFor(() => expect(api.openBoundChatGptInDefaultBrowser).toHaveBeenCalledExactlyOnceWith("work-a"));
    expect(api.openBoundChatGptConversation).not.toHaveBeenCalled();
  });

  it("opens a prewrite failure's exact destination only when it still matches the current binding", async () => {
    const failedHandoff = {
      id: "handoff-prewrite", workstreamId: "work-a", direction: "CODEX_TO_CHATGPT" as const,
      originalText: "approved exact payload", approvedText: "approved exact payload", status: "FAILED" as const,
      payloadHash: "hash", createdAt: 1, failedAt: 2, errorCode: "CHATGPT_AUTH_OR_COMPOSER_REQUIRED", errorMessage: "No write occurred.",
      sourceEndpoint: exactConfirmedSnapshot.activeCodexEndpoint, destinationEndpoint: exactConfirmedSnapshot.activeChatgptEndpoint, attachments: [],
    };
    const failedSnapshot = { ...exactConfirmedSnapshot, handoffs: [failedHandoff] };
    const attention = { sourceId: "handoff-prewrite", kind: "HANDOFF_FAILED" as const, priority: 3, message: "交付失败", activityAt: 10 };
    api.workspaceSnapshot.mockResolvedValue(failedSnapshot);
    api.workstreamSnapshot.mockResolvedValue(failedSnapshot);
    api.dashboardProjection.mockResolvedValue({
      workstreams: [{ projectId: "project-1", projectName: "项目", workstream: workstreams[0], chatgptEndpoint: exactConfirmedSnapshot.activeChatgptEndpoint, codexEndpoint: exactConfirmedSnapshot.activeCodexEndpoint, chatgptRun: null, codexRun: null, attentionItems: [attention], lastActivityAt: 10 }],
      attentionItems: [],
    });
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);

    fireEvent.click(await screen.findByRole("button", { name: /^收件箱/ }));
    fireEvent.click((await screen.findAllByRole("button", { name: /检查交付失败.*A 工作区/ }))[0]);
    expect(await screen.findByText(/Router 在写入输入框前已停止；没有内容发送到 ChatGPT。/)).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "在默认浏览器查看这个精确 ChatGPT 对话" }));
    await waitFor(() => expect(api.openBoundChatGptInDefaultBrowser).toHaveBeenCalledExactlyOnceWith("work-a"));
  });

  it("refuses a prewrite failure destination that no longer equals the current binding", async () => {
    const failedHandoff = {
      id: "handoff-stale", workstreamId: "work-a", direction: "CODEX_TO_CHATGPT" as const,
      originalText: "old payload", approvedText: "old payload", status: "FAILED" as const,
      payloadHash: "hash", createdAt: 1, failedAt: 2, errorCode: "CHATGPT_AUTH_OR_COMPOSER_REQUIRED", errorMessage: "No write occurred.",
      sourceEndpoint: exactConfirmedSnapshot.activeCodexEndpoint,
      destinationEndpoint: { ...exactConfirmedSnapshot.activeChatgptEndpoint, externalId: "conversation-old" }, attachments: [],
    };
    const failedSnapshot = { ...exactConfirmedSnapshot, handoffs: [failedHandoff] };
    const attention = { sourceId: "handoff-stale", kind: "HANDOFF_FAILED" as const, priority: 3, message: "旧交付失败", activityAt: 10 };
    api.workspaceSnapshot.mockResolvedValue(failedSnapshot);
    api.workstreamSnapshot.mockResolvedValue(failedSnapshot);
    api.dashboardProjection.mockResolvedValue({
      workstreams: [{ projectId: "project-1", projectName: "项目", workstream: workstreams[0], chatgptEndpoint: exactConfirmedSnapshot.activeChatgptEndpoint, codexEndpoint: exactConfirmedSnapshot.activeCodexEndpoint, chatgptRun: null, codexRun: null, attentionItems: [attention], lastActivityAt: 10 }],
      attentionItems: [],
    });
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);

    fireEvent.click(await screen.findByRole("button", { name: /^收件箱/ }));
    fireEvent.click((await screen.findAllByRole("button", { name: /检查交付失败.*A 工作区/ }))[0]);
    fireEvent.click(await screen.findByRole("button", { name: "在默认浏览器查看这个精确 ChatGPT 对话" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("已批准交接的精确 ChatGPT 目标不再等于当前绑定；Router 没有用当前或最近对话替代它。");
    expect(api.openBoundChatGptInDefaultBrowser).not.toHaveBeenCalled();
  });

  it("projects only unread exact ReplyObservations into the desktop inbox", async () => {
    api.replyObservations.mockResolvedValue([
      { id: "reply-unread", text: "unread reply", observedAt: 30, pushState: "SENT" },
      { id: "reply-read", text: "read but unresolved reply", observedAt: 20, readAt: 21, pushState: "SENT" },
      { id: "reply-handled", text: "handled reply", observedAt: 10, readAt: 11, handledAt: 12, pushState: "SENT" },
    ]);
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "收件箱" }));
    const inbox = await screen.findByRole("region", { name: "收件箱" });
    expect(inbox).toHaveTextContent("1 条具体事项等待你处理");
    const attentionList = within(inbox).getByRole("region", { name: "待处理列表" });
    expect(attentionList).toHaveTextContent("A 工作区");
    expect(attentionList).not.toHaveTextContent("2 项待处理");
  });

  it("creates a clean unbound Router workstream before opening the precise pairing flow", async () => {
    const clean = { id: "work-clean", projectId: "project-1", name: "干净验收工作区", status: "ACTIVE", createdAt: 2, updatedAt: 2, bindingRevision: 0 };
    const cleanSnapshot = { ...snapshot, workstreams: [...workstreams, clean], selectedWorkstreamId: "work-clean", activeChatgptEndpoint: null, activeCodexEndpoint: null };
    api.workstreamSnapshot.mockImplementation(async (id: string) => id === "work-clean" ? cleanSnapshot : snapshot);
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "收件箱" }));
    fireEvent.click(screen.getByRole("button", { name: "＋ 新建工作" }));
    const newWork = await screen.findByRole("region", { name: "新建工作" });
    expect(newWork).toHaveTextContent("从已有项目开始");
    expect(newWork).toHaveTextContent("不会复用当前工作区的 ChatGPT 或 Codex 对话");
    expect(newWork).toHaveTextContent("创建无项目 Codex 对话，不创建新的 Codex 项目。");
    fireEvent.change(within(newWork).getByLabelText("新工作名称"), { target: { value: "干净验收工作区" } });
    fireEvent.click(within(newWork).getByRole("button", { name: "创建未绑定工作并选择对话" }));
    await waitFor(() => expect(api.createWorkstream).toHaveBeenCalledWith("project-1", "干净验收工作区"));
    await enterAdvancedConnection();
    expect(await screen.findByRole("heading", { name: "选择两端对话" })).toBeVisible();
    expect(screen.getByText("干净验收工作区")).toBeVisible();
    expect(api.readThreadHistory).not.toHaveBeenCalled();
    expect(api.readCodexGoal).not.toHaveBeenCalled();
    expect(api.pairWorkstreamEndpoints).not.toHaveBeenCalled();
    expect(api.prepareExplicitChatGptEndpointBinding).not.toHaveBeenCalled();
    expect(api.startUnprojectedCodexThread).not.toHaveBeenCalled();
  });

  it("creates an independent Router project and its unbound work before opening exact endpoint pairing", async () => {
    const independent = { id: "work-game", projectId: "project-game", name: "当前游戏开发", status: "ACTIVE", createdAt: 2, updatedAt: 2, bindingRevision: 0 };
    const independentSnapshot = { ...snapshot, projects: [...snapshot.projects, { id: "project-game", name: "游戏开发", createdAt: 2, updatedAt: 2 }], workstreams: [...workstreams, independent], selectedProjectId: "project-game", selectedWorkstreamId: "work-game", activeChatgptEndpoint: null, activeCodexEndpoint: null };
    api.createProject.mockResolvedValueOnce({ id: "project-game", name: "游戏开发", createdAt: 2, updatedAt: 2 });
    api.createWorkstream.mockResolvedValueOnce(independent);
    api.workstreamSnapshot.mockImplementation(async (id: string) => id === "work-game" ? independentSnapshot : snapshot);
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "收件箱" }));
    fireEvent.click(screen.getByRole("button", { name: "＋ 新建工作" }));
    const newWork = await screen.findByRole("region", { name: "新建工作" });
    fireEvent.change(within(newWork).getByLabelText("新 Router 项目名称"), { target: { value: "游戏开发" } });
    fireEvent.change(within(newWork).getByLabelText("新 Router 项目工作名称"), { target: { value: "当前游戏开发" } });
    fireEvent.click(within(newWork).getByRole("button", { name: "创建项目并选择两端对话" }));
    await waitFor(() => expect(api.createProject).toHaveBeenCalledWith("游戏开发"));
    await waitFor(() => expect(api.createWorkstream).toHaveBeenCalledWith("project-game", "当前游戏开发"));
    await enterAdvancedConnection();
    expect(await screen.findByRole("heading", { name: "选择两端对话" })).toBeVisible();
    expect(screen.getByText("当前游戏开发")).toBeVisible();
    expect(api.readThreadHistory).not.toHaveBeenCalled();
    expect(api.readCodexGoal).not.toHaveBeenCalled();
  });

  it("projects the edited desktop draft as saving then saved at the Core timestamp", async () => {
    api.readWorkstreamDraft.mockResolvedValue({ workstreamId: "work-a", text: "已加载", revision: 1, updatedAt: 100 });
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "修改" }));
    const textarea = await screen.findByLabelText("你的修改意见");
    await waitFor(() => expect(textarea).toHaveValue("已加载"));
    const persistedAt = Date.UTC(2026, 8, 10, 8, 30);
    let resolveSave!: (value: { workstreamId: string; text: string; revision: number; updatedAt: number }) => void;
    api.saveWorkstreamDraft.mockImplementationOnce(() => new Promise((resolve) => { resolveSave = resolve; }));
    vi.useFakeTimers();
    fireEvent.change(textarea, { target: { value: "待保存的桌面草稿" } });
    expect(screen.getByText("正在自动保存")).toBeVisible();
    await act(async () => { await vi.advanceTimersByTimeAsync(500); });
    expect(api.saveWorkstreamDraft).toHaveBeenCalledWith("work-a", "待保存的桌面草稿", 1);
    await act(async () => { resolveSave({ workstreamId: "work-a", text: "待保存的桌面草稿", revision: 9, updatedAt: persistedAt }); });
    expect(screen.getByText("草稿已自动保存", { exact: false })).toHaveTextContent(new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(persistedAt));
  });

  it("keeps the desktop draft text and reports failed when its matching save rejects", async () => {
    api.readWorkstreamDraft.mockResolvedValue({ workstreamId: "work-a", text: "已加载", revision: 1, updatedAt: 100 });
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "修改" }));
    const textarea = await screen.findByLabelText("你的修改意见");
    await waitFor(() => expect(textarea).toHaveValue("已加载"));
    api.saveWorkstreamDraft.mockRejectedValueOnce(new Error("offline"));
    vi.useFakeTimers();
    fireEvent.change(textarea, { target: { value: "不要丢失" } });
    await act(async () => { await vi.advanceTimersByTimeAsync(500); });
    expect(screen.getByRole("alert")).toHaveTextContent("草稿保存失败");
    expect(textarea).toHaveValue("不要丢失");
  });

  it("serializes successive desktop draft saves so a later edit uses the accepted Core revision", async () => {
    api.readWorkstreamDraft.mockResolvedValue({ workstreamId: "work-a", text: "已加载", revision: 1, updatedAt: 100 });
    let resolveFirst!: (value: { workstreamId: string; text: string; revision: number; updatedAt: number }) => void;
    let resolveSecond!: (value: { workstreamId: string; text: string; revision: number; updatedAt: number }) => void;
    api.saveWorkstreamDraft
      .mockImplementationOnce(() => new Promise((resolve) => { resolveFirst = resolve; }))
      .mockImplementationOnce(() => new Promise((resolve) => { resolveSecond = resolve; }));
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "修改" }));
    const textarea = await screen.findByLabelText("你的修改意见");
    await waitFor(() => expect(textarea).toHaveValue("已加载"));
    vi.useFakeTimers();
    fireEvent.change(textarea, { target: { value: "第一版" } });
    await act(async () => { await vi.advanceTimersByTimeAsync(500); });
    expect(api.saveWorkstreamDraft).toHaveBeenLastCalledWith("work-a", "第一版", 1);
    fireEvent.change(textarea, { target: { value: "第二版" } });
    await act(async () => { await vi.advanceTimersByTimeAsync(500); });
    expect(api.saveWorkstreamDraft).toHaveBeenCalledTimes(1);
    await act(async () => { resolveFirst({ workstreamId: "work-a", text: "第一版", revision: 2, updatedAt: 200 }); });
    await act(async () => { await vi.advanceTimersByTimeAsync(500); });
    expect(api.saveWorkstreamDraft).toHaveBeenLastCalledWith("work-a", "第二版", 2);
    await act(async () => { resolveSecond({ workstreamId: "work-a", text: "第二版", revision: 3, updatedAt: 300 }); });
    expect(screen.getByText("草稿已自动保存", { exact: false })).toBeVisible();
    expect(textarea).toHaveValue("第二版");
  });

  it("does not let an earlier workstream save certify the newly selected desktop draft", async () => {
    api.readWorkstreamDraft.mockImplementation(async (id: string) => id === "work-a" ? { workstreamId: id, text: "已加载", revision: 1, updatedAt: 100 } : null);
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "修改" }));
    const textarea = await screen.findByLabelText("你的修改意见");
    await waitFor(() => expect(textarea).toHaveValue("已加载"));
    let resolveSave!: (value: { workstreamId: string; text: string; revision: number; updatedAt: number }) => void;
    api.saveWorkstreamDraft.mockImplementationOnce(() => new Promise((resolve) => { resolveSave = resolve; }));
    vi.useFakeTimers();
    fireEvent.change(textarea, { target: { value: "A 的未完成保存" } });
    await act(async () => { await vi.advanceTimersByTimeAsync(500); });
    fireEvent.click(screen.getByRole("button", { name: /B 工作区/ }));
    await act(async () => { resolveSave({ workstreamId: "work-a", text: "A 的未完成保存", revision: 7, updatedAt: 777 }); });
    await act(async () => {});
    fireEvent.click(screen.getByRole("button", { name: "修改" }));
    expect(screen.getByLabelText("你的修改意见")).toHaveValue("");
    expect(screen.getByText("草稿尚未保存")).toBeVisible();
  });

  it("uses the integrated Router browser instead of a legacy Connector action", async () => {
    api.workspaceSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    api.workstreamSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    api.openBoundChatGptConversation.mockResolvedValue(undefined);
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "打开 Router 浏览器" }));
    await waitFor(() => expect(api.openBoundChatGptConversation).toHaveBeenCalledExactlyOnceWith("work-a"));
    await waitFor(() => expect(api.openHostChatGptBrowserSetup).toHaveBeenCalledExactlyOnceWith());
    expect(await screen.findByText(/保存绑定后，这个窗口会保持打开/)).toBeVisible();
    expect(api.openBoundChatGptInDefaultBrowser).not.toHaveBeenCalled();
    expect(api.connectNormalChromeReadOnly).not.toHaveBeenCalled();
    expect(api.sendChatGptRequest).not.toHaveBeenCalled();
  });

  it.each(["assistant", "ASSISTANT"])("shows the exact native terminal DTO with %s role instead of a loading error", async (role) => {
    api.workspaceSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    api.workstreamSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    api.readActiveChatGptProviderLatestSnapshot.mockResolvedValue({
      href: "https://chatgpt.com/c/conversation-a", conversationId: "conversation-a",
      turns: [{ id: "native-terminal-a", role, text: "Native exact terminal result", streaming: false, terminal: true }],
    });
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "重新读取当前对话" }));
    expect(await screen.findByText("Native exact terminal result")).toBeVisible();
    expect(screen.queryByText(/页面内容仍在加载/)).toBeNull();
    expect(api.readActiveChatGptProviderLatestSnapshot).toHaveBeenCalledExactlyOnceWith("work-a");
    expect(api.openBoundChatGptConversation).not.toHaveBeenCalled();
    expect(api.sendChatGptRequest).not.toHaveBeenCalled();
  });

  it("displays current terminal content after the first successful check without manufacturing a new observation", async () => {
    api.workspaceSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    api.workstreamSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    api.checkNewChatGptReplies.mockResolvedValue({ state: "OBSERVER_BASELINE_ESTABLISHED", observationCreated: false });
    api.readActiveChatGptProviderLatestSnapshot.mockResolvedValue({
      href: "https://chatgpt.com/c/conversation-a", conversationId: "conversation-a",
      turns: [{ id: "native-baseline-a", role: "ASSISTANT", text: "Visible baseline content" }],
    });
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "检查新回复" }));
    expect(await screen.findByText("Visible baseline content")).toBeVisible();
    expect(screen.getByText(/现有历史不会作为新回复通知/)).toBeVisible();
    expect(screen.queryByText("已记录 1 条新终态回复。")).toBeNull();
    expect(api.sendChatGptRequest).not.toHaveBeenCalled();
  });

  it("reports ordinary Open foreground denial without recreating or confirming the browser", async () => {
    api.workspaceSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    api.workstreamSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    api.openHostChatGptBrowserSetup.mockRejectedValueOnce("SECURITY_BROWSER_FOCUS_BLOCKED");
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "打开 Router 浏览器" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Windows 阻止了切换到前台");
    expect(screen.queryByText(/保存绑定后，这个窗口会保持打开/)).toBeNull();
    expect(api.confirmChatGptAuthenticationCompleted).not.toHaveBeenCalled();
    expect(api.sendChatGptRequest).not.toHaveBeenCalled();
  });

  it("updates startup and disconnect presentation from Core status without a connection action", async () => {
    api.workspaceSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    api.workstreamSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    api.status.mockResolvedValue({ connected: false, connecting: true, detail: "Connecting to Codex…" });
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    expect(await screen.findByText("绑定已保存 · 服务未连接")).toBeVisible();
    await waitFor(() => expect(eventHandlers.get("codex-backend-connected")?.size).toBe(1));
    api.status.mockResolvedValue({ connected: true, connecting: false, detail: null });
    await act(async () => eventHandlers.get("codex-backend-connected")?.forEach(handler => handler({ payload: "" })));
    await waitFor(() => expect(screen.queryByText("绑定已保存 · 服务未连接")).toBeNull());
    api.status.mockResolvedValue({ connected: false, connecting: false, detail: "Native transport closed" });
    await act(async () => eventHandlers.get("codex-backend-disconnected")?.forEach(handler => handler({ payload: "ignored as authority" })));
    expect(await screen.findByText("绑定已保存 · 服务未连接")).toBeVisible();
    expect(api.connect).not.toHaveBeenCalled();
    expect(api.sendChatGptRequest).not.toHaveBeenCalled();
  });

  it("rechecks a startup event during a pending status read instead of retaining its stale result", async () => {
    api.workspaceSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    api.workstreamSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    let finishInitial!: (status: { connected: boolean; connecting: boolean }) => void;
    api.status.mockImplementationOnce(() => new Promise(resolve => { finishInitial = resolve; }));
    api.status.mockResolvedValue({ connected: true, connecting: false });
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    await waitFor(() => expect(api.status).toHaveBeenCalledTimes(1));
    await act(async () => eventHandlers.get("codex-backend-connected")?.forEach(handler => handler({ payload: "" })));
    expect(api.status).toHaveBeenCalledTimes(1);
    await act(async () => finishInitial({ connected: false, connecting: true }));
    await waitFor(() => expect(api.status).toHaveBeenCalledTimes(2));
    expect(screen.queryByText("绑定已保存 · 服务未连接")).toBeNull();
    expect(api.connect).not.toHaveBeenCalled();
  });

  it("offers a bounded read-only Codex connection with its exact failure detail", async () => {
    api.workspaceSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    api.workstreamSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    api.status.mockResolvedValue({ connected: false, connecting: false, detail: "Codex app-server unavailable" });
    api.connect.mockResolvedValue({ connected: false, connecting: false, detail: "Codex app-server unavailable" });
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);

    fireEvent.click(await screen.findByRole("button", { name: "运行环境" }));
    const connect = await screen.findByRole("button", { name: "连接 Codex（只读） Codex 后端" });
    fireEvent.click(connect);

    await waitFor(() => expect(api.connect).toHaveBeenCalledExactlyOnceWith());
    expect(await screen.findByText(/Codex app-server 未连接：Codex app-server unavailable/)).toBeVisible();
  });

  it("keeps an AUTH_REQUIRED browser stopped until the owner's explicit completion action", async () => {
    api.workspaceSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    api.workstreamSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    api.hostEnvironmentStatus.mockResolvedValue({ host: "ONLINE", mobile: "AVAILABLE", browserRuntime: "OFFICIAL_NON_BRANDED_CHROMIUM", chatgptBrowserMode: "AUTH_REQUIRED" });
    api.confirmChatGptAuthenticationCompleted.mockResolvedValue(undefined);
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    const completion = await screen.findByRole("button", { name: "已登录，继续" });
    expect(screen.getByText(/这不代表当前未登录/)).toBeVisible();
    expect(screen.queryByText("需要你完成登录或验证")).toBeNull();
    expect(api.confirmChatGptAuthenticationCompleted).not.toHaveBeenCalled();
    expect(api.openBoundChatGptConversation).not.toHaveBeenCalled();
    expect(api.sendChatGptRequest).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "打开或显示 Router 浏览器" }));
    await waitFor(() => expect(api.openHostChatGptBrowserSetup).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(completion).toBeEnabled());
    expect(screen.getByText(/已显示 Router 的 Chromium 窗口/)).toBeVisible();
    expect(api.confirmChatGptAuthenticationCompleted).not.toHaveBeenCalled();
    expect(api.openBoundChatGptConversation).not.toHaveBeenCalled();
    fireEvent.click(completion);
    await waitFor(() => expect(api.confirmChatGptAuthenticationCompleted).toHaveBeenCalledTimes(1));
  });

  it("explains Windows foreground denial while keeping recovery explicit and allowing another display attempt", async () => {
    api.workspaceSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    api.workstreamSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    api.hostEnvironmentStatus.mockResolvedValue({ host: "ONLINE", mobile: "AVAILABLE", browserRuntime: "OFFICIAL_NON_BRANDED_CHROMIUM", chatgptBrowserMode: "AUTH_REQUIRED" });
    api.openHostChatGptBrowserSetup.mockRejectedValueOnce("SECURITY_BROWSER_FOCUS_BLOCKED").mockResolvedValue(undefined);
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    const open = await screen.findByRole("button", { name: "打开或显示 Router 浏览器" });
    fireEvent.click(open);
    expect(await screen.findByRole("alert")).toHaveTextContent("Windows 阻止了切换到前台");
    expect(screen.queryByText(/已显示 Router 的 Chromium 窗口/)).toBeNull();
    expect(api.confirmChatGptAuthenticationCompleted).not.toHaveBeenCalled();
    await waitFor(() => expect(open).toBeEnabled());
    fireEvent.click(open);
    expect(await screen.findByText(/已显示 Router 的 Chromium 窗口/)).toBeVisible();
    expect(api.openHostChatGptBrowserSetup).toHaveBeenCalledTimes(2);
    expect(api.confirmChatGptAuthenticationCompleted).not.toHaveBeenCalled();
    expect(api.sendChatGptRequest).not.toHaveBeenCalled();
  });

  it("keeps recovery actions locked during open and preserves the stop after failed completion", async () => {
    api.workspaceSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    api.workstreamSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    api.hostEnvironmentStatus.mockResolvedValue({ host: "ONLINE", mobile: "AVAILABLE", browserRuntime: "OFFICIAL_NON_BRANDED_CHROMIUM", chatgptBrowserMode: "AUTH_REQUIRED" });
    let finishOpen!: () => void;
    api.openHostChatGptBrowserSetup.mockImplementation(() => new Promise<void>(resolve => { finishOpen = resolve; }));
    api.confirmChatGptAuthenticationCompleted.mockRejectedValue("SECURITY_BROWSER_CLOSE_PENDING");
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    const open = await screen.findByRole("button", { name: "打开或显示 Router 浏览器" });
    fireEvent.click(open);
    fireEvent.click(open);
    expect(api.openHostChatGptBrowserSetup).toHaveBeenCalledTimes(1);
    const complete = screen.getByRole("button", { name: "已登录，继续" });
    expect(complete).toBeDisabled();
    fireEvent.click(complete);
    expect(api.confirmChatGptAuthenticationCompleted).not.toHaveBeenCalled();
    await act(async () => finishOpen());
    await waitFor(() => expect(complete).toBeEnabled());
    fireEvent.click(complete);
    expect(await screen.findByText(/验证窗口尚未安全结束/)).toBeVisible();
    expect(screen.getByRole("button", { name: "打开或显示 Router 浏览器" })).toBeVisible();
    expect(api.openBoundChatGptConversation).not.toHaveBeenCalled();
    expect(api.sendChatGptRequest).not.toHaveBeenCalled();
  });

  it("shows the bundled browser and exact reply check without extension setup", async () => {
    api.workspaceSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    api.workstreamSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "运行环境" }));
    expect(await screen.findByText("AI Work Router Browser")).toBeVisible();
    expect(screen.queryByText(/chrome:\/\/extensions/)).toBeNull();
    expect(screen.queryByRole("button", { name: "复制加载路径" })).toBeNull();
    expect(screen.getByRole("button", { name: "检查 ChatGPT 新回复 ChatGPT 新回复" })).toBeVisible();
  });

  it("refreshes browser status without navigating, starting or submitting a provider operation", async () => {
    api.workspaceSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    api.workstreamSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "运行环境" }));
    fireEvent.click(screen.getByRole("button", { name: "检查浏览器状态 AI Work Router Browser" }));
    await waitFor(() => expect(api.hostEnvironmentStatus.mock.calls.length).toBeGreaterThanOrEqual(2));
    expect(api.openHostChatGptBrowserSetup).not.toHaveBeenCalled();
    expect(api.openBoundChatGptConversation).not.toHaveBeenCalled();
    expect(api.connectNormalChromeReadOnly).not.toHaveBeenCalled();
    expect(api.sendChatGptRequest).not.toHaveBeenCalled();
  });

  it("presents one Bridge with the two exact binding labels", async () => {
    api.workspaceSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    api.workstreamSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    const bridge = await screen.findByRole("region", { name: "A 工作区 Bridge 两端状态" });
    expect(bridge).toHaveTextContent("ChatGPT conversation");
    expect(bridge).toHaveTextContent("当前 Codex 主对话");
    expect(api.openBoundChatGptInDefaultBrowser).not.toHaveBeenCalled();
    expect(api.confirmExplicitChatGptEndpointBinding).not.toHaveBeenCalled();
  });

  it("checks a bound ChatGPT reply through Router without the default browser or legacy observer", async () => {
    api.workspaceSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    api.workstreamSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    api.checkNewChatGptReplies.mockResolvedValue({ state: "NO_NEW_TERMINAL_REPLY", observationCreated: false });
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "运行环境" }));
    fireEvent.click(screen.getByRole("button", { name: "检查 ChatGPT 新回复 ChatGPT 新回复" }));
    await waitFor(() => expect(api.checkNewChatGptReplies).toHaveBeenCalledExactlyOnceWith("work-a"));
    expect(api.connectNormalChromeReadOnly).not.toHaveBeenCalled();
    expect(api.openBoundChatGptInDefaultBrowser).not.toHaveBeenCalled();
    expect(api.sendChatGptRequest).not.toHaveBeenCalled();
  });

  it("does not duplicate thread creation or move a late candidate to a different Workstream", async () => {
    api.listExternalProjectLinks.mockResolvedValue([linkedChatGptProject]);
    let resolveThread!: (value: { thread: { id: string }; directory: string }) => void;
    api.startUnprojectedCodexThread.mockImplementationOnce(() => new Promise((resolve) => { resolveThread = resolve; }));
    api.workstreamSnapshot.mockImplementation(async (id) => ({ ...snapshot, selectedWorkstreamId: id }));
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "项目" }));
    fireEvent.click(screen.getByRole("button", { name: "新建无项目 Codex 对话" }));
    const create = screen.getByRole("button", { name: "创建无项目对话" });
    fireEvent.click(create);
    fireEvent.click(create);
    expect(api.startUnprojectedCodexThread).toHaveBeenCalledOnce();
    expect(create).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "B 工作区" }));
    await act(async () => resolveThread({ thread: { id: "late-thread-a" }, directory: "D:/scratch-a" }));
    fireEvent.click(screen.getByRole("button", { name: "项目" }));
    fireEvent.click(screen.getByRole("button", { name: "新建无项目 Codex 对话" }));
    expect(screen.queryByRole("button", { name: "核对并绑定此 Codex 对话" })).toBeNull();
    expect(screen.getByLabelText("工作目录")).toHaveValue("");
    expect(api.pairWorkstreamEndpoints).not.toHaveBeenCalled();
  });

  it("does not infer desktop no-project durability from the returned thread ID", async () => {
    api.listExternalProjectLinks.mockResolvedValue([linkedChatGptProject]);
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "项目" }));
    fireEvent.click(screen.getByRole("button", { name: "新建无项目 Codex 对话" }));
    fireEvent.click(screen.getByRole("button", { name: "创建无项目对话" }));
    expect(await screen.findByText("首次有效 Turn 前不要把它视为持久会话。", { exact: false })).toBeVisible();
    expect(screen.getByText("已通过 Codex `thread/read` 验证，并已带入本次配对候选；返回后仍需核对并确认，当前 Endpoint 不会自动改变。")).toBeVisible();
    expect(screen.queryByText("已按 Provider 返回结果确认可持久重开", { exact: false })).toBeNull();
    expect(screen.getByLabelText("工作目录")).toHaveValue("D:/router-scratch");
    expect(screen.getByRole("button", { name: "创建无项目对话" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "核对并绑定此 Codex 对话" }));
    expect(screen.getByRole("heading", { name: "确认这一组对话" })).toBeVisible();
    expect(api.pairWorkstreamEndpoints).not.toHaveBeenCalled();
  });

  it("returns a forward review to its exact ChatGPT observation without opening a provider page", async () => {
    api.workspaceSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    api.workstreamSnapshot.mockResolvedValue(exactConfirmedSnapshot);
    api.replyObservations.mockResolvedValue([{ id: "chatgpt-exact", endpointId: "endpoint-chatgpt-a", text: "这条 ChatGPT 原回复必须可回看", observedAt: 20, pushState: "SENT" }]);
    api.reviewWorkstreamResults.mockResolvedValue([{ runId: "historical-chatgpt-run", provider: "CHATGPT", resultIdentity: "chatgpt-exact", text: "同 ID 的历史结果绝不能替代外部回复", markerText: "历史结果候选", attachments: [] }]);
    render(<UnifiedWorkbenchHost initialSurface="WORKSPACE" />);

    expect(await screen.findByText("这条 ChatGPT 原回复必须可回看")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "选择/编辑交给 Codex 的范围" }));
    fireEvent.change(screen.getByRole("textbox", { name: "交接指令范围" }), { target: { value: "只交接这条精确回复" } });
    fireEvent.click(screen.getByRole("button", { name: "确认范围，进入审阅" }));
    await waitFor(() => expect(api.prepareChatGptToCodexHandoff).toHaveBeenCalledWith("work-a", "chatgpt-exact", "只交接这条精确回复"));
    expect(await screen.findByText("精确 Router 来源记录：")).toHaveTextContent("chatgpt-exact");
    expect(screen.getByText("已选精确 ChatGPT 外部观察")).toBeVisible();
    expect(screen.queryByText("历史结果候选")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "批准此版本（不会发送）" }));
    await waitFor(() => expect(api.approveChatGptToCodexHandoff).toHaveBeenCalledWith("chatgpt-observation-review", 1, "可编辑的 ChatGPT 转交"));
    expect(api.sendChatGptToCodexHandoffReview).not.toHaveBeenCalled();
    expect(await screen.findByText("精确 Router 来源记录：")).toHaveTextContent("chatgpt-exact");
    fireEvent.click(await screen.findByRole("button", { name: "单独发送给 Codex" }));
    await waitFor(() => expect(api.sendChatGptToCodexHandoffReview).toHaveBeenCalledWith("chatgpt-observation-review", 2));
    expect(await screen.findByText("已发送，执行状态独立")).toBeVisible();
    fireEvent.click(await screen.findByRole("button", { name: "查看 ChatGPT 完整原回复" }));
    expect(await screen.findByText("这条 ChatGPT 原回复必须可回看")).toBeVisible();
    expect(api.openBoundChatGptConversation).not.toHaveBeenCalled();
    expect(api.readActiveChatGptProviderLatestSnapshot).not.toHaveBeenCalled();
  });
});

// These fixtures exercise retained host protocol projections through their historical
// composition. They are not Native R2 UI acceptance; native flow checks are separate.
vi.mock("../features/workbench/UnifiedWorkbench",async(importOriginal)=>{
 const actual=await importOriginal<typeof import("../features/workbench/UnifiedWorkbench")>();
 return {...actual,LEGACY_WORKBENCH_DETAILS:true,UnifiedWorkbench:(props:import("../features/workbench/UnifiedWorkbench").UnifiedWorkbenchProps)=>{composition.last=props;return <actual.LegacyWorkbench {...props}/>;}};
});

it("opens the newly created exact Bridge even when the previous render's index lacked its ID",async()=>{render(<UnifiedWorkbenchHost initialSurface="BRIDGES"/>);await waitFor(()=>expect(composition.last?.items.some(item=>item.id==="work-a")).toBe(true));const create=composition.last!.onCreateBridge!;const row={...workstreams[0],id:"work-new-exact",name:"new owner Bridge"};const fresh={...snapshot,workstreams:[...workstreams,row],selectedWorkstreamId:row.id};api.createBridge.mockResolvedValue(row.id);api.workspaceSnapshot.mockResolvedValue(fresh);api.workstreamSnapshot.mockImplementation(async(id:string)=>id===row.id?fresh:snapshot);await act(async()=>{await create("new owner Bridge");});expect(api.selectWorkspace).toHaveBeenCalledWith("project-1",row.id);expect(api.workstreamSnapshot).toHaveBeenCalledWith(row.id);await waitFor(()=>expect(composition.last?.selectedWorkstreamId).toBe(row.id));expect(composition.last?.surface).toBe("WORKSPACE");});

it("lists and opens an exact Bridge from another project without changing its bound conversation",async()=>{const other={...workstreams[0],id:"other-project-exact",projectId:"other-project",name:"Other project Bridge"};api.dashboardProjection.mockResolvedValue({workstreams:[{workstream:other,projectId:other.projectId,projectName:"Other project",attentionItems:[],chatgptEndpoint:null,codexEndpoint:null}],attentionItems:[]});render(<UnifiedWorkbenchHost initialSurface="BRIDGES"/>);await waitFor(()=>expect(composition.last?.items.some(item=>item.id===other.id)).toBe(true));const select=composition.last!.onSelectWorkstream;await act(async()=>{select(other.id);});await waitFor(()=>expect(api.selectWorkspace).toHaveBeenCalledWith(other.projectId,other.id));expect(api.pairWorkstreamEndpoints).not.toHaveBeenCalled();});
