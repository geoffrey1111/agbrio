import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MobileWorkbench } from "./MobileWorkbench";

const response = (body: unknown) => new Response(JSON.stringify(body), { status: 200, headers: { "content-type": "application/json" } });
const failure = (status: number, body: unknown) => new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

describe("Mobile workbench", () => {
  beforeEach(() => {
    vi.stubGlobal("fetch", vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (url.endsWith("/workstreams")) return Promise.resolve(response([{ id: "ws-1", name: "Mobile slice", attentionCount: 1, attentionItems: [{ kind: "CODEX_STRUCTURED_REQUEST", priority: 0 }], chatgptStatus: "READY", codexStatus: "IDLE" }]));
      if (url.endsWith("/review-results")) return Promise.resolve(response([
        { runId: "chat-run", provider: "CHATGPT", resultIdentity: "chat-result", text: "Full durable ChatGPT reply", markerText: "Exact Codex instruction" },
        { runId: "codex-run", provider: "CODEX", resultIdentity: "codex-result", text: "Full durable Codex final result", markerText: "Recommended ChatGPT conclusion", attachments: [{ id: "attachment-1", filename: "review.md", integrityStatus: "VERIFIED", defaultSelected: true }] },
      ]));
      if (url.endsWith("/chatgpt-result-recovery") && init?.method === "POST") return Promise.resolve(response({ recovered: false, message: "No recovery needed" }));
      if (url.endsWith("/codex-requests")) return Promise.resolve(response([{ requestId: "request-1", revision: 7, method: "item/commandExecution/requestApproval", kind: "COMMAND_APPROVAL", choices: [{ id: "accept", label: "Allow once" }], questions: [], isBlocking: false, responseSent: false }]));
      if (url.endsWith("/chatgpt-history")) return Promise.resolve(response({ title: "Bound", completeness: "COMPLETE", messages: [{ id: "chat-1", role: "assistant", text: "History" }] }));
      if (url.endsWith("/codex-history")) return Promise.resolve(response({ history: [{ id: "codex-1", kind: "agentMessage", text: "Codex history" }] }));
      if (url.endsWith("/handoff-review") && init?.method === "POST") return Promise.resolve(response({ actionId: "action-1", revision: 3, status: "READY", message: "Prepared payload" }));
      if (url.endsWith("/codex-handoff-review") && init?.method === "POST") return Promise.resolve(response({ actionId: "codex-action-1", revision: 6, status: "READY", message: "Prepared Codex payload" }));
      if (url.includes("/codex-handoff-review/") && url.endsWith("/approve") && init?.method === "POST") return Promise.resolve(response({ actionId: "codex-action-1", revision: 7, status: "APPROVED", message: "Edited Codex conclusion" }));
      if (url.includes("/codex-handoff-review/") && url.endsWith("/send") && init?.method === "POST") return Promise.resolve(response({ status: "SENT" }));
      if (url.endsWith("/approve") && init?.method === "POST") return Promise.resolve(response({ actionId: "action-1", revision: 4, status: "APPROVED", message: "Edited payload" }));
      if (url.endsWith("/send") && init?.method === "POST") return Promise.resolve(response({ turnId: "turn-1" }));
      if (url.endsWith("/chatgpt-feedback")) return Promise.resolve(response({ runId: "feedback-run", phase: "WAITING_FOR_REVISED_RESULT", message: "ChatGPT feedback accepted" }));
      if (url.endsWith("/chatgpt-feedback/feedback-run")) return Promise.resolve(response({ runId: "feedback-run", phase: "DELIVERY_FAILED", message: "ChatGPT feedback failed without retry" }));
      if (url.endsWith("/codex-feedback") || url.endsWith("/respond")) return Promise.resolve(new Response(null, { status: 204 }));
      return Promise.reject(new Error(`Unexpected request ${url}`));
    }));
  });
  afterEach(() => { cleanup(); vi.unstubAllGlobals(); window.history.replaceState({}, "", "/"); });

  it("shows durable full results and performs an explicit prepare, approve, and send sequence", async () => {
    render(<MobileWorkbench />);
    fireEvent.click((await screen.findAllByRole("button", { name: /Mobile slice/ }))[0]);
    expect(await screen.findByText("Full durable ChatGPT reply")).toBeVisible();
    expect(screen.getByText("Full durable Codex final result")).toBeVisible();
    expect(screen.getByText("建议转发的结论")).toBeVisible();
    expect(screen.getByText("Recommended ChatGPT conclusion")).toBeVisible();
    expect(screen.getByRole("checkbox", { name: "Include review.md" })).toBeChecked();
    fireEvent.click(screen.getByRole("button", { name: "只转发指令" }));
    const payload = await screen.findByRole("textbox", { name: "Approved payload" });
    fireEvent.change(payload, { target: { value: "Edited payload" } });
    fireEvent.click(screen.getByRole("button", { name: "批准这段内容" }));
    await screen.findByRole("button", { name: "发送给当前 Codex" });
    fireEvent.click(screen.getByRole("button", { name: "发送给当前 Codex" }));
    await screen.findByText(/Handoff 已发送不等于 Codex 已完成/);
    const calls = vi.mocked(fetch).mock.calls;
    expect(calls.find(([url]) => String(url).endsWith("/workstreams"))?.[1]).toMatchObject({ cache: "no-store" });
    expect(calls.find(([url]) => String(url).endsWith("/handoff-review"))?.[1]).toMatchObject({ method: "POST", body: JSON.stringify({ responseId: "chat-result" }) });
    expect(calls.find(([url]) => String(url).endsWith("/approve"))?.[1]).toMatchObject({ method: "POST", body: JSON.stringify({ revision: 3, message: "Edited payload" }) });
    expect(calls.find(([url]) => String(url).endsWith("/send"))?.[1]).toMatchObject({ method: "POST", body: JSON.stringify({ revision: 4 }) });
  });
  it("does not offer automatic ChatGPT relay without an exact marker", async () => {
    vi.mocked(fetch).mockImplementation((input: RequestInfo | URL,init?:RequestInit) => {
      const url = String(input);
      if (url.endsWith("/workstreams")) return Promise.resolve(response([{ id: "ws-1", name: "Mobile slice", attentionCount: 0, attentionItems: [] }]));
      if (url.endsWith("/review-results")) return Promise.resolve(response([{ runId: "chat-run", provider: "CHATGPT", resultIdentity: "chat-result", text: "Full reply without a marker" }]));
      if (url.endsWith("/codex-requests")) return Promise.resolve(response([]));
      if (url.endsWith("/chatgpt-history") || url.endsWith("/codex-history")) return Promise.resolve(response({ history: [], messages: [], title: "", completeness: "COMPLETE" }));
      return Promise.reject(new Error(`Unexpected request ${url}`));
    });
    render(<MobileWorkbench />);
    fireEvent.click((await screen.findAllByRole("button", { name: /Mobile slice/ }))[0]);
    expect(await screen.findByText("Full reply without a marker")).toBeVisible();
    expect(screen.queryByRole("button", { name: "只转发指令" })).toBeNull();
    expect(screen.getByText(/不能创建 Handoff/)).toBeVisible();
  });
  it("surfaces an independent unread reply observation and only changes it by explicit action", async () => {
    const original = vi.mocked(fetch).getMockImplementation();
    vi.mocked(fetch).mockImplementation((input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (url.endsWith("/reply-observations")) return Promise.resolve(response([{ id: "observed-1", text: "Complete observed reply without causal attribution", observedAt: 1, readAt: null, handledAt: null, pushState: "FAILED", markerText: null }]));
      if (url.endsWith("/reply-observations/observed-1/read") && init?.method === "POST") return Promise.resolve(new Response(null, { status: 204 }));
      return original!(input, init);
    });
    window.history.pushState({}, "", "/mobile?workstream=ws-1&reply=observed-1");
    render(<MobileWorkbench />);
    expect(await screen.findByText("Complete observed reply without causal attribution")).toBeVisible();
    const observation = document.getElementById("reply-observation-observed-1")!;
    expect(observation).toHaveAttribute("data-deep-link-focus", "true");
    expect(screen.getByText(/发送未成功，回复仍保留/)).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "标为已读" }));
    await waitFor(() => expect(vi.mocked(fetch).mock.calls.some(([url, request]) => String(url).endsWith("/reply-observations/observed-1/read") && request?.method === "POST")).toBe(true));
    expect(screen.getByText(/已读/)).toBeVisible();
  });
  it("sends feedback to each exact provider result without creating a handoff", async () => {
    render(<MobileWorkbench />);
    fireEvent.click((await screen.findAllByRole("button", { name: /Mobile slice/ }))[0]);
    await screen.findByText("Full durable ChatGPT reply");
    fireEvent.click(screen.getByRole("button", { name: "打回 ChatGPT" }));
    fireEvent.change(screen.getByRole("textbox", { name: "ChatGPT revision feedback" }), { target: { value: "Please revise the conclusion." } });
    fireEvent.click(screen.getByRole("button", { name: "打回并告诉 ChatGPT" }));
    fireEvent.click(screen.getByRole("button", { name: "打回 Codex" }));
    fireEvent.change(screen.getByRole("textbox", { name: "Codex revision feedback" }), { target: { value: "Please revise the files." } });
    fireEvent.click(screen.getByRole("button", { name: "打回并告诉 Codex" }));
    await waitFor(() => expect(vi.mocked(fetch).mock.calls.filter(([url]) => String(url).endsWith("-feedback"))).toHaveLength(2));
    expect(await screen.findByText(/修改意见未完成 · 不存在 Handoff/)).toBeVisible();
    expect(screen.getByText("ChatGPT feedback failed without retry")).toBeVisible();
    expect(screen.getByText("技术详情与只读历史").closest("details")).not.toHaveAttribute("open");
    const calls = vi.mocked(fetch).mock.calls;
    expect(calls.find(([url]) => String(url).endsWith("/chatgpt-feedback"))?.[1]).toMatchObject({ method: "POST", body: JSON.stringify({ runId: "chat-run", feedback: "Please revise the conclusion." }) });
    expect(calls.some(([url]) => String(url).endsWith("/chatgpt-feedback/feedback-run"))).toBe(true);
    expect(calls.find(([url]) => String(url).endsWith("/codex-feedback"))?.[1]).toMatchObject({ method: "POST", body: JSON.stringify({ runId: "codex-run", feedback: "Please revise the files." }) });
  });
  it("re-reads only the explicitly selected unconfirmed ChatGPT feedback execution", async () => {
    const original = vi.mocked(fetch).getMockImplementation();
    vi.mocked(fetch).mockImplementation((input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (url.endsWith("/chatgpt-feedback/feedback-run")) return Promise.resolve(response({ runId: "feedback-run", phase: "DELIVERY_UNCONFIRMED", message: "Exact acceptance is known but the result is not readable" }));
      if (url.endsWith("/chatgpt-feedback/feedback-run/result-recovery") && init?.method === "POST") return Promise.resolve(response({ recovered: false, message: "Exact result is still unavailable" }));
      return original!(input, init);
    });
    render(<MobileWorkbench />);
    fireEvent.click((await screen.findAllByRole("button", { name: /Mobile slice/ }))[0]);
    fireEvent.click(await screen.findByRole("button", { name: "打回 ChatGPT" }));
    fireEvent.change(screen.getByRole("textbox", { name: "ChatGPT revision feedback" }), { target: { value: "Please revise." } });
    fireEvent.click(screen.getByRole("button", { name: "打回并告诉 ChatGPT" }));
    const recovery = await screen.findByRole("button", { name: "重新读取这条完整结果" });
    fireEvent.click(recovery);
    await waitFor(() => expect(vi.mocked(fetch).mock.calls.some(([url, request]) => String(url).endsWith("/chatgpt-feedback/feedback-run/result-recovery") && request?.method === "POST")).toBe(true));
    expect(screen.getByText("Exact result is still unavailable")).toBeInTheDocument();
  });
  it("explains an exact feedback recovery rejection without exposing provider diagnostics", async () => {
    const original = vi.mocked(fetch).getMockImplementation();
    vi.mocked(fetch).mockImplementation((input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (url.endsWith("/chatgpt-feedback/feedback-run")) return Promise.resolve(response({ runId: "feedback-run", phase: "DELIVERY_UNCONFIRMED", message: "Router is waiting for one exact result" }));
      if (url.endsWith("/chatgpt-feedback/feedback-run/result-recovery") && init?.method === "POST") return Promise.resolve(failure(400, { error: "Exact provider diagnostics do not include an assistant turn identity" }));
      return original!(input, init);
    });
    render(<MobileWorkbench />);
    fireEvent.click((await screen.findAllByRole("button", { name: /Mobile slice/ }))[0]);
    fireEvent.click(await screen.findByRole("button", { name: "打回 ChatGPT" }));
    fireEvent.change(screen.getByRole("textbox", { name: "ChatGPT revision feedback" }), { target: { value: "Please revise." } });
    fireEvent.click(screen.getByRole("button", { name: "打回并告诉 ChatGPT" }));
    fireEvent.click(await screen.findByRole("button", { name: "重新读取这条完整结果" }));
    expect(await screen.findByText("这条完整结果暂时无法重新读取：Router 尚未确认同一条助手回复的精确身份。未发送任何消息，也不会自动重试。")).toBeVisible();
    expect(screen.queryByText("Exact provider diagnostics do not include an assistant turn identity")).toBeNull();
  });
  it("prepares editable Codex conclusion and selected attachments before separately approving and sending to ChatGPT", async () => {
    render(<MobileWorkbench />);
    fireEvent.click((await screen.findAllByRole("button", { name: /Mobile slice/ }))[0]);
    fireEvent.click(await screen.findByRole("button", { name: "转发结论 + 附件" }));
    const draft = await screen.findByRole("textbox", { name: "Draft for ChatGPT" });
    fireEvent.change(draft, { target: { value: "Edited Codex conclusion" } });
    expect(screen.getByRole("checkbox", { name: "Include review.md" })).toBeChecked();
    expect(screen.getByText("已验证")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "审阅结论和附件" }));
    const payload = await screen.findByRole("textbox", { name: "Approved payload" });
    expect(payload).toHaveValue("Edited Codex conclusion");
    expect(screen.getByRole("heading", { name: "准备送回 ChatGPT" })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "批准这段内容" }));
    fireEvent.click(await screen.findByRole("button", { name: "发送给当前 ChatGPT" }));
    await screen.findByText(/Handoff 已发送不等于 ChatGPT 已完成/);
    const calls = vi.mocked(fetch).mock.calls;
    expect(calls.find(([url]) => String(url).endsWith("/codex-handoff-review"))?.[1]).toMatchObject({ method: "POST", body: JSON.stringify({ runId: "codex-run", attachmentIds: ["attachment-1"] }) });
    expect(calls.find(([url]) => String(url).includes("/codex-handoff-review/") && String(url).endsWith("/approve"))?.[1]).toMatchObject({ method: "POST", body: JSON.stringify({ revision: 6, message: "Edited Codex conclusion" }) });
    expect(calls.find(([url]) => String(url).includes("/codex-handoff-review/") && String(url).endsWith("/send"))?.[1]).toMatchObject({ method: "POST", body: JSON.stringify({ revision: 7 }) });
  });
  it("renders and responds to a structured Codex request without exposing raw identifiers", async () => {
    render(<MobileWorkbench />);
    fireEvent.click((await screen.findAllByRole("button", { name: /Mobile slice/ }))[0]);
    expect(await screen.findByRole("heading", { name: "Codex 需要你批准命令" })).toBeVisible();
    expect(screen.getByText(/当前 turn 已暂停/)).toBeVisible();
    expect(screen.getByText(/只有 app-server 收到的当前 exact Codex request/)).toBeVisible();
    expect(screen.queryByText("request-1")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "允许这一次" }));
    await waitFor(() => expect(vi.mocked(fetch).mock.calls.some(([url]) => String(url).endsWith("/codex-requests/request-1/respond"))).toBe(true));
    const responseCall = vi.mocked(fetch).mock.calls.find(([url]) => String(url).endsWith("/codex-requests/request-1/respond"));
    expect(responseCall?.[1]).toMatchObject({ method: "POST", body: JSON.stringify({ revision: 7, decision: "accept" }) });
    expect(await screen.findByText(/回应已送达 · 等待当前 turn 继续/)).toBeVisible();
    expect(screen.getByRole("button", { name: "允许这一次" })).toBeDisabled();
  });
  it("projects actual Codex attention instead of inferring a ChatGPT result from provider status", async () => {
    vi.mocked(fetch).mockImplementation((input: RequestInfo | URL,init?:RequestInit) => {
      const url = String(input);
      if (url.endsWith("/workstreams")) return Promise.resolve(response([{ id: "ws-1", name: "Mobile slice", attentionCount: 1, attentionItems: [{ kind: "CODEX_RESULT_READY", priority: 0 }], chatgptStatus: "COMPLETED", codexStatus: "COMPLETED" }]));
      return Promise.reject(new Error(`Unexpected request ${url}`));
    });
    render(<MobileWorkbench />);
    expect(await screen.findByText(/Codex 有新结果/)).toBeVisible();
    expect(screen.queryByText(/ChatGPT 有新结果/)).toBeNull();
  });
  it("loads history only after progressive disclosure opens", async () => {
    render(<MobileWorkbench />);
    fireEvent.click((await screen.findAllByRole("button", { name: /Mobile slice/ }))[0]);
    await screen.findByText("Full durable ChatGPT reply");
    expect(vi.mocked(fetch).mock.calls.some(([url]) => String(url).endsWith("/chatgpt-history") || String(url).endsWith("/codex-history"))).toBe(false);
    fireEvent.click(screen.getByText("技术详情与只读历史"));
    await waitFor(() => expect(vi.mocked(fetch).mock.calls.some(([url]) => String(url).endsWith("/chatgpt-history"))).toBe(true));
    expect(await screen.findByText("Codex history")).toBeVisible();
  });
  it("refreshes only the selected semantic result and request state", async () => {
    let refresh = false;
    vi.mocked(fetch).mockImplementation((input: RequestInfo | URL,init?:RequestInit) => {
      const url = String(input);
      if (url.endsWith("/workstreams")) return Promise.resolve(response([{ id: "ws-1", name: "Mobile slice", attentionCount: 1, attentionItems: [{ kind: "CODEX_RESULT_READY", priority: 0 }] }]));
      if (url.endsWith("/chatgpt-result-recovery") && init?.method === "POST") return Promise.resolve(response({ recovered: true, message: "Recovered exact result without sending" }));
      if (url.endsWith("/review-results")) return Promise.resolve(response(refresh ? [{ runId: "new-run", provider: "CODEX", resultIdentity: "new-result", text: "Revised Codex result", attachments: [] }] : [{ runId: "old-run", provider: "CHATGPT", resultIdentity: "old-result", text: "Original ChatGPT result", markerText: "Exact instruction" }]));
      if (url.endsWith("/codex-requests")) return Promise.resolve(response(refresh ? [] : [{ requestId: "request-1", revision: 1, method: "item/commandExecution/requestApproval", kind: "COMMAND_APPROVAL", choices: [], questions: [], isBlocking: true, responseSent: false }]));
      return Promise.reject(new Error(`Unexpected request ${url}`));
    });
    render(<MobileWorkbench />);
    fireEvent.click((await screen.findAllByRole("button", { name: /Mobile slice/ }))[0]);
    expect(await screen.findByText("Original ChatGPT result")).toBeVisible();
    expect(screen.getByRole("heading", { name: "Codex 需要你批准命令" })).toBeVisible();
    refresh = true;
    fireEvent.click(screen.getByRole("button", { name: "刷新当前状态" }));
    expect(await screen.findByText("Revised Codex result", { selector: ".mobile-result-text" })).toBeVisible();
    expect(screen.queryByText("Original ChatGPT result")).toBeNull();
    expect(screen.queryByRole("heading", { name: "Codex 需要你批准命令" })).toBeNull();
    expect(vi.mocked(fetch).mock.calls.some(([url, init]) => String(url).endsWith("/chatgpt-result-recovery") && init?.method === "POST")).toBe(true);
  });
  it("returns from a READY handoff review to the retained complete result without changing it", async () => {
    render(<MobileWorkbench />);
    fireEvent.click((await screen.findAllByRole("button", { name: /Mobile slice/ }))[0]);
    fireEvent.click(await screen.findByRole("button", { name: "只转发指令" }));
    expect(await screen.findByRole("button", { name: /返回完整结果/ })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: /返回完整结果/ }));
    expect(await screen.findByText("Full durable ChatGPT reply")).toBeVisible();
  });
  it("keeps an approved handoff locked instead of offering a back path that could silently edit it", async () => {
    render(<MobileWorkbench />);
    fireEvent.click((await screen.findAllByRole("button", { name: /Mobile slice/ }))[0]);
    fireEvent.click(await screen.findByRole("button", { name: "只转发指令" }));
    fireEvent.click(await screen.findByRole("button", { name: "批准这段内容" }));
    expect(await screen.findByText(/为避免静默解锁/)).toBeVisible();
    expect(screen.queryByRole("button", { name: "返回完整结果" })).toBeNull();
    expect(screen.getByRole("textbox", { name: "Approved payload" })).toBeDisabled();
  });
  it("keeps a Codex delivery as SENDING when exact acceptance is unproven", async () => {
    const original = vi.mocked(fetch).getMockImplementation();
    vi.mocked(fetch).mockImplementation((input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (url.includes("/codex-handoff-review/") && url.endsWith("/send") && init?.method === "POST") return Promise.resolve(response({ status: "SENDING" }));
      if (url.endsWith("/codex-handoff-review/codex-action-1") && !init?.method) return Promise.resolve(response({ actionId: "codex-action-1", revision: 8, status: "SENDING", message: "Edited Codex conclusion" }));
      return original!(input, init);
    });
    render(<MobileWorkbench />);
    fireEvent.click((await screen.findAllByRole("button", { name: /Mobile slice/ }))[0]);
    fireEvent.click(await screen.findByRole("button", { name: "转发结论 + 附件" }));
    fireEvent.click(await screen.findByRole("button", { name: "审阅结论和附件" }));
    fireEvent.click(await screen.findByRole("button", { name: "批准这段内容" }));
    fireEvent.click(await screen.findByRole("button", { name: "发送给当前 ChatGPT" }));
    expect(await screen.findByRole("heading", { name: "正在确认送达" })).toBeVisible();
    expect(screen.getAllByText("交付状态待确认")).toHaveLength(2);
    expect(screen.queryByText("Handoff · 已发送")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "刷新当前状态" }));
    await waitFor(() => expect(vi.mocked(fetch).mock.calls.some(([url, init]) => String(url).endsWith("/codex-handoff-review/codex-action-1") && !init?.method)).toBe(true));
  });
  it("keeps the reviewable result available when lazy read-only detail calls fail", async () => {
    vi.mocked(fetch).mockImplementation((input: RequestInfo | URL,init?:RequestInit) => {
      const url = String(input);
      if (url.endsWith("/workstreams")) return Promise.resolve(response([{ id: "ws-1", name: "Mobile slice", attentionCount: 1, attentionItems: [{ kind: "CHATGPT_RESULT_READY", priority: 0 }], chatgptStatus: "READY", codexStatus: "IDLE" }]));
      if (url.endsWith("/review-results")) return Promise.resolve(response([{ runId: "chat-run", provider: "CHATGPT", resultIdentity: "chat-result", text: "Observed durable result", markerText: "Exact instruction" }]));
      if (url.endsWith("/codex-requests")) return Promise.resolve(response([]));
      if (url.endsWith("/chatgpt-history") || url.endsWith("/codex-history")) return Promise.resolve(failure(503, {}));
      return Promise.reject(new Error(`Unexpected request ${url}`));
    });
    render(<MobileWorkbench />);
    fireEvent.click((await screen.findAllByRole("button", { name: /Mobile slice/ }))[0]);
    expect(await screen.findByText("Observed durable result")).toBeVisible();
    expect(screen.getByRole("button", { name: "只转发指令" })).toBeEnabled();
    expect(vi.mocked(fetch).mock.calls.some(([url]) => String(url).endsWith("-history"))).toBe(false);
    fireEvent.click(screen.getByText("技术详情与只读历史"));
    expect(await screen.findByText(/HTTP 503 \(Router HTTP 503\)/)).toBeVisible();
    expect(screen.queryByRole("alert")).toBeNull();
  });
  it("surfaces a failed initial load without offering a handoff", async () => { vi.mocked(fetch).mockRejectedValueOnce(new Error("Access unavailable")); render(<MobileWorkbench />); await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("Access unavailable")); expect(screen.queryByRole("button", { name: "Prepare for Codex" })).toBeNull(); });
});
