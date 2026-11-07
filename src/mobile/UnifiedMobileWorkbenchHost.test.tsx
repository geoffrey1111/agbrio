import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const composition = vi.hoisted(()=>({native:false}));

const api = vi.hoisted(() => ({
  sendChatGptDiscussion: vi.fn(), selectHandoffAttachments: vi.fn(), confirmChatGptAuthenticationCompleted: vi.fn(),
  pushStatus: vi.fn(),
  testPush: vi.fn(),
  testReplyPush: vi.fn(),
  workstreams: vi.fn(), workstream: vi.fn(), codexHistory: vi.fn(), codexGoal: vi.fn(),
  workstreamDraft: vi.fn(), replyObservations: vi.fn(), saveWorkstreamDraft: vi.fn(), codexFeedbackDraft: vi.fn(), saveCodexFeedbackDraft: vi.fn(),
  pairWorkstreamEndpoints: vi.fn(), prepareOwnerConfirmedChatGptEndpointBinding: vi.fn(), confirmExplicitChatGptEndpointBinding: vi.fn(), reviewResults: vi.fn(), providerRunStatus: vi.fn(), projectLinks: vi.fn(), chatGptProjectDirectory: vi.fn(), openHostChatGptBrowserSetup: vi.fn(),
  upsertProjectLink: vi.fn(), startUnprojectedCodexThread: vi.fn(), createVerifiedLocalBackup: vi.fn(),
  sendCodexFeedback: vi.fn(), codexRequests: vi.fn(), respondToCodexRequest: vi.fn(), prepareHandoff: vi.fn(), approveHandoff: vi.fn(), prepareCodexHandoff: vi.fn(), approveCodexHandoff: vi.fn(), codexHandoffReview: vi.fn(), codexHandoffManualDestination: vi.fn(), manualChatGptDiscussionDestination: vi.fn(), chatGptHandoffReview: vi.fn(), sendCodexHandoff: vi.fn(),
  checkNewChatGptReplies: vi.fn(), checkNewCodexReplies: vi.fn(),
}));
const push = vi.hoisted(() => ({
  disableWebPush: vi.fn(),
  enableWebPush: vi.fn(),
  pushSetupState: vi.fn(),
}));
vi.mock("./api",()=>({mobileApi:api,request:vi.fn(async(path:string)=>{
 if(path.includes("/delivery"))return {account:"",recipient:"",hasSecret:false,windowsStatus:"UNAVAILABLE",channels:[],history:[]};
 if(path.includes("/events"))return {events:[],nextCursor:0,hasMore:false};
 if(path==="/codex-watches")return [];
 throw Error("fixture route unavailable");
})}));
vi.mock("./push", () => push);
import { UnifiedMobileWorkbenchHost } from "./UnifiedMobileWorkbenchHost";

const workstreams = [
  { id: "work-a", name: "A 工作区", attentionCount: 0, attentionItems: [] },
  { id: "work-b", name: "B 工作区", attentionCount: 0, attentionItems: [] },
];
const linkedChatGptProject = { id: "link-1", projectId: "project-1", provider: "CHATGPT", externalProjectId: "g-persisted", canonicalUrl: "https://chatgpt.com/g/g-persisted/project", label: "已保存 ChatGPT 项目", sourceKind: "test-persisted-project-link", createdAt: 1, updatedAt: 1 };

function snapshot(id: string) {
  return { selectedWorkstreamId: id, activeChatgptEndpoint: null, activeCodexEndpoint: null, projects: [{ id: "project-1", name: "项目" }], workstreams: workstreams.map((item) => ({ id: item.id, projectId: "project-1", name: item.name, status: "ACTIVE", updatedAt: 1, bindingRevision: id === item.id ? 3 : 2 })) };
}

const confirmedChatGptEndpoint = { id: "endpoint-chatgpt-a", workstreamId: "work-a", provider: "CHATGPT" as const, externalId: "12345678-abcd", label: "已验证的精确对话", status: "ACTIVE", createdAt: 1 };
function activeChatGptSnapshot(id: string, externalId = confirmedChatGptEndpoint.externalId) {
  return { ...snapshot(id), activeChatgptEndpoint: { ...confirmedChatGptEndpoint, externalId }, activeCodexEndpoint: { id: "endpoint-codex-a", workstreamId: "work-a", provider: "CODEX" as const, externalId: "thread-codex-a", label: "Codex", status: "ACTIVE", createdAt: 1 } };
}

