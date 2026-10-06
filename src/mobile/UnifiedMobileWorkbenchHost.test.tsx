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
  fireEvent.click(await screen.findByRole("button",{name:"消息通知"}));
  expect(await screen.findByRole("button",{name:"发送测试通知"})).toBeVisible();
  fireEvent.click(screen.getByRole("button",{name:"测试真实回复通知"}));await waitFor(()=>expect(api.testReplyPush).toHaveBeenCalledTimes(1));
});

it("tells the owner when Cloudflare Access authentication is required instead of claiming that the Windows Router is off", async () => {
  api.workstreams.mockRejectedValue(Object.assign(new Error("Cloudflare Access authentication is required (Router HTTP 401)"), { status: 401 }));
  render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);

  expect(await screen.findByRole("heading", { name: "需要重新登录手机访问" })).toBeVisible();
  expect(screen.getByText("这不是 ChatGPT 登录，也不表示电脑上的 Router 已关闭。请不要反复刷新。")).toBeVisible();
  expect(screen.getByRole("button", { name: "重新打开手机访问页" })).toBeVisible();
  expect(screen.queryByText("在 Windows 上检查 AI Work Router")).toBeNull();
});

it("keeps a current exact Codex observation visible on mobile and prepares its reverse review", async () => {
  api.workstream.mockResolvedValue(activeChatGptSnapshot("work-a"));
  api.replyObservations.mockResolvedValue([{
    id: "codex-observation", endpointId: "endpoint-codex-a", text: "Codex 已完成的外部回复", observedAt: 20, pushState: "SENT",
  }]);
  render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);

  expect(await screen.findByText("Codex 已完成的外部回复")).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "审阅这条 Codex 回复" }));
  await waitFor(() => expect(api.prepareCodexHandoff).toHaveBeenCalledWith("work-a", "codex-observation", []));
  expect(await screen.findByRole("heading", { name: "审阅并批准后，单独发送给 ChatGPT" })).toBeVisible();
  expect(screen.getByText("来源：选定的精确 Codex 外部回复。下面是可编辑的转交草稿；此页尚未向 ChatGPT 发送任何内容。")).toBeVisible();
  expect(screen.getByRole("button", { name: "批准此版本（不会发送）" })).toBeVisible();
});

it("requires explicit attachment selection for an exact external Codex observation", async () => {
  api.workstream.mockResolvedValue(activeChatGptSnapshot("work-a"));
  api.replyObservations.mockResolvedValue([{
    id: "codex-files", endpointId: "endpoint-codex-a", text: "Codex external files", observedAt: 20, pushState: "SENT",
    attachments: [
      { id: "file-valid", filename: "selected.txt", integrityStatus: "VERIFIED", defaultSelected: true, warnings: [] },
      { id: "file-bad", filename: "changed.txt", integrityStatus: "MISMATCH", defaultSelected: false, warnings: ["hash mismatch"] },
    ],
  }]);
  render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);
  fireEvent.click(await screen.findByRole("button", { name: "审阅这条 Codex 回复" }));
  const selected = await screen.findByRole("checkbox", { name: /selected.txt/ });
  expect(selected).not.toBeChecked();
  expect(screen.getByRole("checkbox", { name: /changed.txt/ })).toBeDisabled();
  expect(api.prepareCodexHandoff).not.toHaveBeenCalled();
  fireEvent.click(selected);
  fireEvent.click(screen.getByRole("button", { name: "审阅此回复，准备转发给 ChatGPT" }));
  await waitFor(() => expect(api.prepareCodexHandoff).toHaveBeenCalledWith("work-a", "codex-files", ["file-valid"]));
  expect(await screen.findByRole("button", { name: "批准此版本（不会发送）" })).toBeVisible();
  expect(api.approveCodexHandoff).not.toHaveBeenCalled();
  expect(api.sendCodexHandoff).not.toHaveBeenCalled();
});

it("restores a durable Codex review from its exact handoff URL after a mobile reload", async () => {
  window.history.replaceState({}, "", "/mobile?workstream=work-a&handoff=restart-safe-review");
  api.workstream.mockResolvedValue(activeChatGptSnapshot("work-a"));
  api.codexHandoffReview.mockResolvedValue({
    actionId: "restart-safe-review", revision: 3, status: "APPROVED", message: "Owner-approved exact payload", attachments: ["design.md"], requiresManualDispatch: true,
    codexOutboundIdentity: {
      workstreamId: "work-a", sourceId: "codex-observation", sourceKind: "REPLY_OBSERVATION", sourceEndpointId: "endpoint-codex-a",
      sourceCodexThreadId: "thread-codex-a", destinationChatgptConversationId: "12345678-abcd",
    },
  });
  render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);

  expect(await screen.findByRole("heading", { name: "内容已批准，等待你手动发送" })).toBeVisible();
  expect(screen.getByText("Owner-approved exact payload")).toBeVisible();
  expect(screen.getAllByText("本次审阅保存的精确 ChatGPT 对话")).toHaveLength(2);
  expect(screen.getAllByText("12345678-abcd")).toHaveLength(2);
  expect(screen.getByText("codex-observation")).toBeVisible();
  expect(api.codexHandoffReview).toHaveBeenCalledWith("restart-safe-review");
});

it("restores the delivery attached to an exact failed review instead of a newer handoff", async () => {
  window.history.replaceState({}, "", "/mobile?workstream=work-a&handoff=failed-review");
  const delivery = {
    id: "exact-failed-delivery", workstreamId: "work-a", direction: "CODEX_TO_CHATGPT" as const,
    approvedText: "Exact failed approved content", createdAt: 20, status: "FAILED" as const,
    errorCode: "AUTH_REQUIRED", errorMessage: "Owner verification required; no provider submission occurred",
    sourceKind: "ENDPOINT" as const, sourceEndpoint: { id: "endpoint-codex-a", externalId: "thread-codex-a" },
    destinationEndpoint: { id: "endpoint-chatgpt-a", externalId: "12345678-abcd" },
  };
  api.workstream.mockResolvedValue({
    ...activeChatGptSnapshot("work-a"),
    handoffs: [delivery, { ...delivery, id: "newer-delivery", approvedText: "Unrelated newer content", createdAt: 30 }],
    mobileCodexOutboundReviews: [{
      actionId: "failed-review", workstreamId: "work-a", sourceReferenceId: "codex-observation", sourceEndpointId: "endpoint-codex-a",
      sourceCodexThreadId: "thread-codex-a", destinationChatgptConversationId: "12345678-abcd", originalText: "original",
      approvedText: delivery.approvedText, revision: 4, status: "FAILED", handoffId: delivery.id, createdAt: 10, updatedAt: 20,
    }],
  });
  api.codexHandoffReview.mockResolvedValue({
    actionId: "failed-review", revision: 4, status: "FAILED", message: delivery.approvedText, attachments: ["selected.txt"],
    codexOutboundIdentity: { workstreamId: "work-a", sourceId: "codex-observation", sourceKind: "REPLY_OBSERVATION", sourceEndpointId: "endpoint-codex-a", sourceCodexThreadId: "thread-codex-a", destinationChatgptConversationId: "12345678-abcd" },
  });
  render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);
  expect(await screen.findByText(delivery.errorMessage)).toBeVisible();
  expect(screen.getByText(delivery.id)).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "查看已批准的完整内容" }));
  expect(screen.getByText(delivery.approvedText)).toBeVisible();
  expect(screen.queryByText("Unrelated newer content")).toBeNull();
  expect(api.sendCodexHandoff).not.toHaveBeenCalled();
});

it("restores a durable ChatGPT-to-Codex review from its exact handoff URL after a mobile reload", async () => {
  window.history.replaceState({}, "", "/mobile?workstream=work-a&handoff=restart-safe-inbound-review");
  api.workstream.mockResolvedValue(activeChatGptSnapshot("work-a"));
  api.codexHandoffReview.mockRejectedValue(new Error("The mobile Codex Handoff review is no longer available"));
  api.chatGptHandoffReview.mockResolvedValue({
    actionId: "restart-safe-inbound-review", revision: 3, status: "APPROVED", message: "Owner-approved exact ChatGPT payload",
    chatgptInboundIdentity: {
      workstreamId: "work-a", sourceId: "reply-old", sourceKind: "REPLY_OBSERVATION", sourceEndpointId: "endpoint-chatgpt-a",
      sourceChatgptConversationId: "12345678-abcd", destinationCodexThreadId: "thread-codex-a",
    },
  });
  render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);

  expect(await screen.findByRole("heading", { name: "内容已锁定，等待发送" })).toBeVisible();
  expect(screen.getByText("Owner-approved exact ChatGPT payload")).toBeVisible();
  expect(screen.getAllByText("本次审阅保存的精确 Codex 对话")).toHaveLength(1);
  expect(screen.getAllByText("thread-codex-a")).toHaveLength(1);
  expect(screen.getByText("reply-old")).toBeVisible();
  expect(screen.getByRole("button", { name: "单独发送给 Codex" })).toBeVisible();
  expect(api.codexHandoffReview).toHaveBeenCalledWith("restart-safe-inbound-review");
  expect(api.chatGptHandoffReview).toHaveBeenCalledWith("restart-safe-inbound-review");
});

