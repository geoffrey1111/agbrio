import { StrictMode,useLayoutEffect } from "react";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const composition=vi.hoisted(()=>({selectDuringLayout:null as string|null,last:null as import("../features/workbench/UnifiedWorkbench").UnifiedWorkbenchProps|null}));
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
  composition.selectDuringLayout=null;
    composition.last=null;
  composition.last=null;
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




});

vi.mock("../features/workbench/UnifiedWorkbench",async(importOriginal)=>{
 const actual=await importOriginal<typeof import("../features/workbench/UnifiedWorkbench")>();
 return {...actual,UnifiedWorkbench:(props:import("../features/workbench/UnifiedWorkbench").UnifiedWorkbenchProps)=>{composition.last=props;const scheduled=composition.selectDuringLayout;useLayoutEffect(()=>{if(scheduled&&props.selectedWorkstreamId){composition.selectDuringLayout=null;props.onSelectWorkstream(scheduled);}},[scheduled,props.selectedWorkstreamId]);return <actual.UnifiedWorkbench {...props}/>;}};
});

it("opens the newly created exact Bridge even when the previous render's index lacked its ID",async()=>{render(<UnifiedWorkbenchHost initialSurface="BRIDGES"/>);await waitFor(()=>expect(composition.last?.items.some(item=>item.id==="work-a")).toBe(true));const create=composition.last!.onCreateBridge!;const row={...workstreams[0],id:"work-new-exact",name:"new owner Bridge"};const fresh={...snapshot,workstreams:[...workstreams,row],selectedWorkstreamId:row.id};api.createBridge.mockResolvedValue(row.id);api.workspaceSnapshot.mockResolvedValue(fresh);api.workstreamSnapshot.mockImplementation(async(id:string)=>id===row.id?fresh:snapshot);await act(async()=>{await create("new owner Bridge");});expect(api.selectWorkspace).toHaveBeenCalledWith("project-1",row.id);expect(api.workstreamSnapshot).toHaveBeenCalledWith(row.id);await waitFor(()=>expect(composition.last?.selectedWorkstreamId).toBe(row.id));expect(composition.last?.surface).toBe("WORKSPACE");});

it("lists and opens an exact Bridge from another project without changing its bound conversation",async()=>{const other={...workstreams[0],id:"other-project-exact",projectId:"other-project",name:"Other project Bridge"};api.dashboardProjection.mockResolvedValue({workstreams:[{workstream:other,projectId:other.projectId,projectName:"Other project",attentionItems:[],chatgptEndpoint:null,codexEndpoint:null}],attentionItems:[]});render(<UnifiedWorkbenchHost initialSurface="BRIDGES"/>);await waitFor(()=>expect(composition.last?.items.some(item=>item.id===other.id)).toBe(true));const select=composition.last!.onSelectWorkstream;await act(async()=>{select(other.id);});await waitFor(()=>expect(api.selectWorkspace).toHaveBeenCalledWith(other.projectId,other.id));expect(api.pairWorkstreamEndpoints).not.toHaveBeenCalled();});

it("selects the new reader while presentation persistence is pending",async()=>{
 render(<UnifiedWorkbenchHost initialSurface="BRIDGES"/>);await waitFor(()=>expect(composition.last?.items.some(item=>item.id==="work-b")).toBe(true));
 let finish!:()=>void;api.selectWorkspace.mockImplementationOnce(()=>new Promise(resolve=>{finish=()=>resolve(snapshot);}));
 act(()=>{composition.last!.onSelectWorkstream("work-b");});
 await waitFor(()=>expect(composition.last?.selectedWorkstreamId).toBe("work-b"));expect(composition.last?.surface).toBe("WORKSPACE");
 await act(async()=>finish());
});

it("keeps a newer selection when the previous commit still has a pending reader effect",async()=>{
 composition.selectDuringLayout="work-b";render(<UnifiedWorkbenchHost initialSurface="BRIDGES"/>);
 await waitFor(()=>expect(api.selectWorkspace).toHaveBeenCalledWith("project-1","work-b"));
 await waitFor(()=>expect(composition.last?.selectedWorkstreamId).toBe("work-b"));
});