function useCompactMobileViewport() {
  vi.stubGlobal("matchMedia", vi.fn().mockImplementation(() => ({
    matches: true, media: "(max-width: 680px)", onchange: null,
    addListener: vi.fn(), removeListener: vi.fn(), addEventListener: vi.fn(), removeEventListener: vi.fn(), dispatchEvent: vi.fn(),
  })));
}


async function enterAdvancedConnection() {
  await waitFor(() => {
    const entry=screen.queryByRole("button",{name:"项目链接与高级连接 ›"});
    if(entry){fireEvent.click(entry);return;}
    if(screen.queryByLabelText("具体 ChatGPT 对话链接") || screen.queryByRole("heading",{name:"当前对话已绑定"})) return;
    throw new Error("Waiting for connection entry");
  });
}

beforeEach(() => {
  composition.native=false;
  push.pushSetupState.mockResolvedValue("NOT_SUBSCRIBED");
  api.pushStatus.mockResolvedValue({ activeSubscriptionCount: 0 });
  api.testPush.mockResolvedValue(undefined);
  api.testReplyPush.mockResolvedValue(undefined);
  api.workstreams.mockResolvedValue(workstreams);
  api.workstream.mockImplementation(async (id: string) => snapshot(id));
  api.codexHistory.mockResolvedValue({ history: [] });
  api.codexGoal.mockResolvedValue(null);
  api.workstreamDraft.mockResolvedValue(null);
  api.codexFeedbackDraft.mockResolvedValue(null);
  api.reviewResults.mockResolvedValue([]);
  api.providerRunStatus.mockResolvedValue({ runId: "run-target", provider: "CODEX", status: "FAILED", terminalCode: "TEST_FAILURE", startedAt: 5, terminalAt: 9, updatedAt: 10, hasReviewableResult: false });
  api.projectLinks.mockResolvedValue([]);
  api.chatGptProjectDirectory.mockResolvedValue({ projectUrl: "https://chatgpt.com/g/g-persisted/project", conversations: [], completeness: "EMPTY", hasMore: false, sourceKind: "PROJECT_CONTAINER" });
  api.openHostChatGptBrowserSetup.mockResolvedValue(undefined);
  api.prepareOwnerConfirmedChatGptEndpointBinding.mockResolvedValue({ workstreamId: "work-a", externalId: "12345678-abcd", label: "已在默认浏览器核对的精确对话", verification: "OWNER_CONFIRMED_EXACT_URL", expectedOldEndpointId: null, expectedBindingRevision: 3 });
  api.confirmExplicitChatGptEndpointBinding.mockResolvedValue(confirmedChatGptEndpoint);
  api.saveWorkstreamDraft.mockResolvedValue({ workstreamId: "work-a", text: "", revision: 1, updatedAt: 1 });
  api.saveCodexFeedbackDraft.mockResolvedValue({ workstreamId: "work-a", sourceRunId: "result-codex-a", text: "", revision: 1, updatedAt: 1 });
  api.sendCodexFeedback.mockResolvedValue({ turn_id: "turn-feedback" });
  api.codexRequests.mockResolvedValue([]);
  api.respondToCodexRequest.mockResolvedValue(undefined);
  api.prepareHandoff.mockResolvedValue({ actionId: "chatgpt-observation-review", revision: 1, status: "READY", message: "可编辑的 ChatGPT 转交" });
  api.approveHandoff.mockResolvedValue({ actionId: "chatgpt-observation-review", revision: 2, status: "APPROVED", message: "明确选定的范围" });
  api.prepareCodexHandoff.mockResolvedValue({ actionId: "codex-observation-review", revision: 1, status: "READY", message: "可编辑的 Codex 回传" });
  api.approveCodexHandoff.mockResolvedValue({ actionId: "codex-observation-review", revision: 2, status: "APPROVED", message: "可编辑的 Codex 回传" });
  api.codexHandoffReview.mockResolvedValue({ actionId: "codex-observation-review", revision: 1, status: "READY", message: "可编辑的 Codex 回传" });
  api.codexHandoffManualDestination.mockResolvedValue({ canonicalUrl: "https://chatgpt.com/c/12345678-abcd", conversationId: "12345678-abcd" });
  api.manualChatGptDiscussionDestination.mockResolvedValue({ canonicalUrl: "https://chatgpt.com/c/12345678-abcd", conversationId: "12345678-abcd" });
  api.chatGptHandoffReview.mockRejectedValue(new Error("The mobile Codex Handoff review is no longer available"));
  api.sendCodexHandoff.mockResolvedValue({ status: "SENT", detail: "测试送达" });
  api.checkNewChatGptReplies.mockResolvedValue({ state: "NO_NEW_TERMINAL_REPLY", observationCreated: false });
  api.checkNewCodexReplies.mockResolvedValue({ state: "NO_NEW_TERMINAL_REPLY", observationCreated: false });
  api.replyObservations.mockImplementation(async (id: string) => id === "work-a" ? [
    { id: "reply-new", endpointId: "endpoint-chatgpt-a", text: "latest reply must not replace target", observedAt: 20, pushState: "SENT" },
    { id: "reply-old", endpointId: "endpoint-chatgpt-a", text: "targeted reply", observedAt: 10, pushState: "SENT" },
  ] : []);
});