it("restores the exact ChatGPT-to-Codex review under React StrictMode instead of consuming its action ID during effect replay", async () => {
  window.history.replaceState({}, "", "/mobile?workstream=work-a&handoff=strict-inbound-review");
  api.workstream.mockResolvedValue(activeChatGptSnapshot("work-a"));
  api.codexHandoffReview.mockRejectedValue(new Error("The mobile Codex Handoff review is no longer available"));
  api.chatGptHandoffReview.mockResolvedValue({
    actionId: "strict-inbound-review", revision: 4, status: "APPROVED", message: "StrictMode exact payload",
    chatgptInboundIdentity: {
      workstreamId: "work-a", sourceId: "reply-old", sourceKind: "REPLY_OBSERVATION", sourceEndpointId: "endpoint-chatgpt-a",
      sourceChatgptConversationId: "12345678-abcd", destinationCodexThreadId: "thread-codex-a",
    },
  });
  render(<StrictMode><UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" /></StrictMode>);

  expect(await screen.findByRole("heading", { name: "内容已锁定，等待发送" })).toBeVisible();
  expect(screen.getByText("StrictMode exact payload")).toBeVisible();
  expect(screen.getByRole("button", { name: "单独发送给 Codex" })).toBeVisible();
  expect(api.chatGptHandoffReview).toHaveBeenCalledWith("strict-inbound-review");
});

it("uses the Router safety capability to offer copy-and-manual-send instead of an impossible ChatGPT dispatch", async () => {
  api.workstream.mockResolvedValue(activeChatGptSnapshot("work-a"));
  api.replyObservations.mockResolvedValue([{
    id: "codex-manual", endpointId: "endpoint-codex-a", text: "必须由 owner 手动发送的 Codex 回复", observedAt: 20, pushState: "SENT",
  }]);
  api.prepareCodexHandoff.mockResolvedValue({ actionId: "codex-manual-review", revision: 1, status: "READY", message: "已批准内容", requiresManualDispatch: true });
  api.approveCodexHandoff.mockResolvedValue({ actionId: "codex-manual-review", revision: 2, status: "APPROVED", message: "已批准内容", requiresManualDispatch: true });
  render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);

  fireEvent.click(await screen.findByRole("button", { name: "审阅这条 Codex 回复" }));
  fireEvent.click(await screen.findByRole("button", { name: "批准此版本（不会发送）" }));

  expect(await screen.findByText("内容已批准，等待你在 ChatGPT 手动发送")).toBeVisible();
  expect(screen.getByText("Router 尚未把这段内容写入 ChatGPT，也不会把它标记为已送达。")).toBeVisible();
  expect(screen.getByRole("button", { name: "复制已批准内容，手动发送到 ChatGPT" })).toBeVisible();
  expect(screen.queryByRole("button", { name: "单独发送给 ChatGPT" })).toBeNull();
  expect(api.sendCodexHandoff).not.toHaveBeenCalled();
});

it("resolves an approved mobile handoff to an exact owner-operated ChatGPT link without sending", async () => {
  useCompactMobileViewport();
  api.workstream.mockResolvedValue(activeChatGptSnapshot("work-a"));
  api.replyObservations.mockResolvedValue([{
    id: "codex-manual-link", endpointId: "endpoint-codex-a", text: "必须由 owner 手动发送的 Codex 回复", observedAt: 20, pushState: "SENT",
  }]);
  api.prepareCodexHandoff.mockResolvedValue({ actionId: "codex-manual-link-review", revision: 1, status: "READY", message: "已批准内容", requiresManualDispatch: true });
  api.approveCodexHandoff.mockResolvedValue({ actionId: "codex-manual-link-review", revision: 2, status: "APPROVED", message: "已批准内容", requiresManualDispatch: true });
  render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);

  fireEvent.click(await screen.findByRole("button", { name: "审阅这条 Codex 回复" }));
  fireEvent.click(await screen.findByRole("button", { name: "批准此版本（不会发送）" }));
  fireEvent.click(await screen.findByRole("button", { name: "核对并显示精确 ChatGPT 链接" }));

  await waitFor(() => expect(api.codexHandoffManualDestination).toHaveBeenCalledWith("codex-manual-link-review"));
  expect(await screen.findByRole("link", { name: "打开已核对的 ChatGPT 对话" })).toHaveAttribute("href", "https://chatgpt.com/c/12345678-abcd");
  expect(api.sendCodexHandoff).not.toHaveBeenCalled();
});

it("sends an ordinary discussion only after the explicit mobile action and distinguishes accepted from complete", async () => {
  api.workstream.mockResolvedValue(activeChatGptSnapshot("work-a"));
  api.sendChatGptDiscussion.mockRejectedValue(new Error("CHATGPT_ACCEPTED_PENDING_TERMINAL"));
  render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);
  fireEvent.click(await screen.findByRole("button", { name: "回复 ChatGPT" }));
  fireEvent.change(await screen.findByLabelText("你的修改意见"), { target: { value: "owner 显式发送的讨论" } });
  expect(api.sendChatGptDiscussion).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "发送给 ChatGPT" }));
  await waitFor(() => expect(api.sendChatGptDiscussion).toHaveBeenCalledExactlyOnceWith("work-a", "owner 显式发送的讨论"));
  expect(api.sendCodexHandoff).not.toHaveBeenCalled();
  expect(api.manualChatGptDiscussionDestination).not.toHaveBeenCalled();
});

it("labels a Service Worker notification-render receipt without claiming an iOS banner", async () => {
  api.workstream.mockResolvedValue(activeChatGptSnapshot("work-a"));
  api.replyObservations.mockResolvedValue([{
    id: "codex-rendered", endpointId: "endpoint-codex-a", text: "Codex 已完成的外部回复", observedAt: 20,
    pushState: "SENT", pushRenderedAt: 21,
  }]);
  render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);
  expect(await screen.findByText(/PWA 已执行通知展示（系统横幅无回执）/)).toBeVisible();
});

it("keeps one recovered exact pre-composer failure reachable from the normal workspace", async () => {
  api.workstream.mockResolvedValue({
    ...activeChatGptSnapshot("work-a"),
    handoffs: [{
      id: "persisted-handoff", workstreamId: "work-a", direction: "CODEX_TO_CHATGPT", approvedText: "只发送这份已批准内容", createdAt: 20,
      status: "FAILED", errorCode: "CHATGPT_AUTH_OR_COMPOSER_REQUIRED", errorMessage: "ChatGPT rejected the carrier before text entry; no provider submission occurred",
      sourceKind: "ENDPOINT", sourceEndpoint: { id: "endpoint-codex-a", externalId: "thread-codex-a" }, destinationEndpoint: { id: "endpoint-chatgpt-a", externalId: "12345678-abcd" },
    }],
  });
  api.replyObservations.mockResolvedValue([{
    id: "codex-observation", endpointId: "endpoint-codex-a", text: "当前 Codex 完成回复", observedAt: 20, pushState: "SENT",
  }]);
  render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);

  fireEvent.click(await screen.findByRole("button", { name: "查看未发送的交付记录" }));
  expect(await screen.findByText("尚未发送到 ChatGPT")).toBeVisible();
  expect(screen.getByRole("region", { name: "手动发送的精确目标" })).toHaveTextContent("12345678-abcd");
  expect(screen.getByRole("button", { name: "复制已批准内容，手动发送到 ChatGPT" })).toBeVisible();
  expect(api.sendCodexHandoff).not.toHaveBeenCalled();
});

it("keeps one approved exact Codex review with a separate explicit send reachable after returning to the ordinary workspace", async () => {
  api.workstream.mockResolvedValue({
    ...activeChatGptSnapshot("work-a"),
    mobileCodexOutboundReviews: [{
      actionId: "approved-manual-review", workstreamId: "work-a", sourceRunId: null,
      sourceReferenceId: "codex-observation", sourceEndpointId: "endpoint-codex-a", sourceCodexThreadId: "thread-codex-a",
      destinationChatgptConversationId: "12345678-abcd", originalText: "原始 Codex 回复", approvedText: "这是已经批准、尚未手动发送的准确内容",
      revision: 2, status: "APPROVED", handoffId: null, createdAt: 10, updatedAt: 20,
    }],
  });
  api.replyObservations.mockResolvedValue([{
    id: "codex-observation", endpointId: "endpoint-codex-a", text: "当前 Codex 完成回复", observedAt: 20, pushState: "SENT",
  }]);
  render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);

  fireEvent.click(await screen.findByRole("button", { name: "查看已批准内容并单独发送" }));
  expect(await screen.findByText("结论和附件已锁定，尚未发送")).toBeVisible();
  expect(screen.getByText("这是已经批准、尚未手动发送的准确内容")).toBeVisible();
  expect(screen.getByRole("button", { name: "单独发送给 ChatGPT" })).toBeVisible();
  expect(api.sendCodexHandoff).not.toHaveBeenCalled();
  expect(api.codexHandoffReview).not.toHaveBeenCalled();
  expect(api.sendCodexHandoff).not.toHaveBeenCalled();
});

it("keeps one approved exact ChatGPT-to-Codex review reachable after returning to the ordinary workspace", async () => {
  api.workstream.mockResolvedValue({
    ...activeChatGptSnapshot("work-a"),
    mobileChatgptInboundReviews: [{
      actionId: "approved-inbound-review", workstreamId: "work-a", sourceRunId: null,
      sourceReferenceId: "reply-old", sourceEndpointId: "endpoint-chatgpt-a", sourceChatgptConversationId: "12345678-abcd",
      destinationCodexThreadId: "thread-codex-a", originalText: "原始 ChatGPT 回复", approvedText: "这是已经批准、尚未发送给 Codex 的准确内容",
      revision: 2, status: "APPROVED", handoffId: null, createdAt: 10, updatedAt: 20,
    }],
  });
  render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);

  fireEvent.click(await screen.findByRole("button", { name: "查看已批准内容并单独发送" }));
  expect(await screen.findByRole("heading", { name: "内容已锁定，等待发送" })).toBeVisible();
  expect(screen.getByText("这是已经批准、尚未发送给 Codex 的准确内容")).toBeVisible();
  expect(screen.getByRole("group", { name: "本次接收端精确绑定" })).toHaveTextContent("thread-codex-a");
  expect(screen.getByRole("button", { name: "单独发送给 Codex" })).toBeVisible();
  expect(api.chatGptHandoffReview).not.toHaveBeenCalled();
});

it("reopens a durable exact pre-composer failure from inbox after the session review is gone", async () => {
  api.workstreams.mockResolvedValue([{
    id: "work-a", name: "精确交付失败", bindingSummary: "工作流 a · ChatGPT exact · Codex exact", attentionCount: 1,
    attentionItems: [{ sourceId: "persisted-handoff", kind: "HANDOFF_FAILED", priority: 0, message: "ChatGPT 未写入；等待人工处理", activityAt: 20 }],
  }]);
  api.workstream.mockResolvedValue({
    ...activeChatGptSnapshot("work-a"),
    handoffs: [{
      id: "persisted-handoff", workstreamId: "work-a", direction: "CODEX_TO_CHATGPT", approvedText: "只发送这份已批准内容", createdAt: 20,
      status: "FAILED", errorCode: "CHATGPT_AUTH_OR_COMPOSER_REQUIRED", errorMessage: "ChatGPT rejected the carrier before text entry; no provider submission occurred",
      sourceKind: "ENDPOINT", sourceEndpoint: { id: "endpoint-codex-a", externalId: "thread-codex-a" }, destinationEndpoint: { id: "endpoint-chatgpt-a", externalId: "12345678-abcd" },
    }, {
      id: "newer-different-handoff", workstreamId: "work-a", direction: "CODEX_TO_CHATGPT", approvedText: "绝不能被本次收件箱点击替代的较新内容", createdAt: 30,
      status: "FAILED", errorCode: "CHATGPT_AUTH_OR_COMPOSER_REQUIRED", errorMessage: "另一次交付失败",
      sourceKind: "ENDPOINT", sourceEndpoint: { id: "endpoint-codex-a", externalId: "thread-codex-a" }, destinationEndpoint: { id: "endpoint-chatgpt-a", externalId: "12345678-abcd" },
    }],
  });
  render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);

  fireEvent.click(await screen.findByRole("button", { name: /^收件箱/ }));
  fireEvent.click(await screen.findByRole("button", { name: /检查交付失败.*精确交付失败/ }));
  expect(await screen.findByText("尚未发送到 ChatGPT")).toBeVisible();
  expect(screen.getByText("Router 在写入输入框前已停止", { exact: false })).toBeVisible();
  expect(screen.getByRole("button", { name: "复制已批准内容，手动发送到 ChatGPT" })).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "查看已批准的完整内容" }));
  expect(screen.getByRole("region", { name: "已批准内容" })).toHaveTextContent("只发送这份已批准内容");
  expect(screen.queryByText("绝不能被本次收件箱点击替代的较新内容")).toBeNull();
  fireEvent.click(screen.getByRole("group", { name: "精确交付记录" }).querySelector("summary")!);
  expect(screen.getByRole("group", { name: "精确交付记录" })).toHaveTextContent("persisted-handoff");
});

it("recovers one exact durable delivery record when an old mobile review URL has expired", async () => {
  window.history.replaceState({}, "", "/mobile?workstream=work-a&handoff=old-session-only-review");
  api.workstream.mockResolvedValue({
    ...activeChatGptSnapshot("work-a"),
    handoffs: [{
      id: "only-current-binding-handoff", workstreamId: "work-a", direction: "CODEX_TO_CHATGPT", approvedText: "旧页面已批准的内容", createdAt: 20,
      status: "FAILED", errorCode: "CHATGPT_AUTH_OR_COMPOSER_REQUIRED", errorMessage: "ChatGPT rejected the carrier before text entry; no provider submission occurred",
      sourceKind: "ENDPOINT", sourceEndpoint: { id: "endpoint-codex-a", externalId: "thread-codex-a" }, destinationEndpoint: { id: "endpoint-chatgpt-a", externalId: "12345678-abcd" },
    }],
  });
  api.codexHandoffReview.mockRejectedValue(new Error("The mobile ChatGPT Handoff review is no longer available"));
  render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);

  expect(await screen.findByText("尚未发送到 ChatGPT")).toBeVisible();
  expect(screen.getByRole("alert")).toHaveTextContent("唯一的持久交付记录");
  expect(screen.getByText("12345678-abcd")).toBeVisible();
  expect(screen.getByRole("button", { name: "复制已批准内容，手动发送到 ChatGPT" })).toBeVisible();
  expect(api.codexHandoffReview).toHaveBeenCalledWith("old-session-only-review");
  expect(api.sendCodexHandoff).not.toHaveBeenCalled();
});

it("opens the exact Codex conversation from the workspace entry instead of silently falling back to ChatGPT", async () => {
  api.workstream.mockResolvedValue(activeChatGptSnapshot("work-a"));
  api.replyObservations.mockResolvedValue([{
    id: "codex-observation", endpointId: "endpoint-codex-a", text: "菜单打开的 Codex 外部回复", observedAt: 20, pushState: "SENT",
  }]);
  api.codexHistory.mockResolvedValue({ history: [{ id: "history-a", kind: "AgentMessage", text: "仅供核对的 Codex 线程历史" }] });
  render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);

  fireEvent.click(await screen.findByRole("button", { name: "Codex 对话记录" }));

  expect(await screen.findByRole("region", { name: "Codex 对话" })).toBeVisible();
  expect(screen.getByText("当前已绑定 Codex 对话")).toBeVisible();
  expect(screen.getByText("菜单打开的 Codex 外部回复")).toBeVisible();
  expect(screen.getByText("仅供核对的 Codex 线程历史")).toBeVisible();
  expect(screen.getByRole("button", { name: "审阅此回复，准备转发给 ChatGPT" })).toBeVisible();
  expect(screen.queryByText("当前 ChatGPT 对话")).toBeNull();
});

it("refreshes and exposes a newly materialized Codex observation after the exact manual check", async () => {
  useCompactMobileViewport();
  api.workstream.mockResolvedValue(activeChatGptSnapshot("work-a"));
  api.replyObservations
    .mockResolvedValueOnce([])
    .mockResolvedValueOnce([{
      id: "codex-new-observation", endpointId: "endpoint-codex-a", text: "刚检查到的 Codex 完整回复", observedAt: 30, pushState: "SENT",
    }]);
  api.checkNewCodexReplies.mockResolvedValue({ state: "NEW_TERMINAL_REPLY", observationCreated: true });
  render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);

  fireEvent.click(await screen.findByRole("button", { name: "更多工作区操作" }));
  fireEvent.click(await screen.findByRole("button", { name: "检查 Codex 新回复" }));
  await waitFor(() => expect(api.checkNewCodexReplies).toHaveBeenCalledWith("work-a"));
  expect(await screen.findByText("刚检查到的 Codex 完整回复")).toBeVisible();
  // The menu closes on selection; completion must remain visible without reopening it.
  expect(screen.getByText("Codex 检查完成：已记录 1 条新的完成回复。")).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "更多工作区操作" }));
  expect(screen.getByText("Codex 检查完成：已记录 1 条新的完成回复。")).toBeVisible();
});

it("keeps edits made before and during file capture and blocks approval until selection is confirmed", async () => {
  useCompactMobileViewport();
  api.workstream.mockResolvedValue(activeChatGptSnapshot("work-a"));
  api.replyObservations.mockResolvedValue([{ id: "chat-source", endpointId: "endpoint-chatgpt-a", text: "Full native source", observedAt: 20, pushState: "NO_SUBSCRIPTION" }]);
  api.prepareHandoff.mockResolvedValue({ actionId: "chatgpt-observation-review", revision: 1, status: "READY", message: "Server original", attachments: [], attachmentOptions: ["selected.txt"] });
  let finishCapture!: (value: unknown) => void;
  api.selectHandoffAttachments.mockImplementationOnce(() => new Promise(resolve => { finishCapture = resolve; }));
  render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);
  fireEvent.click(await screen.findByRole("button", { name: "选择/编辑交给 Codex 的范围" }));
  fireEvent.change(screen.getByRole("textbox", { name: "交接指令范围" }), { target: { value: "Initial selection" } });
  fireEvent.click(screen.getByRole("button", { name: "确认范围，进入审阅" }));
  const editor = await screen.findByRole("textbox", { name: "Handoff message" });
  fireEvent.change(editor, { target: { value: "Edited before capture" } });
  fireEvent.click(screen.getByRole("checkbox", { name: "selected.txt" }));
  const approve = screen.getByRole("button", { name: "批准此版本（不会发送）" });
  expect(approve).toBeDisabled();
  fireEvent.click(screen.getByRole("button", { name: "确认附件选择" }));
  expect(approve).toBeDisabled();
  fireEvent.change(editor, { target: { value: "Edited while capture was pending" } });
  await act(async () => finishCapture({ actionId: "chatgpt-observation-review", revision: 2, status: "READY", message: "Server original", attachments: ["selected.txt"], attachmentOptions: ["selected.txt"] }));
  expect(editor).toHaveValue("Edited while capture was pending");
  await waitFor(() => expect(approve).toBeEnabled());
  fireEvent.click(approve);
  await waitFor(() => expect(api.approveHandoff).toHaveBeenCalledWith("chatgpt-observation-review", 2, "Edited while capture was pending"));
});