it("opens the active phone notification controls through Native R2 Settings", async () => {
  composition.native=true;useCompactMobileViewport();
  api.workstream.mockResolvedValue(activeChatGptSnapshot("work-a"));
  push.pushSetupState.mockResolvedValue("SUBSCRIBED");api.pushStatus.mockResolvedValue({activeSubscriptionCount:1});
  render(<UnifiedMobileWorkbenchHost initialSurface="SETTINGS"/>);
  fireEvent.click(await screen.findByRole("button",{name:/^通知提醒渠道/}));
  expect(await screen.findByRole("button",{name:"发送测试通知"})).toBeVisible();
  fireEvent.click(screen.getByRole("button",{name:"测试真实回复通知"}));await waitFor(()=>expect(api.testReplyPush).toHaveBeenCalledTimes(1));
});

it("tells the owner when Cloudflare Access authentication is required instead of claiming that the Windows Router is off", async () => {
  api.workstreams.mockRejectedValue(Object.assign(new Error("Cloudflare Access authentication is required (Router HTTP 401)"), { status: 401 }));
  render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);

  expect(await screen.findByRole("heading", { name: "需要重新登录手机访问" })).toBeVisible();
  expect(screen.getByText("这不表示电脑上的 Router 已关闭。请不要反复刷新。")).toBeVisible();
  expect(screen.getByRole("button", { name: "重新打开手机访问页" })).toBeVisible();
  expect(screen.queryByText("在 Windows 上检查 AI Work Router")).toBeNull();
});






























afterEach(() => { cleanup(); vi.clearAllMocks(); vi.useRealTimers(); vi.unstubAllGlobals(); window.history.replaceState({}, "", "/"); });

async function openPreparedChatGptCandidate() {
  fireEvent.click(await screen.findByRole("button", { name: "项目" }));
    await enterAdvancedConnection();
  const url = await screen.findByLabelText("具体 ChatGPT 对话链接");
  fireEvent.change(url, { target: { value: "https://chatgpt.com/g/g-project/c/12345678-abcd" } });
  fireEvent.click(screen.getByRole("button", { name: "我已在默认浏览器核对，准备绑定" }));
  await screen.findByRole("button", { name: "确认绑定" });
}

describe("UnifiedMobileWorkbenchHost notification and pairing scope", () => {
  it("uses the mobile unavailable shell for an initial Router read failure and retries without inventing a computer state", async () => {
    api.workstreams.mockRejectedValueOnce(new Error("network unavailable"));
    render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);
    const unavailable = await screen.findByRole("main", { name: "手机端运行状态不可用" });
    expect(unavailable).toHaveTextContent("无法读取 Router 状态");
    expect(unavailable).toHaveTextContent("Router 状态请求未成功");
    expect(unavailable).not.toHaveTextContent("network unavailable");
    expect(unavailable).not.toHaveTextContent("电脑端可能未运行");
    fireEvent.click(screen.getByRole("button", { name: "重新连接" }));
    await waitFor(() => expect(api.workstreams).toHaveBeenCalledTimes(2));
  });



























});



it("uses global lifecycle and human conversation labels for phone Bridges missing from the focused snapshot",async()=>{
 composition.native=true;useCompactMobileViewport();
 api.workstreams.mockResolvedValue([{...workstreams[0],status:"ACTIVE",projectName:"当前项目",sourceLabel:"Codex · 当前控制对话",updatedAt:10},{...workstreams[1],status:"ACTIVE",trashedAt:11,projectName:"其他项目",sourceLabel:"已删除的对话",updatedAt:11}]);
 api.workstream.mockResolvedValue({...snapshot("work-a"),workstreams:[snapshot("work-a").workstreams[0]]});
 render(<UnifiedMobileWorkbenchHost initialSurface="BRIDGES"/>);
 expect(await screen.findByText("Bridge · Codex · 当前控制对话")).toBeVisible();
 expect(screen.queryByText("B 工作区")).toBeNull();
});