it("explains the exact Router browser check and degraded passive observation fallback", async () => {
  useCompactMobileViewport();
  api.workstream.mockResolvedValue(activeChatGptSnapshot("work-a"));
  api.checkNewChatGptReplies.mockResolvedValue({ state: "NO_NEW_TERMINAL_REPLY", observationCreated: false });
  render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);

  fireEvent.click(await screen.findByRole("button", { name: "更多工作区操作" }));
  fireEvent.click(await screen.findByRole("button", { name: "查看回复观察说明" }));
  fireEvent.click(screen.getByRole("button", { name: "更多工作区操作" }));
  expect(await screen.findByText(/Router 在电脑端读取精确绑定的 ChatGPT 对话/)).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "检查 ChatGPT 新回复" }));
  await waitFor(() => expect(api.checkNewChatGptReplies).toHaveBeenCalledWith("work-a"));
});

it("maps a retired observer error to the integrated Router browser without reinstalling a Connector", async () => {
  useCompactMobileViewport();
  api.workstream.mockResolvedValue(activeChatGptSnapshot("work-a"));
  api.checkNewChatGptReplies.mockRejectedValue(new Error("CHATGPT_NORMAL_BROWSER_REMOTE_DEBUGGING_REQUIRED"));
  render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);

  fireEvent.click(await screen.findByRole("button", { name: "更多工作区操作" }));
  fireEvent.click(await screen.findByRole("button", { name: "检查 ChatGPT 新回复" }));

  expect((await screen.findAllByText(/旧的浏览器观察路径已停用/)).length).toBeGreaterThan(0);
  expect((await screen.findAllByText(/AI Work Router Browser 核对当前绑定对话/)).length).toBeGreaterThan(0);
  expect(screen.queryByText(/加载 AI Work Router Connector/)).toBeNull();
  expect(screen.queryByText(/chrome:\/\/inspect\/#remote-debugging/)).toBeNull();
});

it("does not expose the retired Chrome debug-service timeout as an owner setup task", async () => {
  useCompactMobileViewport();
  api.workstream.mockResolvedValue(activeChatGptSnapshot("work-a"));
  api.checkNewChatGptReplies.mockRejectedValue(new Error("CHATGPT_NORMAL_BROWSER_AUTO_CONNECT_TIMED_OUT"));
  render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);

  fireEvent.click(await screen.findByRole("button", { name: "更多工作区操作" }));
  fireEvent.click(await screen.findByRole("button", { name: "检查 ChatGPT 新回复" }));

  expect((await screen.findAllByText(/旧的浏览器观察路径已停用/)).length).toBeGreaterThan(0);
  expect((await screen.findAllByText(/手机端不会启动旧链路/)).length).toBeGreaterThan(0);
  expect(screen.queryByText(/远程调试服务在本次 20 秒内没有响应/)).toBeNull();
});

it("disambiguates same-named workstreams with Router-derived stable binding summaries", async () => {
  api.workstreams.mockResolvedValue([
    { id: "work-a", name: "PRE-FLIGHT", bindingSummary: "工作流 6f5b8e67… · ChatGPT 6ab22967… · Codex 01a0d92e…", attentionCount: 1, attentionItems: [{ sourceId: "reply-a", kind: "CHATGPT_REPLY_OBSERVED", priority: 0 }] },
    { id: "work-b", name: "PRE-FLIGHT", bindingSummary: "工作流 37810cd6… · ChatGPT 6ab73a54… · Codex 01a0dbc4…", attentionCount: 1, attentionItems: [{ sourceId: "reply-b", kind: "CHATGPT_REPLY_OBSERVED", priority: 0 }] },
  ]);
  render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);

  expect(await screen.findByText("工作流 6f5b8e67… · ChatGPT 6ab22967… · Codex 01a0d92e…")).toBeVisible();
  expect(screen.getByText("工作流 37810cd6… · ChatGPT 6ab73a54… · Codex 01a0dbc4…")).toBeVisible();
  expect(screen.getByRole("button", { name: "PRE-FLIGHT · 工作流 6f5b8e67… · ChatGPT 6ab22967… · Codex 01a0d92e…" })).toBeVisible();
  expect(screen.getByRole("button", { name: "PRE-FLIGHT · 工作流 37810cd6… · ChatGPT 6ab73a54… · Codex 01a0dbc4…" })).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: /收件箱/ }));
  const inbox = await screen.findByRole("region", { name: "收件箱" });
  expect(within(inbox).getByText("工作流 6f5b8e67… · ChatGPT 6ab22967… · Codex 01a0d92e…")).toBeVisible();
  expect(within(inbox).getByText("工作流 37810cd6… · ChatGPT 6ab73a54… · Codex 01a0dbc4…")).toBeVisible();
});

it("opens the exact attention reply instead of the first observation in its workstream", async () => {
  api.workstreams.mockResolvedValue([{ id: "work-a", name: "精确待办", bindingSummary: "工作流 a · ChatGPT exact-a", attentionCount: 1, attentionItems: [{ sourceId: "reply-old", kind: "CHATGPT_REPLY_OBSERVED", priority: 3, message: "ChatGPT 有新回复", activityAt: 10 }] }]);
  api.workstream.mockResolvedValue(activeChatGptSnapshot("work-a"));
  render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);

  fireEvent.click(await screen.findByRole("button", { name: /^收件箱/ }));
  fireEvent.click(screen.getByRole("button", { name: /查看 ChatGPT 新回复.*精确待办/ }));
  expect(await screen.findByText("targeted reply")).toBeVisible();
  expect(screen.queryByText("latest reply must not replace target")).toBeNull();
});

it("keeps multiple unread replies from one workstream separately actionable", async () => {
  api.workstreams.mockResolvedValue([{
    id: "work-a", name: "逐条待办", bindingSummary: "工作流 a · ChatGPT exact-a", attentionCount: 2,
    attentionItems: [
      { sourceId: "reply-old", kind: "CHATGPT_REPLY_OBSERVED", priority: 3, message: "较早的精确回复", activityAt: 10 },
      { sourceId: "reply-new", kind: "CHATGPT_REPLY_OBSERVED", priority: 3, message: "较新的精确回复", activityAt: 20 },
    ],
  }]);
  api.workstream.mockResolvedValue(activeChatGptSnapshot("work-a"));
  api.replyObservations.mockResolvedValue([
    { id: "reply-new", endpointId: "endpoint-chatgpt-a", text: "new exact reply", observedAt: 20, pushState: "SENT" },
    { id: "reply-old", endpointId: "endpoint-chatgpt-a", text: "old exact reply", observedAt: 10, pushState: "SENT" },
  ]);
  render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);

  fireEvent.click(await screen.findByRole("button", { name: /^收件箱/ }));
  const unreadCards = screen.getAllByRole("button", { name: /查看 ChatGPT 新回复.*逐条待办/ });
  expect(unreadCards).toHaveLength(2);
  const older = unreadCards.find((card) => card.textContent?.includes("reply-old"));
  const newer = unreadCards.find((card) => card.textContent?.includes("reply-new"));
  expect(older).toBeDefined();
  expect(newer).toBeDefined();

  fireEvent.click(older!);
  expect(await screen.findByText("old exact reply")).toBeVisible();
  expect(screen.queryByText("new exact reply")).toBeNull();
  fireEvent.click(newer!);
  expect(await screen.findByText("new exact reply")).toBeVisible();
  expect(screen.queryByText("old exact reply")).toBeNull();
});

it("returns a forward review to its exact selected ChatGPT reply", async () => {
  api.workstreams.mockResolvedValue([{ id: "work-a", name: "前向精确待办", bindingSummary: "工作流 a · ChatGPT exact-a", attentionCount: 1, attentionItems: [{ sourceId: "reply-old", kind: "CHATGPT_REPLY_OBSERVED", priority: 3, message: "ChatGPT 有新回复", activityAt: 10 }] }]);
  api.workstream.mockResolvedValue(activeChatGptSnapshot("work-a"));
  api.prepareHandoff.mockResolvedValue({ actionId: "chatgpt-observation-review", revision: 1, status: "READY", message: "明确选定的范围" });
  render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);

  fireEvent.click(await screen.findByRole("button", { name: /^收件箱/ }));
  fireEvent.click(screen.getByRole("button", { name: /查看 ChatGPT 新回复.*前向精确待办/ }));
  expect(await screen.findByText("targeted reply")).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "选择/编辑交给 Codex 的范围" }));
  fireEvent.change(screen.getByRole("textbox", { name: "交接指令范围" }), { target: { value: "只交接这条精确回复" } });
  fireEvent.click(screen.getByRole("button", { name: "确认范围，进入审阅" }));
  await waitFor(() => expect(api.prepareHandoff).toHaveBeenCalledWith("work-a", "reply-old", "只交接这条精确回复"));
    expect(await screen.findByText("精确 Router 来源记录：")).toHaveTextContent("reply-old");
  fireEvent.click(screen.getByRole("button", { name: "批准此版本（不会发送）" }));
  expect(await screen.findByText("精确 Router 来源记录：")).toHaveTextContent("reply-old");
  fireEvent.click(await screen.findByRole("button", { name: "查看 ChatGPT 完整原回复" }));
  expect(await screen.findByText("targeted reply")).toBeVisible();
  expect(screen.queryByText("latest reply must not replace target")).toBeNull();
});

it("opens and reviews the exact Codex inbox observation instead of substituting the newest reply", async () => {
  api.workstreams.mockResolvedValue([{ id: "work-a", name: "精确 Codex 待办", bindingSummary: "工作流 a · Codex exact-a", attentionCount: 1, attentionItems: [{ sourceId: "codex-old", kind: "CODEX_REPLY_OBSERVED", priority: 3, message: "Codex 有新回复", activityAt: 10 }] }]);
  api.workstream.mockResolvedValue(activeChatGptSnapshot("work-a"));
  api.replyObservations.mockResolvedValue([
    { id: "codex-new", endpointId: "endpoint-codex-a", text: "newer Codex reply must not replace the requested reply", observedAt: 20, pushState: "SENT" },
    { id: "codex-old", endpointId: "endpoint-codex-a", text: "targeted exact Codex reply", observedAt: 10, pushState: "SENT" },
  ]);
  render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);

  fireEvent.click(await screen.findByRole("button", { name: /^收件箱/ }));
  fireEvent.click(screen.getByRole("button", { name: /查看 Codex 新回复.*精确 Codex 待办/ }));
  expect(await screen.findByRole("region", { name: "Codex 对话" })).toBeVisible();
  expect(screen.getByText("从收件箱打开的精确 Codex 回复")).toBeVisible();
  expect(screen.getByText("targeted exact Codex reply")).toBeVisible();
  expect(screen.queryByText("newer Codex reply must not replace the requested reply")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "审阅此回复，准备转发给 ChatGPT" }));
  await waitFor(() => expect(api.prepareCodexHandoff).toHaveBeenCalledWith("work-a", "codex-old", []));
  expect(await screen.findByRole("heading", { name: "审阅并批准后，单独发送给 ChatGPT" })).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "查看 Codex 完整原回复" }));
  expect(await screen.findByRole("region", { name: "Codex 对话" })).toBeVisible();
  expect(screen.getByText("targeted exact Codex reply")).toBeVisible();
  expect(screen.queryByText("newer Codex reply must not replace the requested reply")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "审阅此回复，准备转发给 ChatGPT" }));
  await waitFor(() => expect(api.prepareCodexHandoff).toHaveBeenCalledTimes(2));
  fireEvent.click(await screen.findByRole("button", { name: "批准此版本（不会发送）" }));
  expect(await screen.findByRole("button", { name: "返回编辑（需重新批准）" })).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "返回编辑（需重新批准）" }));
  await waitFor(() => expect(api.prepareCodexHandoff).toHaveBeenCalledTimes(3));
  expect(await screen.findByRole("heading", { name: "审阅并批准后，单独发送给 ChatGPT" })).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "批准此版本（不会发送）" }));
  fireEvent.click(await screen.findByRole("button", { name: "单独发送给 ChatGPT" }));
  await waitFor(() => expect(api.sendCodexHandoff).toHaveBeenCalledWith("codex-observation-review", 2));
  expect(await screen.findByText("已送达 ChatGPT")).toBeVisible();
  expect(screen.getByText("执行状态待确认")).toBeVisible();
});

it("fails closed when a mobile Codex inbox target is unavailable instead of substituting the newest thread reply", async () => {
  api.workstreams.mockResolvedValue([{ id: "work-a", name: "缺失 Codex 回复", bindingSummary: "工作流 a · Codex exact-a", attentionCount: 1, attentionItems: [{ sourceId: "codex-missing", kind: "CODEX_REPLY_OBSERVED", priority: 3, message: "Codex 有新回复", activityAt: 10 }] }]);
  api.workstream.mockResolvedValue(activeChatGptSnapshot("work-a"));
  api.replyObservations.mockResolvedValue([{ id: "codex-new", endpointId: "endpoint-codex-a", text: "newer mobile Codex reply must not replace an unavailable inbox target", observedAt: 20, pushState: "SENT" }]);
  render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);

  fireEvent.click(await screen.findByRole("button", { name: /^收件箱/ }));
  fireEvent.click(screen.getByRole("button", { name: /查看 Codex 新回复.*缺失 Codex 回复/ }));
  expect(await screen.findByRole("region", { name: "Codex 对话" })).toBeVisible();
  expect(screen.getByRole("alert")).toHaveTextContent("收件箱指定的精确 Codex 回复当前不可读；Router 没有显示同一线程的较新回复。");
  expect(screen.queryByText("newer mobile Codex reply must not replace an unavailable inbox target")).toBeNull();
  expect(screen.queryByRole("button", { name: "审阅此回复，准备转发给 ChatGPT" })).toBeNull();
});

it("keeps a historical ChatGPT result out of the Codex result page", async () => {
  api.workstreams.mockResolvedValue([{ id: "work-a", name: "历史结果", bindingSummary: "工作流 a · ChatGPT exact-a", attentionCount: 1, attentionItems: [{ sourceId: "chat-run", kind: "CHATGPT_RESULT_READY", priority: 4, message: "ChatGPT 结果等待审阅", activityAt: 10 }] }]);
  api.workstream.mockResolvedValue(activeChatGptSnapshot("work-a"));
  api.reviewResults.mockResolvedValue([{ runId: "chat-run", provider: "CHATGPT", resultIdentity: "chat-result", text: "只读的历史 ChatGPT 结果", attachments: [] }]);
  render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);

  fireEvent.click(await screen.findByRole("button", { name: /^收件箱/ }));
  fireEvent.click(screen.getByRole("button", { name: /审阅 ChatGPT 结果.*历史结果/ }));
  expect(await screen.findByRole("region", { name: "ChatGPT 历史结果" })).toBeVisible();
  expect(screen.getByText("只读的历史 ChatGPT 结果")).toBeVisible();
  expect(screen.getByText("这里不会把历史结果伪装成可直接发送的消息。", { exact: false })).toBeVisible();
});

it("opens only the exact ChatGPT result named by mobile inbox attention until the owner explicitly requests all history", async () => {
  api.workstreams.mockResolvedValue([{ id: "work-a", name: "精确历史结果", bindingSummary: "工作流 a · ChatGPT exact-a", attentionCount: 1, attentionItems: [{ sourceId: "chat-target", kind: "CHATGPT_RESULT_READY", priority: 4, message: "ChatGPT 结果等待审阅", activityAt: 10 }] }]);
  api.workstream.mockResolvedValue(activeChatGptSnapshot("work-a"));
  api.reviewResults.mockResolvedValue([
    { runId: "chat-newer", provider: "CHATGPT", resultIdentity: "chat-newer", text: "newer mobile ChatGPT result must not replace the inbox target", attachments: [] },
    { runId: "chat-target", provider: "CHATGPT", resultIdentity: "chat-target", text: "targeted mobile ChatGPT result", attachments: [] },
  ]);
  render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);

  fireEvent.click(await screen.findByRole("button", { name: /^收件箱/ }));
  fireEvent.click(screen.getByRole("button", { name: /审阅 ChatGPT 结果.*精确历史结果/ }));
  expect(await screen.findByRole("region", { name: "ChatGPT 历史结果" })).toBeVisible();
  expect(screen.getByText("从收件箱打开的精确 ChatGPT 执行结果")).toBeVisible();
  expect(screen.getByText("targeted mobile ChatGPT result")).toBeVisible();
  expect(screen.getByText("chat-target")).toBeVisible();
  expect(screen.queryByText("newer mobile ChatGPT result must not replace the inbox target")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "查看全部 ChatGPT 历史结果" }));
  expect(await screen.findByText("newer mobile ChatGPT result must not replace the inbox target")).toBeVisible();
});