it("opens an exact Bridge notification from the default directory and returns to the directory",async()=>{
 composition.native=true;useCompactMobileViewport();window.history.replaceState({},"","/mobile?workstream=work-b&reply=notification-qa");
 render(<UnifiedMobileWorkbenchHost/>);await waitFor(()=>expect(api.workstream).toHaveBeenCalledWith("work-b"));
 expect(await screen.findByRole("heading",{name:"B 工作区"})).toBeVisible();expect(screen.queryByRole("heading",{name:"对话"})).not.toBeInTheDocument();
 fireEvent.click(screen.getByRole("button",{name:"返回"}));expect(await screen.findByRole("heading",{name:"对话"})).toBeVisible();
});
it("handles a warm notification intent without falling back to the first Bridge",async()=>{
 composition.native=true;useCompactMobileViewport();let receive!:(event:MessageEvent)=>void;
 vi.stubGlobal("navigator",{...navigator,serviceWorker:{addEventListener:vi.fn((name,handler)=>{if(name==="message")receive=handler;}),removeEventListener:vi.fn()}});
 render(<UnifiedMobileWorkbenchHost/>);await screen.findByRole("heading",{name:/^(对话|Chats)$/});await waitFor(()=>expect(receive).toBeDefined());const ack=vi.fn();
 act(()=>receive({data:{type:"AGBRIO_OPEN_BRIDGE",target:"/mobile?workstream=work-b&reply=warm-qa"},origin:window.location.origin,ports:[{postMessage:ack}]} as unknown as MessageEvent));
 await waitFor(()=>expect(api.workstream).toHaveBeenCalledWith("work-b"));expect(await screen.findByRole("heading",{name:"B 工作区"})).toBeVisible();expect(ack).toHaveBeenCalledWith({accepted:true});
});
it("rejects a missing notification Bridge without reading the first available one",async()=>{
 composition.native=true;useCompactMobileViewport();window.history.replaceState({},"","/mobile?workstream=missing-qa");render(<UnifiedMobileWorkbenchHost/>);
 await screen.findByText("通知指定的工作区当前不可用，未回退到其他工作区。");expect(api.workstream).not.toHaveBeenCalled();
});
it("ignores an older index response after a newer Bridge notification arrives",async()=>{
 composition.native=true;useCompactMobileViewport();window.history.replaceState({},"","/mobile?workstream=work-a");let complete!:(value:typeof workstreams)=>void,receive!:(e:MessageEvent)=>void;
 api.workstreams.mockImplementationOnce(()=>new Promise(resolve=>{complete=resolve;}));
 vi.stubGlobal("navigator",{...navigator,serviceWorker:{addEventListener:vi.fn((name,handler)=>{if(name==="message")receive=handler;}),removeEventListener:vi.fn()}});
 render(<UnifiedMobileWorkbenchHost/>);await waitFor(()=>expect(receive).toBeDefined());
 act(()=>receive({data:{type:"AGBRIO_OPEN_BRIDGE",target:"/mobile?workstream=work-b"},origin:window.location.origin} as MessageEvent));
 await screen.findByRole("heading",{name:"B 工作区"});await act(async()=>complete(workstreams));expect(screen.getByRole("heading",{name:"B 工作区"})).toBeVisible();expect(api.workstream).not.toHaveBeenCalledWith("work-a");
});
it("does not replace a new notification reader with an old snapshot finishing late",async()=>{
 composition.native=true;useCompactMobileViewport();let complete!:(value:ReturnType<typeof snapshot>)=>void,receive!:(e:MessageEvent)=>void;
 api.workstream.mockImplementation(async(id:string)=>id==="work-a"?new Promise(resolve=>{complete=resolve;}):snapshot(id));
 vi.stubGlobal("navigator",{...navigator,serviceWorker:{addEventListener:vi.fn((name,handler)=>{if(name==="message")receive=handler;}),removeEventListener:vi.fn()}});
 render(<UnifiedMobileWorkbenchHost/>);await waitFor(()=>expect(api.workstream).toHaveBeenCalledWith("work-a"));
 act(()=>receive({data:{type:"AGBRIO_OPEN_BRIDGE",target:"/mobile?workstream=work-b"},origin:window.location.origin} as MessageEvent));await screen.findByRole("heading",{name:"B 工作区"});
 await act(async()=>complete(snapshot("work-a")));expect(screen.getByRole("heading",{name:"B 工作区"})).toBeVisible();expect(screen.queryByRole("heading",{name:"A 工作区"})).not.toBeInTheDocument();
});