it("fails closed for a missing exact Codex result instead of silently opening another mobile result", async () => {
  api.workstreams.mockResolvedValue([{ id: "work-a", name: "缺失历史结果", bindingSummary: "工作流 a · Codex exact-a", attentionCount: 1, attentionItems: [{ sourceId: "codex-missing", kind: "CODEX_RESULT_READY", priority: 4, message: "Codex 结果等待审阅", activityAt: 10 }] }]);
  api.workstream.mockResolvedValue(activeChatGptSnapshot("work-a"));
  api.reviewResults.mockResolvedValue([{ runId: "codex-other", provider: "CODEX", resultIdentity: "codex-other", text: "another mobile Codex result must not replace the inbox target", attachments: [] }]);
  render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);

  fireEvent.click(await screen.findByRole("button", { name: /^收件箱/ }));
  fireEvent.click(screen.getByRole("button", { name: /审阅 Codex 结果.*缺失历史结果/ }));
  expect(await screen.findByRole("region", { name: "Codex 结果与附件" })).toBeVisible();
  expect(screen.getByRole("alert")).toHaveTextContent("这条精确 Codex 历史结果当前不可读；Router 没有切换到其他结果。");
  expect(screen.queryByText("another mobile Codex result must not replace the inbox target")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "查看全部 Codex 历史结果" }));
  expect(await screen.findByText("another mobile Codex result must not replace the inbox target")).toBeVisible();
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

  it("uses the V3 default /mobile target exactly, focuses it, and never substitutes the newest reply", async () => {
    api.workstream.mockResolvedValue(activeChatGptSnapshot("work-a"));
    window.history.replaceState({}, "", "/mobile?workstream=work-a&reply=reply-old");
    render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);
    expect(await screen.findByText("targeted reply")).toBeVisible();
    expect(screen.queryByText("latest reply must not replace target")).toBeNull();
    await waitFor(() => expect(document.getElementById("reply-observation-reply-old")).toHaveFocus());
  });

  it("fails closed for an unavailable notification reply and does not fall back to another reply", async () => {
    window.history.replaceState({}, "", "/mobile?workstream=work-a&reply=reply-missing");
    render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);
    expect(await screen.findByRole("alert")).toHaveTextContent("未显示其他回复");
    expect(screen.queryByText("latest reply must not replace target")).toBeNull();
  });

  it("keeps ordinary /mobile default behavior when no exact notification target was supplied", async () => {
    api.workstream.mockResolvedValue(activeChatGptSnapshot("work-a"));
    window.history.replaceState({}, "", "/mobile");
    render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);
    expect(await screen.findByText("latest reply must not replace target")).toBeVisible();
  });

  it("clears A pairing input before B can submit it", async () => {
    api.projectLinks.mockResolvedValue([linkedChatGptProject]);
    window.history.replaceState({}, "", "/mobile");
    render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "项目" }));
    await enterAdvancedConnection();
    await screen.findByRole("heading", { name: "选择两端对话" });
    const surface = screen.getByRole("region", { name: "项目与对话连接" });
    const chatgpt = within(surface).getByLabelText("具体 ChatGPT 对话链接");
    fireEvent.change(chatgpt, { target: { value: "conversation-a" } });
    fireEvent.click(screen.getByRole("button", { name: /B 工作区/ }));
    fireEvent.click(screen.getByRole("button", { name: "项目" }));
    await enterAdvancedConnection();
    const reselectedSurface = await screen.findByRole("region", { name: "项目与对话连接" });
    await waitFor(() => expect(within(reselectedSurface).getByLabelText("具体 ChatGPT 对话链接")).toHaveValue(""));
    expect(within(reselectedSurface).getByRole("button", { name: "核对这组配对" })).toBeDisabled();
    expect(api.pairWorkstreamEndpoints).not.toHaveBeenCalled();
  });

  it("uses the same mobile list-first connection surface", async () => {
    api.projectLinks.mockResolvedValue([linkedChatGptProject]);
    window.history.replaceState({}, "", "/mobile");
    render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "项目" }));
    await enterAdvancedConnection();
    expect(await screen.findByRole("heading", { name: "选择两端对话" })).toBeVisible();
    expect(within(screen.getByRole("region", { name: "项目与对话连接" })).getByLabelText("具体 ChatGPT 对话链接")).toBeVisible();
  });

  it("shows the persisted exact pair on mobile instead of a false unbound or rebind state", async () => {
    api.workstream.mockImplementation(async (id: string) => activeChatGptSnapshot(id));
    render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);
    expect(await screen.findByRole("button", { name: "Codex 对话记录" })).toBeVisible();
    expect(screen.queryByText("Codex 未绑定")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "查看两端绑定与详情" }));
    await enterAdvancedConnection();
    const binding = await screen.findByRole("region", { name: "当前对话已绑定" });
    expect(within(binding).getByText("已验证的精确对话")).toBeVisible();
    expect(within(binding).getByText("Codex")).toBeVisible();
    expect(within(binding).queryByLabelText("具体 ChatGPT 对话链接")).toBeNull();
  });

  it("prepares an owner-confirmed URL into a visible candidate before separate confirmation", async () => {
    api.projectLinks.mockResolvedValue([linkedChatGptProject]);
    render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "项目" }));
    await enterAdvancedConnection();
    const url = await screen.findByLabelText("具体 ChatGPT 对话链接");
    fireEvent.change(url, { target: { value: "https://chatgpt.com/g/g-project/c/12345678-abcd" } });
    fireEvent.click(screen.getByRole("button", { name: "我已在默认浏览器核对，准备绑定" }));
    await waitFor(() => expect(api.prepareOwnerConfirmedChatGptEndpointBinding).toHaveBeenCalledWith("work-a", "https://chatgpt.com/g/g-project/c/12345678-abcd"));
    expect(await screen.findByText("已在默认浏览器核对的精确对话")).toBeVisible();
    expect(api.pairWorkstreamEndpoints).not.toHaveBeenCalled();
    expect(api.confirmExplicitChatGptEndpointBinding).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "确认绑定" }));
    await waitFor(() => expect(api.confirmExplicitChatGptEndpointBinding).toHaveBeenCalledWith("work-a"));
    expect(api.confirmExplicitChatGptEndpointBinding).toHaveBeenCalledWith("work-a");
  });

  it("keeps a transaction failure visible on the owner-confirmed candidate and permits a later retry", async () => {
    api.projectLinks.mockResolvedValue([linkedChatGptProject]);
    api.confirmExplicitChatGptEndpointBinding.mockRejectedValueOnce(new Error("checked persistence unavailable"));
    render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);
    await openPreparedChatGptCandidate();
    fireEvent.click(screen.getByRole("button", { name: "确认绑定" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("绑定没有保存。当前绑定没有改变。");
    expect(screen.getByText("已在默认浏览器核对的精确对话")).toBeVisible();
    expect(screen.getByRole("button", { name: "确认绑定" })).toBeEnabled();
  });

  it("renders CONFIRMING immediately and blocks a duplicate mobile confirmation", async () => {
    api.projectLinks.mockResolvedValue([linkedChatGptProject]);
    api.workstream.mockResolvedValue({ ...activeChatGptSnapshot("work-a"), activeCodexEndpoint: null });
    let resolveConfirmation: (endpoint: typeof confirmedChatGptEndpoint) => void = () => undefined;
    api.confirmExplicitChatGptEndpointBinding.mockImplementationOnce(() => new Promise((resolve) => { resolveConfirmation = resolve; }));
    render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);
    await openPreparedChatGptCandidate();
    const confirm = screen.getByRole("button", { name: "确认绑定" });
    fireEvent.click(confirm);
    fireEvent.click(confirm);
    expect(api.confirmExplicitChatGptEndpointBinding).toHaveBeenCalledTimes(1);
    expect(await screen.findByRole("status")).toHaveTextContent("正在确认绑定");
    expect(screen.getByRole("button", { name: "正在确认绑定…" })).toBeDisabled();
    await act(async () => { resolveConfirmation(confirmedChatGptEndpoint); });
  });

  it("retains SUCCEEDED rather than reclassifying a committed owner-confirmed binding when exact Mobile reproof fails", async () => {
    api.projectLinks.mockResolvedValue([linkedChatGptProject]);
    render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);
    await openPreparedChatGptCandidate();
    api.workstream.mockRejectedValueOnce(new Error("refresh unavailable"));
    fireEvent.click(screen.getByRole("button", { name: "确认绑定" }));
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("ChatGPT 对话已经绑定，但工作区状态刷新失败"));
    expect(screen.getByText("已在默认浏览器核对的精确对话")).toBeVisible();
    expect(screen.getByRole("button", { name: "ChatGPT 对话已绑定" })).toBeDisabled();
    expect(screen.queryByText("绑定没有保存。当前绑定没有改变。")).toBeNull();
  });

  it("retains SUCCEEDED when broad Mobile load fails after an exact ACTIVE reproof", async () => {
    api.projectLinks.mockResolvedValue([linkedChatGptProject]);
    render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);
    await openPreparedChatGptCandidate();
    api.workstream.mockResolvedValue(activeChatGptSnapshot("work-a"));
    api.workstreamDraft.mockRejectedValueOnce(new Error("draft read unavailable"));
    fireEvent.click(screen.getByRole("button", { name: "确认绑定" }));
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("ChatGPT 对话已经绑定，但工作区状态刷新失败"));
    expect(screen.getByText("已在默认浏览器核对的精确对话")).toBeVisible();
    expect(screen.getByRole("button", { name: "ChatGPT 对话已绑定" })).toBeDisabled();
  });

  it("fails closed as committed-success warning when returned or refreshed ChatGPT Endpoint identity is not exact", async () => {
    api.projectLinks.mockResolvedValue([linkedChatGptProject]);
    api.confirmExplicitChatGptEndpointBinding.mockResolvedValueOnce({ ...confirmedChatGptEndpoint, externalId: "conversation-other" });
    render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);
    await openPreparedChatGptCandidate();
    fireEvent.click(screen.getByRole("button", { name: "确认绑定" }));
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("返回的 Endpoint 未得到一致确认"));
    expect(screen.getByRole("button", { name: "ChatGPT 对话已绑定" })).toBeDisabled();
    expect(screen.queryByText("绑定没有保存。当前绑定没有改变。")).toBeNull();
  });

  it("requires a refreshed ACTIVE ChatGPT Endpoint equal to the confirmed exact Endpoint", async () => {
    api.projectLinks.mockResolvedValue([linkedChatGptProject]);
    render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);
    await openPreparedChatGptCandidate();
    api.workstream.mockResolvedValueOnce(activeChatGptSnapshot("work-a", "conversation-other"));
    fireEvent.click(screen.getByRole("button", { name: "确认绑定" }));
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("刷新后的 ACTIVE Endpoint 尚未得到一致确认"));
    expect(screen.getByRole("button", { name: "ChatGPT 对话已绑定" })).toBeDisabled();
  });

  it("does not expose managed Host browser setup while automated access is paused", async () => {
    api.projectLinks.mockResolvedValue([linkedChatGptProject]);
    render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "项目" }));
    await enterAdvancedConnection();
    expect(screen.queryByRole("button", { name: "打开专用浏览器登录" })).toBeNull();
    expect(api.openHostChatGptBrowserSetup).not.toHaveBeenCalled();
  });

it("opens only the exact Codex structured request selected from Inbox", async () => {
  api.workstreams.mockResolvedValue([{
    id: "work-a", name: "精确请求", attentionCount: 1,
    attentionItems: [{ sourceId: "request-target", kind: "CODEX_STRUCTURED_REQUEST", priority: 0, message: "需要一次授权", activityAt: 20 }],
  }]);
  api.workstream.mockResolvedValue({ ...snapshot("work-a"), activeCodexEndpoint: { id: "endpoint-codex-a", externalId: "thread-codex-a" } });
  api.codexRequests.mockResolvedValue([
    { requestId: "request-other", revision: 1, method: "item/commandExecution/requestApproval", kind: "COMMAND_APPROVAL", reason: "较早的请求，不能替代收件箱选择", choices: [{ id: "accept", label: "Allow once" }, { id: "decline", label: "Decline" }], questions: [], isBlocking: true, responseSent: false },
    { requestId: "request-target", revision: 2, method: "item/commandExecution/requestApproval", kind: "COMMAND_APPROVAL", reason: "收件箱点击的精确请求", choices: [{ id: "accept", label: "Allow once" }, { id: "decline", label: "Decline" }], questions: [], isBlocking: true, responseSent: false },
  ]);
  render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);

  fireEvent.click(await screen.findByRole("button", { name: /^收件箱/ }));
  fireEvent.click(await screen.findByRole("button", { name: /处理 Codex 请求.*精确请求/ }));

  expect(await screen.findByText("收件箱点击的精确请求")).toBeVisible();
  expect(screen.queryByText("较早的请求，不能替代收件箱选择")).toBeNull();
  expect(screen.getByText("request-target")).toBeVisible();
});

it("opens the exact Provider run status after its Inbox status action", async () => {
  api.workstreams.mockResolvedValue([{
    id: "work-a", name: "执行状态", attentionCount: 1,
    attentionItems: [{ sourceId: "run-target", kind: "PROVIDER_RUN_FAILED", priority: 0, message: "执行失败", activityAt: 20 }],
  }]);
  api.workstream.mockResolvedValue(snapshot("work-a"));
  render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);

  fireEvent.click(await screen.findByRole("button", { name: /^收件箱/ }));
  fireEvent.click(await screen.findByRole("button", { name: /查看失败执行状态.*执行状态/ }));

  expect(await screen.findByText("Codex 执行：FAILED")).toBeVisible();
  expect(screen.getByText("run-target")).toBeVisible();
  expect(screen.getByLabelText("Router 记录的执行时间")).toHaveTextContent("开始：");
  expect(screen.getByLabelText("Router 记录的执行时间")).toHaveTextContent("终态确认：");
  expect(screen.getByLabelText("Router 记录的执行时间")).toHaveTextContent("最后状态更新：");
  expect(screen.getByText(/不会自动重试、发送或替换为另一条结果/)).toBeVisible();
  expect(api.providerRunStatus).toHaveBeenCalledWith("work-a", "run-target");
});

it("locks a pending M17 response so opposite decisions cannot race", async () => {
    api.workstream.mockResolvedValue({ ...snapshot("work-a"), activeCodexEndpoint: { id: "endpoint-codex-a", externalId: "thread-codex-a" } });
    api.codexRequests.mockResolvedValue([{ requestId: "opaque-request-1", revision: 9, method: "item/commandExecution/requestApproval", kind: "COMMAND_APPROVAL", reason: "一次性选择", choices: [{ id: "accept", label: "Allow once" }, { id: "decline", label: "Decline" }], questions: [], isBlocking: true, responseSent: false }]);
    let resolveResponse: () => void = () => undefined;
    api.respondToCodexRequest.mockImplementationOnce(() => new Promise<void>((resolve) => { resolveResponse = resolve; }));
    render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "Codex 授权请求" }));
    fireEvent.click(screen.getByRole("button", { name: "允许这一次" }));
    fireEvent.click(screen.getByRole("button", { name: "拒绝" }));
    expect(api.respondToCodexRequest).toHaveBeenCalledTimes(1);
    expect(api.respondToCodexRequest).toHaveBeenCalledWith("work-a", "opaque-request-1", { revision: 9, decision: "accept" });
    resolveResponse();
    expect(await screen.findByRole("heading", { name: "已允许这一次" })).toBeVisible();
  });

  it("opens M15 as a direct exact-result feedback draft and sends only after the user action", async () => {
    api.workstream.mockResolvedValue({ ...snapshot("work-a"), activeCodexEndpoint: { id: "endpoint-codex-a", externalId: "thread-codex-a" } });
    api.reviewResults.mockResolvedValue([{ runId: "result-codex-a", provider: "CODEX", resultIdentity: "Codex 完整结果", text: "需要修改的完整结果", attachments: [] }]);
    render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "Codex 结果与附件 1" }));
    fireEvent.click(await screen.findByRole("button", { name: "修改" }));
    const draft = await screen.findByLabelText("修改意见");
    fireEvent.change(draft, { target: { value: "请补充手机端验证。" } });
    expect(api.sendCodexFeedback).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "发送修改意见" }));
    await waitFor(() => expect(api.sendCodexFeedback).toHaveBeenCalledWith("work-a", "result-codex-a", "请补充手机端验证。"));
  });

  it("locks a pending M15 send against double click and retains the exact draft after failure", async () => {
    api.workstream.mockResolvedValue({ ...snapshot("work-a"), activeCodexEndpoint: { id: "endpoint-codex-a", externalId: "thread-codex-a" } });
    api.reviewResults.mockResolvedValue([{ runId: "result-codex-a", provider: "CODEX", resultIdentity: "Codex 完整结果", text: "需要修改的完整结果", attachments: [] }]);
    let rejectSend: (reason?: unknown) => void = () => undefined;
    api.sendCodexFeedback.mockImplementationOnce(() => new Promise((_resolve, reject) => { rejectSend = reject; }));
    render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "Codex 结果与附件 1" }));
    fireEvent.click(await screen.findByRole("button", { name: "修改" }));
    fireEvent.change(await screen.findByLabelText("修改意见"), { target: { value: "保留手机端精确修改草稿。" } });
    const send = screen.getByRole("button", { name: "发送修改意见" });
    fireEvent.click(send);
    fireEvent.click(send);
    expect(api.sendCodexFeedback).toHaveBeenCalledTimes(1);
    expect(send).toBeDisabled();
    rejectSend(new Error("exact thread unavailable"));
    await waitFor(() => expect(send).toBeEnabled());
    expect(screen.getByLabelText("修改意见")).toHaveValue("保留手机端精确修改草稿。");
  });

  it("keeps M15 drafts isolated by exact result run and preserves them on cancel", async () => {
    api.workstream.mockResolvedValue({ ...snapshot("work-a"), activeCodexEndpoint: { id: "endpoint-codex-a", externalId: "thread-codex-a" } });
    api.reviewResults.mockResolvedValue([
      { runId: "result-codex-a", provider: "CODEX", resultIdentity: "结果 A", text: "A", attachments: [] },
      { runId: "result-codex-b", provider: "CODEX", resultIdentity: "结果 B", text: "B", attachments: [] },
    ]);
    render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "Codex 结果与附件 2" }));
    fireEvent.click(screen.getAllByRole("button", { name: "要求 Codex 修改" })[0]);
    fireEvent.change(await screen.findByLabelText("修改意见"), { target: { value: "只属于结果 A 的手机草稿" } });
    fireEvent.click(screen.getByRole("button", { name: "取消，保留草稿" }));
    fireEvent.click(screen.getAllByRole("button", { name: "要求 Codex 修改" })[1]);
    expect(await screen.findByLabelText("修改意见")).toHaveValue("");
    fireEvent.click(screen.getByRole("button", { name: "取消，保留草稿" }));
    fireEvent.click(screen.getAllByRole("button", { name: "要求 Codex 修改" })[0]);
    expect(await screen.findByLabelText("修改意见")).toHaveValue("只属于结果 A 的手机草稿");
    fireEvent.click(screen.getByRole("button", { name: "发送修改意见" }));
    await waitFor(() => expect(api.sendCodexFeedback).toHaveBeenCalledWith("work-a", "result-codex-a", "只属于结果 A 的手机草稿"));
  });

  it("autosaves an M15 draft through its exact persisted result scope", async () => {
    api.workstream.mockResolvedValue({ ...snapshot("work-a"), activeCodexEndpoint: { id: "endpoint-codex-a", externalId: "thread-codex-a" } });
    api.reviewResults.mockResolvedValue([{ runId: "result-codex-a", provider: "CODEX", resultIdentity: "Codex 完整结果", text: "需要修改的完整结果", attachments: [] }]);
    render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "Codex 结果与附件 1" }));
    fireEvent.click(screen.getByRole("button", { name: "修改" }));
    const textarea = await screen.findByLabelText("修改意见");
    vi.useFakeTimers();
    fireEvent.change(textarea, { target: { value: "手机刷新后也要保留" } });
    await act(async () => { await vi.advanceTimersByTimeAsync(500); });
    expect(api.saveCodexFeedbackDraft).toHaveBeenCalledWith("work-a", "result-codex-a", "手机刷新后也要保留", null);
  });

  it("restores a persisted M15 draft only for the opened exact result", async () => {
    api.workstream.mockResolvedValue({ ...snapshot("work-a"), activeCodexEndpoint: { id: "endpoint-codex-a", externalId: "thread-codex-a" } });
    api.reviewResults.mockResolvedValue([{ runId: "result-codex-a", provider: "CODEX", resultIdentity: "Codex 完整结果", text: "需要修改的完整结果", attachments: [] }]);
    api.codexFeedbackDraft.mockResolvedValue({ workstreamId: "work-a", sourceRunId: "result-codex-a", text: "手机已从 Router 草稿恢复", revision: 4, updatedAt: 10 });
    render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "Codex 结果与附件 1" }));
    fireEvent.click(screen.getByRole("button", { name: "修改" }));
    await waitFor(() => expect(screen.getByLabelText("修改意见")).toHaveValue("手机已从 Router 草稿恢复"));
    expect(api.codexFeedbackDraft).toHaveBeenCalledWith("work-a", "result-codex-a");
  });

  it("projects the edited mobile draft as saving then saved at the API timestamp", async () => {
    api.workstreamDraft.mockResolvedValue({ workstreamId: "work-a", text: "已加载", revision: 1, updatedAt: 100 });
    render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);
    const textarea = await screen.findByLabelText("继续当前精确对话");
    await waitFor(() => expect(textarea).toHaveValue("已加载"));
    const persistedAt = Date.UTC(2026, 8, 10, 9, 15);
    let resolveSave!: (value: { workstreamId: string; text: string; revision: number; updatedAt: number }) => void;
    api.saveWorkstreamDraft.mockImplementationOnce(() => new Promise((resolve) => { resolveSave = resolve; }));
    vi.useFakeTimers();
    fireEvent.change(textarea, { target: { value: "待保存的手机草稿" } });
    expect(screen.getByText("正在自动保存")).toBeVisible();
    await act(async () => { await vi.advanceTimersByTimeAsync(500); });
    expect(api.saveWorkstreamDraft).toHaveBeenCalledWith("work-a", "待保存的手机草稿", 1);
    await act(async () => { resolveSave({ workstreamId: "work-a", text: "待保存的手机草稿", revision: 9, updatedAt: persistedAt }); });
    expect(screen.getByText("草稿已自动保存", { exact: false })).toHaveTextContent(new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(persistedAt));
  });

  it("keeps the mobile draft text and reports failed when its matching save rejects", async () => {
    api.workstreamDraft.mockResolvedValue({ workstreamId: "work-a", text: "已加载", revision: 1, updatedAt: 100 });
    render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);
    const textarea = await screen.findByLabelText("继续当前精确对话");
    await waitFor(() => expect(textarea).toHaveValue("已加载"));
    api.saveWorkstreamDraft.mockRejectedValueOnce(new Error("offline"));
    vi.useFakeTimers();
    fireEvent.change(textarea, { target: { value: "手机草稿不能丢" } });
    await act(async () => { await vi.advanceTimersByTimeAsync(500); });
    expect(screen.getByRole("alert")).toHaveTextContent("草稿保存失败");
    expect(textarea).toHaveValue("手机草稿不能丢");
  });

  it("serializes successive mobile draft saves so a later edit uses the accepted Core revision", async () => {
    api.workstreamDraft.mockResolvedValue({ workstreamId: "work-a", text: "已加载", revision: 1, updatedAt: 100 });
    let resolveFirst!: (value: { workstreamId: string; text: string; revision: number; updatedAt: number }) => void;
    let resolveSecond!: (value: { workstreamId: string; text: string; revision: number; updatedAt: number }) => void;
    api.saveWorkstreamDraft
      .mockImplementationOnce(() => new Promise((resolve) => { resolveFirst = resolve; }))
      .mockImplementationOnce(() => new Promise((resolve) => { resolveSecond = resolve; }));
    render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);
    const textarea = await screen.findByLabelText("继续当前精确对话");
    await waitFor(() => expect(textarea).toHaveValue("已加载"));
    vi.useFakeTimers();
    fireEvent.change(textarea, { target: { value: "手机第一版" } });
    await act(async () => { await vi.advanceTimersByTimeAsync(500); });
    expect(api.saveWorkstreamDraft).toHaveBeenLastCalledWith("work-a", "手机第一版", 1);
    fireEvent.change(textarea, { target: { value: "手机第二版" } });
    await act(async () => { await vi.advanceTimersByTimeAsync(500); });
    expect(api.saveWorkstreamDraft).toHaveBeenCalledTimes(1);
    await act(async () => { resolveFirst({ workstreamId: "work-a", text: "手机第一版", revision: 2, updatedAt: 200 }); });
    await act(async () => { await vi.advanceTimersByTimeAsync(500); });
    expect(api.saveWorkstreamDraft).toHaveBeenLastCalledWith("work-a", "手机第二版", 2);
    await act(async () => { resolveSecond({ workstreamId: "work-a", text: "手机第二版", revision: 3, updatedAt: 300 }); });
    expect(screen.getByText("草稿已自动保存", { exact: false })).toBeVisible();
    expect(textarea).toHaveValue("手机第二版");
  });

  it("does not let an earlier mobile workstream save certify the newly selected draft", async () => {
    api.workstreamDraft.mockImplementation(async (id: string) => id === "work-a" ? { workstreamId: id, text: "已加载", revision: 1, updatedAt: 100 } : null);
    render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);
    const textarea = await screen.findByLabelText("继续当前精确对话");
    await waitFor(() => expect(textarea).toHaveValue("已加载"));
    let resolveSave!: (value: { workstreamId: string; text: string; revision: number; updatedAt: number }) => void;
    api.saveWorkstreamDraft.mockImplementationOnce(() => new Promise((resolve) => { resolveSave = resolve; }));
    vi.useFakeTimers();
    fireEvent.change(textarea, { target: { value: "A 的手机保存" } });
    await act(async () => { await vi.advanceTimersByTimeAsync(500); });
    fireEvent.click(screen.getByRole("button", { name: /B 工作区/ }));
    await act(async () => { resolveSave({ workstreamId: "work-a", text: "A 的手机保存", revision: 7, updatedAt: 777 }); });
    await act(async () => {});
    expect(screen.getByLabelText("继续当前精确对话")).toHaveValue("");
    expect(screen.getByText("草稿尚未保存")).toBeVisible();
  });

  it("does not infer mobile no-project durability from the returned thread ID", async () => {
    api.projectLinks.mockResolvedValue([linkedChatGptProject]);
    api.startUnprojectedCodexThread.mockResolvedValueOnce({ thread: { id: "thread-new" }, directory: "D:/router-scratch" });
    render(<UnifiedMobileWorkbenchHost initialSurface="WORKSPACE" />);
    fireEvent.click(await screen.findByRole("button", { name: "项目" }));
    await enterAdvancedConnection();
    await screen.findByRole("heading", { name: "选择两端对话" });
    fireEvent.click(screen.getByRole("button", { name: "下一步：选择 Codex 项目" }));
    fireEvent.click(screen.getByRole("button", { name: "返回 ChatGPT 对话" }));
    fireEvent.click(screen.getByRole("button", { name: "＋ 新建无项目 Codex 对话" }));
    fireEvent.click(screen.getByRole("button", { name: "创建无项目对话" }));
    expect(await screen.findByText("首次有效 Turn 前不要把它视为持久会话。", { exact: false })).toBeVisible();
    expect(screen.getByText("已通过 Codex `thread/read` 验证，并已带入本次配对候选；返回后仍需核对并确认，当前 Endpoint 不会自动改变。")).toBeVisible();
    expect(screen.queryByText("已按 Provider 返回结果确认可持久重开", { exact: false })).toBeNull();
  });
});

// These fixtures exercise retained host protocol projections through their historical
// composition. They are not Native R2 UI acceptance; native flow checks are separate.
vi.mock("../features/workbench/UnifiedWorkbench",async(importOriginal)=>{
 const actual=await importOriginal<typeof import("../features/workbench/UnifiedWorkbench")>();
 return {...actual,get LEGACY_WORKBENCH_DETAILS(){return !composition.native;},UnifiedWorkbench:(props:Parameters<typeof actual.UnifiedWorkbench>[0])=>composition.native?<actual.UnifiedWorkbench {...props}/>:<actual.LegacyWorkbench {...props}/>};
});


it("uses global lifecycle and human conversation labels for phone Bridges missing from the focused snapshot",async()=>{
 composition.native=true;useCompactMobileViewport();
 api.workstreams.mockResolvedValue([{...workstreams[0],status:"ACTIVE",projectName:"当前项目",sourceLabel:"Codex · 当前控制对话",updatedAt:10},{...workstreams[1],status:"ACTIVE",trashedAt:11,projectName:"其他项目",sourceLabel:"已删除的对话",updatedAt:11}]);
 api.workstream.mockResolvedValue({...snapshot("work-a"),workstreams:[snapshot("work-a").workstreams[0]]});
 render(<UnifiedMobileWorkbenchHost initialSurface="BRIDGES"/>);
 expect(await screen.findByText("Codex · 当前控制对话")).toBeVisible();
 expect(screen.queryByText("B 工作区")).toBeNull();
});
