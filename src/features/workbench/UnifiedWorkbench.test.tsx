// Historical V3/V5 projection contract. Native R2 is tested in NativeWorkbench.test.tsx.
import { useState } from "react";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { LegacyWorkbench as UnifiedWorkbench, type WorkbenchSurface } from "./UnifiedWorkbench";

function renderWorkbench() {
  const callbacks = {
    onSelectWorkstream: vi.fn(), onDraftChange: vi.fn(), onPinChange: vi.fn(), onLifecycleChange: vi.fn(), onCreateVerifiedBackup: vi.fn(), onReplyRead: vi.fn(), onReplyHandled: vi.fn(), onAddSelectionToHandoff: vi.fn(), onPrepareHandoff: vi.fn(), onHandoffMessageChange: vi.fn(), onApproveHandoff: vi.fn(), onSendHandoff: vi.fn(), onGoalAction: vi.fn(),
  };
  render(<UnifiedWorkbench {...callbacks} selectedWorkstreamId="work-1" items={[{ id: "work-1", name: "发布计划", projectName: "Router", lifecycle: "ACTIVE", pinned: true, attentionCount: 2, statusLabel: "等待审阅", updatedAt: Date.UTC(2026, 8, 10) }, { id: "work-2", name: "旧测试", projectName: "Router", lifecycle: "TRASHED", updatedAt: Date.UTC(2026, 8, 9) }]} reply={{ id: "reply-1", workstreamId: "work-1", provider: "CHATGPT", title: "完整回复", observedAt: Date.UTC(2026, 8, 10), readState: "UNREAD", text: "# 完整结果\n\n正文与 `代码`。", completeness: "COMPLETE", sourceLabel: "测试精确来源" }} draft={{ value: "已有草稿", saveState: "SAVED" }} handoff={{ id: "handoff-1", status: "READY", direction: "CHATGPT_TO_CODEX", message: "手动补充", candidates: [{ id: "candidate-1", label: "选文候选", text: "候选正文" }], canApprove: true, canSend: false }} goal={{ threadId: "thread-exact", text: "V3 implementation", status: "ACTIVE", controllableActions: ["PAUSE", "DELETE"], readAt: Date.UTC(2026, 8, 10) }} lifecycle={{ canArchive: true, canTrash: true, canRestore: true, trashCount: 1 }} runtime={{ checks: [{ id: "host", label: "Host", state: "READY", detail: "正在监听" }] }} />);
  return callbacks;
}

describe("UnifiedWorkbench", () => {
  afterEach(cleanup);

  it("keeps role-mode utility surfaces and critical recovery visible without stacking the role reader", () => {
    const common = { onSelectWorkstream:vi.fn(), onDraftChange:vi.fn(), items:[{id:"work-1",name:"当前工作",lifecycle:"ACTIVE" as const}], selectedWorkstreamId:"work-1", draft:{value:""}, roleCompatible:true, bridgePanel:<div>role original</div>, criticalNotice:<div role="status">自动操作保持暂停</div>, goal:{threadId:"thread-exact",text:"独立目标",status:"ACTIVE" as const,controllableActions:[],readAt:0} };
    const {rerender}=render(<UnifiedWorkbench {...common} surface="WORKSPACE"/>);
    expect(screen.getByText("role original")).toBeVisible();
    rerender(<UnifiedWorkbench {...common} surface="GOAL"/>);
    expect(screen.getByRole("region",{name:"Goal 控制"})).toBeVisible();
    expect(screen.getByText("role original")).not.toBeVisible();
    expect(screen.getByRole("status")).toHaveTextContent("自动操作保持暂停");
    rerender(<UnifiedWorkbench {...common} surface="INBOX"/>);
    expect(screen.getByText("role original")).not.toBeVisible();
    expect(screen.getByRole("status")).toHaveTextContent("自动操作保持暂停");
  });

  it("lets a phone owner close the workstream action menu without choosing an action", () => {
    const originalMatchMedia = window.matchMedia;
    Object.defineProperty(window, "matchMedia", { configurable: true, value: vi.fn().mockImplementation((query: string) => ({ matches: query === "(max-width: 680px)", addEventListener: vi.fn(), removeEventListener: vi.fn() })) });
    try {
      renderWorkbench();
      fireEvent.click(screen.getByRole("button", { name: "更多工作区操作" }));
      expect(screen.getByRole("navigation", { name: "工作区更多操作" })).toBeVisible();
      fireEvent.click(screen.getByRole("button", { name: "关闭工作区操作菜单" }));
      expect(screen.queryByRole("navigation", { name: "工作区更多操作" })).toBeNull();
    } finally {
      Object.defineProperty(window, "matchMedia", { configurable: true, value: originalMatchMedia });
    }
  });

  it("uses the Figma START shell only when Router has no local workstream, and its entry cards navigate without selecting an identity", async () => {
    const user = userEvent.setup();
    const onSurfaceChange = vi.fn();
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} onSurfaceChange={onSurfaceChange} items={[]} draft={{ value: "" }} />);
    expect(screen.getByRole("region", { name: "V3.1 工作台入口" })).toBeVisible();
    expect(screen.getByText("少找入口，多把事情做完。")).toBeVisible();
    await user.click(screen.getByRole("button", { name: /外部项目与对话连接/ }));
    expect(onSurfaceChange).toHaveBeenCalledWith("PROJECT_HOME");
    await user.click(screen.getByRole("button", { name: /Codex 结果与一次性授权/ }));
    expect(onSurfaceChange).toHaveBeenCalledWith("NEW_WORK");
  });

  it("keeps the D02 rail list-first and routes archive controls outside the reading shell", () => {
    const callbacks = renderWorkbench();
    expect(screen.getByRole("region", { name: "置顶工作" })).toBeVisible();
    expect(screen.queryByRole("tablist", { name: "工作区状态" })).toBeNull();
    expect(screen.queryByRole("button", { name: "当前目标" })).toBeNull();
    expect(screen.queryByRole("button", { name: "取消置顶" })).toBeNull();
    expect(screen.queryByRole("button", { name: "移至回收站" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "发布计划" }));
    expect(callbacks.onSelectWorkstream).toHaveBeenCalledWith("work-1");
    fireEvent.contextMenu(screen.getByRole("button", { name: "发布计划" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "取消置顶" }));
    expect(callbacks.onPinChange).toHaveBeenCalledWith("work-1", false);
    expect(screen.getByRole("button", { name: "归档与回收站" })).toBeVisible();
    expect(screen.getByRole("button", { name: /电脑正在运行/ })).toHaveTextContent("2 项状态需要核对");
  });

  it("requires a separate user action to create a consistent local backup", () => {
    const callbacks = renderWorkbench();
    fireEvent.click(screen.getByRole("button", { name: "归档与回收站" }));
    fireEvent.click(screen.getByRole("button", { name: "整理测试工作区" }));
    fireEvent.click(screen.getByRole("button", { name: "创建一致性备份" }));
    expect(callbacks.onCreateVerifiedBackup).toHaveBeenCalledTimes(1);
  });

  it("keeps each Figma runtime-row action tied to the Host capability supplied for that row", () => {
    const onOpenDiagnostics = vi.fn();
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} surface="RUNTIME" items={[]} draft={{ value: "" }} runtime={{ checks: [{ id: "chatgpt", label: "Playwright 载体", state: "WARNING", detail: "载体尚未连接", action: onOpenDiagnostics, actionLabel: "查看详情" }] }} />);
    const refresh = screen.getByRole("button", { name: "查看详情" });
    fireEvent.click(refresh);
    expect(onOpenDiagnostics).toHaveBeenCalledOnce();
    expect(screen.queryByRole("button", { name: "查看详情 Playwright 载体" })).toBeNull();
    expect(screen.getByLabelText("Playwright 载体 的当前操作位于页面顶部")).toBeVisible();
  });

  it("presents the exact-page Connector without reviving a Router-owned carrier", () => {
    const onRefresh = vi.fn();
    const onOpenDefaultBrowser = vi.fn();
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} surface="CHATGPT_DIAGNOSTICS" items={[]} draft={{ value: "" }} runtime={{ checks: [{ id: "browser-runtime", label: "ChatGPT 自动载体（已暂停）", state: "UNAVAILABLE", detail: "需要查看时可在默认浏览器打开这个精确绑定对话。", secondaryAction: onOpenDefaultBrowser, secondaryActionLabel: "在默认浏览器打开当前对话" }, { id: "chatgpt", label: "ChatGPT 自动读取（已暂停）", state: "UNAVAILABLE", detail: "默认浏览器中的内容不会被 Router 导入或自动检查。" }], onOpenDiagnostics: onRefresh }} />);
    const diagnostics = screen.getByRole("region", { name: "ChatGPT 连接诊断" });
    expect(diagnostics).toHaveTextContent("使用你日常 Chrome 的当前精确对话");
    expect(diagnostics).toHaveTextContent("Connector 只观察已打开且 conversation ID 精确匹配的页面");
    expect(diagnostics).toHaveTextContent("一次文字发送只能来自精确绑定对话的 Review → Edit → Approve → Send");
    expect(diagnostics).toHaveTextContent("chrome://extensions");
    expect(within(diagnostics).queryByRole("button", { name: /打开 Router 专用浏览器/ })).toBeNull();
    expect(diagnostics).not.toHaveTextContent("Playwright");
    fireEvent.click(within(diagnostics).getByRole("button", { name: "在默认浏览器打开当前对话" }));
    expect(onOpenDefaultBrowser).toHaveBeenCalledOnce();
    fireEvent.click(within(diagnostics).getByRole("button", { name: "重新检查 Router 运行状态" }));
    expect(onRefresh).toHaveBeenCalledOnce();
  });

  it("lets an unavailable manually-bound ChatGPT reader open only the exact default-browser link", () => {
    const onOpenBoundChatGptInDefaultBrowser = vi.fn();
    const onRetryCurrentChatGptHistory = vi.fn();
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} items={[{ id: "work-1", name: "人工核对", lifecycle: "ACTIVE" }]} selectedWorkstreamId="work-1" draft={{ value: "" }} reply={{ id: "reply-manual", workstreamId: "work-1", provider: "CHATGPT", title: "人工查看", observedAt: Date.UTC(2026, 8, 27), readState: "READ", text: "当前内容不会由 Router 读取。", completeness: "UNAVAILABLE", readOnly: true, historyReadFailure: "MANUAL_READ_REQUIRED" }} onOpenBoundChatGptInDefaultBrowser={onOpenBoundChatGptInDefaultBrowser} onRetryCurrentChatGptHistory={onRetryCurrentChatGptHistory} />);
    const recovery = screen.getByText(/自动读取已暂停。此入口只让你在默认浏览器人工查看/).closest("aside");
    expect(recovery).not.toBeNull();
    expect(recovery).toHaveTextContent("自动读取已暂停");
    expect(within(recovery!).getByRole("button", { name: "在默认浏览器查看当前对话" })).toBeVisible();
    expect(within(recovery!).queryByRole("button", { name: "重新读取当前对话" })).toBeNull();
    expect(screen.queryByRole("button", { name: "打开专用浏览器登录" })).toBeNull();
    fireEvent.click(within(recovery!).getByRole("button", { name: "在默认浏览器查看当前对话" }));
    expect(onOpenBoundChatGptInDefaultBrowser).toHaveBeenCalledOnce();
    expect(onRetryCurrentChatGptHistory).not.toHaveBeenCalled();
  });

  it("keeps the D12 recycle selection, exact restore, and separately confirmed local purge distinct", () => {
    const onLifecycleChange = vi.fn();
    const onPurgeTrashedWorkstream = vi.fn();
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} surface="RECYCLE" selectedWorkstreamId="trash-exact" lifecycle={{ canPurge: true,canArchive:false,canTrash:false,canRestore:false }} items={[{ id: "trash-exact", name: "旧测试工作", lifecycle: "TRASHED", updatedAt: Date.UTC(2026, 8, 9) }, { id: "archive-exact", name: "已归档工作", lifecycle: "ARCHIVED", updatedAt: Date.UTC(2026, 8, 8) }]} draft={{ value: "" }} onLifecycleChange={onLifecycleChange} onPurgeTrashedWorkstream={onPurgeTrashedWorkstream} />);
    const recycle = screen.getByRole("region", { name: "归档与回收站" });
    fireEvent.click(within(recycle).getByRole("button", { name: "恢复" }));
    expect(onLifecycleChange).toHaveBeenCalledWith("trash-exact", "ACTIVE");
    expect(within(recycle).getByRole("button", { name: "永久清除选中项" })).toBeEnabled();
    expect(onPurgeTrashedWorkstream).not.toHaveBeenCalled();
  });

  it("keeps D13 test cleanup blocked without an approved exact manifest while allowing only a verified backup", () => {
    const onCreateVerifiedBackup = vi.fn();
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} surface="TEST_CLEANUP" items={[]} draft={{ value: "" }} onCreateVerifiedBackup={onCreateVerifiedBackup} />);
    const cleanup = screen.getByRole("region", { name: "整理测试工作区" });
    expect(cleanup).toHaveTextContent("当前没有已核验的测试清单");
    expect(cleanup).toHaveTextContent("不能备份并移入回收站");
    fireEvent.click(within(cleanup).getByRole("button", { name: "创建一致性备份" }));
    expect(onCreateVerifiedBackup).toHaveBeenCalledOnce();
    expect(screen.queryByRole("button", { name: "备份并移入回收站" })).toBeNull();
  });

  it("keeps reading actions, draft autosave callback, and handoff approval distinct", async () => {
    const callbacks = renderWorkbench();
    fireEvent.click(screen.getByRole("button", { name: "标为已读" }));
    fireEvent.change(screen.getByLabelText("继续当前精确对话"), { target: { value: "更新后的草稿" } });
    fireEvent.click(screen.getByRole("button", { name: "审阅这条 ChatGPT 回复" }));
    fireEvent.click(await screen.findByRole("button", { name: /^选文候选/ }));
    fireEvent.click(screen.getByRole("button", { name: "批准此版本（不会发送）" }));
    expect(callbacks.onReplyRead).toHaveBeenCalledWith("reply-1");
    expect(callbacks.onDraftChange).toHaveBeenCalledWith("更新后的草稿");
    expect(callbacks.onHandoffMessageChange).toHaveBeenCalledWith("handoff-1", "候选正文");
    expect(callbacks.onApproveHandoff).toHaveBeenCalledWith("handoff-1");
    expect(callbacks.onSendHandoff).not.toHaveBeenCalled();
  });

  it("requires an explicit ChatGPT attachment selection after review and never preselects it", () => {
    const onSelectHandoffAttachments = vi.fn();
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} onSelectHandoffAttachments={onSelectHandoffAttachments} surface="HANDOFF_REVIEW" selectedWorkstreamId="work-1" items={[{ id: "work-1", name: "附件验证", lifecycle: "ACTIVE" }]} draft={{ value: "" }} handoff={{ id: "handoff-attachment", revision: 7, status: "READY", direction: "CHATGPT_TO_CODEX", message: "请读取所选附件", attachmentOptions: ["source.txt"], selectedAttachmentLabels: [], canApprove: true, canSend: false }} />);
    const selection = screen.getByRole("region", { name: "ChatGPT 附件选择" });
    const checkbox = within(selection).getByRole("checkbox", { name: "source.txt" });
    expect(checkbox).not.toBeChecked();
    fireEvent.click(checkbox);
    fireEvent.click(within(selection).getByRole("button", { name: "确认附件选择" }));
    expect(onSelectHandoffAttachments).toHaveBeenCalledWith("handoff-attachment", ["source.txt"]);
  });

  it("keeps D15's approved default copy while retaining draft-save status for assistive technology", () => {
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} surface="DISCUSSION" selectedWorkstreamId="work-1" items={[{ id: "work-1", name: "界面重设计", lifecycle: "ACTIVE" }]} draft={{ value: "已有草稿", saveState: "SAVED", savedAt: Date.UTC(2026, 8, 11, 8, 26) }} />);
    const focus = screen.getByRole("region", { name: "回复 ChatGPT" });
    expect(focus).toHaveTextContent("草稿离开后保留。发送是否送达，与新回复是否生成分开显示。");
    expect(within(focus).getByRole("status")).toHaveTextContent("草稿已自动保存");
    expect(within(focus).getByRole("button", { name: "取消，保留草稿" })).not.toHaveClass("v3-primary");
  });

  it("locks one pending discussion send and keeps the exact draft after failure", async () => {
    let rejectSubmission: (reason?: unknown) => void = () => undefined;
    const onSendDiscussion = vi.fn(() => new Promise<void>((_resolve, reject) => { rejectSubmission = reject; }));
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} onSendDiscussion={onSendDiscussion} surface="DISCUSSION" selectedWorkstreamId="work-1" items={[{ id: "work-1", name: "界面重设计", lifecycle: "ACTIVE" }]} draft={{ value: "仅发送给当前精确对话", saveState: "SAVED" }} />);
    const focus = screen.getByRole("region", { name: "回复 ChatGPT" });
    const send = within(focus).getByRole("button", { name: "发送给 ChatGPT" });
    fireEvent.click(send);
    fireEvent.click(send);
    expect(onSendDiscussion).toHaveBeenCalledTimes(1);
    expect(send).toBeDisabled();
    expect(send).toHaveTextContent("正在发送讨论…");
    rejectSubmission(new Error("exact adapter unavailable"));
    await waitFor(() => expect(send).toBeEnabled());
    expect(within(focus).getByLabelText("你的修改意见")).toHaveValue("仅发送给当前精确对话");
  });

  it("offers owner-led copy and exact destination instead of a paused Router discussion send", async () => {
    const resolveDestination = vi.fn().mockResolvedValue("https://chatgpt.com/c/exact-conversation");
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} onOpenManualDiscussionDestination={resolveDestination} surface="DISCUSSION" selectedWorkstreamId="work-1" items={[{ id: "work-1", name: "界面重设计", lifecycle: "ACTIVE" }]} draft={{ value: "只由 owner 手动发送", saveState: "SAVED" }} />);
    const focus = screen.getByRole("region", { name: "回复 ChatGPT" });
    expect(within(focus).getByText("Router 的 ChatGPT 自动写入已暂停。草稿不会自动发送或标为送达：请复制后，在当前精确绑定的 ChatGPT 对话中自行粘贴并发送。")).toBeVisible();
    expect(within(focus).queryByRole("button", { name: "发送给 ChatGPT" })).toBeNull();
    fireEvent.click(within(focus).getByRole("button", { name: "在默认浏览器查看当前精确 ChatGPT 对话" }));
    await waitFor(() => expect(resolveDestination).toHaveBeenCalledOnce());
    expect(within(focus).getByRole("link", { name: "打开已核对的 ChatGPT 对话" })).toHaveAttribute("href", "https://chatgpt.com/c/exact-conversation");
  });

  it("keeps an unmarked exact ChatGPT reply on the ordinary discussion path while offering explicit scope editing", () => {
    const onSurfaceChange = vi.fn();
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} onSurfaceChange={onSurfaceChange} onAddSelectionToHandoff={vi.fn()} selectedWorkstreamId="work-1" items={[{ id: "work-1", name: "界面重设计", lifecycle: "ACTIVE" }]} draft={{ value: "" }} reply={{ id: "reply-ordinary", workstreamId: "work-1", provider: "CHATGPT", observedAt: Date.UTC(2026, 8, 11, 8, 26), readState: "UNREAD", text: "普通讨论，没有交接标记。", completeness: "COMPLETE" }} />);
    fireEvent.click(screen.getByRole("button", { name: "回复 ChatGPT" }));
    expect(onSurfaceChange).toHaveBeenCalledWith("DISCUSSION");
    expect(screen.queryByRole("button", { name: "准备交接给 Codex" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "选择/编辑交给 Codex 的范围" })).toBeVisible();
    expect(screen.getByRole("button", { name: "选择范围" })).toBeVisible();
  });

  it("labels the active reader with the provider that supplied the displayed reply", () => {
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} selectedWorkstreamId="work-1" items={[{ id: "work-1", name: "来源核对", lifecycle: "ACTIVE" }]} draft={{ value: "" }} reply={{ id: "reply-codex", workstreamId: "work-1", provider: "CODEX", observedAt: 1, readState: "READ", text: "这是当前精确 Codex 回复。", completeness: "COMPLETE" }} hasCodexEndpoint />);
    const source = screen.getByRole("navigation", { name: "当前阅读来源" });
    expect(within(source).getByText("Codex 最新回复")).toHaveAttribute("aria-current", "page");
    expect(within(source).getByRole("button", { name: "Codex 对话记录" })).toBeVisible();
    expect(screen.queryByText("ChatGPT 最新回复")).toBeNull();
  });

  it("does not render the ChatGPT check command on a Codex reply", () => {
    const onCheckNewChatGptReplies = vi.fn();
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} onCheckNewChatGptReplies={onCheckNewChatGptReplies} selectedWorkstreamId="work-1" items={[{ id: "work-1", name: "来源核对", lifecycle: "ACTIVE" }]} draft={{ value: "" }} reply={{ id: "reply-codex", workstreamId: "work-1", provider: "CODEX", observedAt: 1, readState: "READ", text: "这是当前精确 Codex 回复。", completeness: "COMPLETE" }} />);
    expect(screen.queryByRole("button", { name: "检查新回复" })).toBeNull();
    expect(onCheckNewChatGptReplies).not.toHaveBeenCalled();
  });

  it("moves an explicit Reader text selection into the separate Handoff review surface", async () => {
    const onAddSelectionToHandoff = vi.fn();
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} onAddSelectionToHandoff={onAddSelectionToHandoff} selectedWorkstreamId="work-1" items={[{ id: "work-1", name: "界面重设计", lifecycle: "ACTIVE" }]} draft={{ value: "" }} reply={{ id: "reply-selection", workstreamId: "work-1", provider: "CHATGPT", observedAt: 1, readState: "READ", text: "可补充交接的完整回复。", completeness: "COMPLETE" }} />);
    const reader = screen.getByLabelText("完整回复");
    vi.spyOn(window, "getSelection").mockReturnValue({ toString: () => "补充段落", anchorNode: reader } as unknown as Selection);
    fireEvent.mouseUp(reader);
    fireEvent.click(screen.getByRole("button", { name: "核对这段范围，进入审阅" }));
    expect(screen.getByRole("region", { name: "交接指令范围" })).toHaveTextContent("补充段落");
    expect(onAddSelectionToHandoff).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "确认范围，进入审阅" }));
    expect(onAddSelectionToHandoff).toHaveBeenCalledWith("reply-selection", "补充段落");
  });

  it("requires an owner-confirmed instruction range when natural language does not provide a marker", () => {
    const onAddSelectionToHandoff = vi.fn();
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} onAddSelectionToHandoff={onAddSelectionToHandoff} selectedWorkstreamId="work-1" items={[{ id: "work-1", name: "范围核对", lifecycle: "ACTIVE" }]} draft={{ value: "" }} reply={{ id: "reply-range", workstreamId: "work-1", provider: "CHATGPT", observedAt: 1, readState: "READ", text: "上面的内容交给 Codex。\n\n实际目标：只验证手机入口。", completeness: "COMPLETE" }} />);

    fireEvent.click(screen.getByRole("button", { name: "选择范围" }));
    expect(screen.getByRole("region", { name: "交接指令范围" })).toHaveTextContent("Router 不会从“上面”“这段”等自然语言猜测范围");
    fireEvent.change(screen.getByRole("textbox", { name: "交接指令范围" }), { target: { value: "实际目标：只验证手机入口。" } });
    fireEvent.click(screen.getByRole("button", { name: "确认范围，进入审阅" }));
    expect(onAddSelectionToHandoff).toHaveBeenCalledWith("reply-range", "实际目标：只验证手机入口。");
  });

  it("keeps the Reader in place when selected-text preparation fails", async () => {
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} onAddSelectionToHandoff={() => Promise.reject(new Error("exact handoff unavailable"))} selectedWorkstreamId="work-1" items={[{ id: "work-1", name: "界面重设计", lifecycle: "ACTIVE" }]} draft={{ value: "" }} reply={{ id: "reply-selection-fail", workstreamId: "work-1", provider: "CHATGPT", observedAt: 1, readState: "READ", text: "保持当前阅读。", completeness: "COMPLETE" }} />);
    const reader = screen.getByLabelText("完整回复");
    vi.spyOn(window, "getSelection").mockReturnValue({ toString: () => "失败段落", anchorNode: reader } as unknown as Selection);
    fireEvent.mouseUp(reader);
    fireEvent.click(screen.getByRole("button", { name: "核对这段范围，进入审阅" }));
    fireEvent.click(screen.getByRole("button", { name: "确认范围，进入审阅" }));
    await waitFor(() => expect(screen.getByRole("region", { name: "交接指令范围" })).toBeVisible());
    expect(screen.queryByRole("region", { name: "人工交接审阅" })).toBeNull();
  });

  it("keeps the Reader in place when marker-based preparation fails", async () => {
    const onSurfaceChange = vi.fn();
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} onSurfaceChange={onSurfaceChange} onPrepareHandoff={() => Promise.reject(new Error("marker handoff unavailable"))} selectedWorkstreamId="work-1" items={[{ id: "work-1", name: "界面重设计", lifecycle: "ACTIVE" }]} draft={{ value: "" }} reply={{ id: "reply-marker-fail", workstreamId: "work-1", provider: "CHATGPT", observedAt: 1, readState: "READ", text: "已识别交接标记的完整回复。", completeness: "COMPLETE" }} />);
    fireEvent.click(screen.getByRole("button", { name: "审阅这条 ChatGPT 回复" }));
    await waitFor(() => expect(onSurfaceChange).not.toHaveBeenCalled());
  });

  it("opens delivery status for a real sending Handoff without offering a resend", () => {
    const onSurfaceChange = vi.fn();
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} onSurfaceChange={onSurfaceChange} surface="HANDOFF_REVIEW" selectedWorkstreamId="work-1" items={[{ id: "work-1", name: "界面重设计", lifecycle: "ACTIVE" }]} draft={{ value: "" }} handoff={{ id: "handoff-sending", status: "SENDING", direction: "CODEX_TO_CHATGPT", message: "已批准内容", canApprove: false, canSend: false }} />);
    expect(screen.getByRole("button", { name: "查看送达状态" })).toBeVisible();
    expect(screen.queryByRole("button", { name: /发送给/ })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "查看送达状态" }));
    expect(onSurfaceChange).toHaveBeenCalledWith("HANDOFF_STATUS");
  });

  it("locks one pending approved Handoff send before it can be dispatched twice", async () => {
    let resolveSend: () => void = () => undefined;
    const onSendHandoff = vi.fn(() => new Promise<void>((resolve) => { resolveSend = resolve; }));
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} onSendHandoff={onSendHandoff} surface="HANDOFF_REVIEW" selectedWorkstreamId="work-1" items={[{ id: "work-1", name: "界面重设计", lifecycle: "ACTIVE" }]} draft={{ value: "" }} handoff={{ id: "handoff-approved", status: "APPROVED", direction: "CHATGPT_TO_CODEX", message: "已批准内容", canApprove: false, canSend: true }} />);
    const send = screen.getByRole("button", { name: "单独发送给 Codex" });
    fireEvent.click(send);
    fireEvent.click(send);
    expect(onSendHandoff).toHaveBeenCalledTimes(1);
    expect(send).toBeDisabled();
    expect(send).toHaveTextContent("正在发送给 Codex…");
    resolveSend();
    await waitFor(() => expect(send).toBeEnabled());
  });

  it("locks one pending approval so an owner cannot approve the same review twice", async () => {
    let resolveApproval: () => void = () => undefined;
    const onApproveHandoff = vi.fn(() => new Promise<void>((resolve) => { resolveApproval = resolve; }));
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} onApproveHandoff={onApproveHandoff} surface="HANDOFF_REVIEW" selectedWorkstreamId="work-1" items={[{ id: "work-1", name: "界面重设计", lifecycle: "ACTIVE" }]} draft={{ value: "" }} handoff={{ id: "handoff-ready", status: "READY", direction: "CHATGPT_TO_CODEX", message: "待批准内容", canApprove: true, canSend: false }} />);
    const approve = screen.getByRole("button", { name: "批准此版本（不会发送）" });
    fireEvent.click(approve);
    fireEvent.click(approve);
    expect(onApproveHandoff).toHaveBeenCalledTimes(1);
    expect(approve).toBeDisabled();
    expect(approve).toHaveTextContent("正在批准此版本…");
    resolveApproval();
    await waitFor(() => expect(approve).toBeEnabled());
  });

  it("names a retained Codex ProviderRun as a historical result, not an external reply", () => {
    const onOpenCodexResults = vi.fn();
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} onOpenCodexResults={onOpenCodexResults} surface="HANDOFF_REVIEW" selectedWorkstreamId="work-1" items={[{ id: "work-1", name: "来源核对", lifecycle: "ACTIVE" }]} draft={{ value: "" }} handoff={{ id: "handoff-history", status: "READY", direction: "CODEX_TO_CHATGPT", message: "保留的结果", canApprove: true, canSend: false, origin: { provider: "CODEX", sourceKind: "PROVIDER_RUN" } }} />);
    fireEvent.click(screen.getByRole("button", { name: "查看 Codex 完整原结果" }));
    expect(onOpenCodexResults).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("button", { name: "查看 Codex 完整原回复" })).not.toBeInTheDocument();
  });

  it("offers a forward Handoff its exact ChatGPT source instead of a generic return", () => {
    const onOpenChatGptOrigin = vi.fn();
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} onOpenChatGptOrigin={onOpenChatGptOrigin} surface="HANDOFF_REVIEW" selectedWorkstreamId="work-1" items={[{ id: "work-1", name: "来源核对", lifecycle: "ACTIVE" }]} draft={{ value: "" }} handoff={{ id: "handoff-forward", status: "APPROVED", direction: "CHATGPT_TO_CODEX", message: "已批准内容", canApprove: false, canSend: true, origin: { provider: "CHATGPT", sourceKind: "REPLY_OBSERVATION" } }} />);
    fireEvent.click(screen.getByRole("button", { name: "查看 ChatGPT 完整原回复" }));
    expect(onOpenChatGptOrigin).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button", { name: "返回原回复，另建审阅版本" })).toBeVisible();
    expect(screen.queryByRole("button", { name: "返回编辑（需重新批准）" })).toBeNull();
  });

  it("keeps a sending delivery detail distinct from unknown execution", () => {
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} surface="HANDOFF_STATUS" selectedWorkstreamId="work-1" items={[{ id: "work-1", name: "界面重设计", lifecycle: "ACTIVE" }]} draft={{ value: "" }} handoff={{ id: "handoff-sending", status: "SENDING", direction: "CODEX_TO_CHATGPT", message: "已批准内容", canApprove: false, canSend: false, deliveryDetail: "ChatGPT 已接受请求，仍在确认精确送达。" }} />);
    expect(screen.getByText("正在确认送达")).toBeVisible();
    expect(screen.getByText("ChatGPT 已接受请求，仍在确认精确送达。")).toBeVisible();
    expect(screen.getByText("尚未可确认")).toBeVisible();
    expect(screen.queryByRole("button", { name: /发送给/ })).not.toBeInTheDocument();
  });

  it("marks a proven pre-composer reverse failure as safe for owner-led manual dispatch", () => {
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} surface="HANDOFF_STATUS" items={[]} draft={{ value: "" }} handoff={{ id: "handoff-prewrite", handoffId: "handoff-prewrite", status: "FAILED", direction: "CODEX_TO_CHATGPT", message: "approved exact payload", canApprove: false, canSend: false, deliveryCode: "CHATGPT_AUTH_OR_COMPOSER_REQUIRED", deliveryDetail: "ChatGPT rejected the carrier before text entry; no provider submission occurred", destination: { provider: "CHATGPT", label: "游戏开发管理对话", externalId: "6ab7e39e-d0d0-83ec-87da-31d699cedfd3" } }} />);
    expect(screen.getByText("尚未发送到 ChatGPT")).toBeVisible();
    expect(screen.getByText("Router 在写入输入框前已停止", { exact: false })).toBeVisible();
    expect(screen.getByRole("button", { name: "复制已批准内容，手动发送到 ChatGPT" })).toBeVisible();
    expect(screen.getByRole("region", { name: "手动发送的精确目标" })).toHaveTextContent("游戏开发管理对话");
    expect(screen.getByRole("region", { name: "手动发送的精确目标" })).toHaveTextContent("6ab7e39e-d0d0-83ec-87da-31d699cedfd3");
    fireEvent.click(screen.getByRole("button", { name: "查看已批准的完整内容" }));
    expect(screen.getByRole("region", { name: "已批准内容" })).toHaveTextContent("approved exact payload");
  });

  it("offers the host-guarded exact ChatGPT destination from approved manual review and delivery status", () => {
    const onOpenHandoffDestination = vi.fn();
    const handoff = { id: "handoff-manual", handoffId: "handoff-manual", status: "APPROVED" as const, direction: "CODEX_TO_CHATGPT" as const, message: "approved exact payload", canApprove: false, canSend: false, requiresManualDispatch: true, destination: { provider: "CHATGPT" as const, label: "游戏开发管理对话", externalId: "6ab7e39e-d0d0-83ec-87da-31d699cedfd3" } };
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} onOpenHandoffDestination={onOpenHandoffDestination} surface="HANDOFF_REVIEW" selectedWorkstreamId="work-1" items={[{ id: "work-1", name: "游戏开发", lifecycle: "ACTIVE" }]} draft={{ value: "" }} handoff={handoff} />);
    expect(screen.getByText("Router 尚未把这段内容写入 ChatGPT，也不会把它标记为已送达。")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "在默认浏览器查看这个精确 ChatGPT 对话" }));
    expect(onOpenHandoffDestination).toHaveBeenCalledWith("handoff-manual");

    cleanup();
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} onOpenHandoffDestination={onOpenHandoffDestination} surface="HANDOFF_STATUS" items={[]} draft={{ value: "" }} handoff={{ ...handoff, status: "FAILED", requiresManualDispatch: false, deliveryCode: "CHATGPT_AUTH_OR_COMPOSER_REQUIRED" }} />);
    fireEvent.click(screen.getByRole("button", { name: "在默认浏览器查看这个精确 ChatGPT 对话" }));
    expect(onOpenHandoffDestination).toHaveBeenLastCalledWith("handoff-manual");
    expect(onOpenHandoffDestination).toHaveBeenCalledTimes(2);
  });

  it("renders an already Router-validated manual ChatGPT link instead of opening another destination", () => {
    const onOpenHandoffDestination = vi.fn();
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} onOpenHandoffDestination={onOpenHandoffDestination} surface="HANDOFF_REVIEW" selectedWorkstreamId="work-1" items={[{ id: "work-1", name: "游戏开发", lifecycle: "ACTIVE" }]} draft={{ value: "" }} handoff={{ id: "handoff-link", status: "APPROVED", direction: "CODEX_TO_CHATGPT", message: "approved exact payload", canApprove: false, canSend: false, requiresManualDispatch: true, destination: { provider: "CHATGPT", label: "游戏开发管理对话", externalId: "6ab7e39e-d0d0-83ec-87da-31d699cedfd3", canonicalUrl: "https://chatgpt.com/c/6ab7e39e-d0d0-83ec-87da-31d699cedfd3" } }} />);
    expect(screen.getByRole("link", { name: "打开已核对的 ChatGPT 对话" })).toHaveAttribute("href", "https://chatgpt.com/c/6ab7e39e-d0d0-83ec-87da-31d699cedfd3");
    expect(onOpenHandoffDestination).not.toHaveBeenCalled();
  });

  it("keeps a recovered failed reverse handoff reachable from the ordinary workspace", () => {
    const onSurfaceChange = vi.fn();
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} onPrepareHandoff={vi.fn()} onSurfaceChange={onSurfaceChange} selectedWorkstreamId="work-1" items={[{ id: "work-1", name: "游戏开发", lifecycle: "ACTIVE" }]} draft={{ value: "" }} reply={{ id: "reply-codex", workstreamId: "work-1", provider: "CODEX", observedAt: 1, readState: "READ", text: "当前 Codex 回复", completeness: "COMPLETE" }} handoff={{ id: "handoff-prewrite", handoffId: "handoff-prewrite", status: "FAILED", direction: "CODEX_TO_CHATGPT", message: "approved exact payload", canApprove: false, canSend: false, deliveryCode: "CHATGPT_AUTH_OR_COMPOSER_REQUIRED", deliveryDetail: "ChatGPT rejected the carrier before text entry; no provider submission occurred", destination: { provider: "CHATGPT", label: "游戏开发管理对话", externalId: "6ab7e39e-d0d0-83ec-87da-31d699cedfd3" } }} />);

    fireEvent.click(screen.getByRole("button", { name: "查看未发送的交付记录" }));
    expect(onSurfaceChange).toHaveBeenCalledWith("HANDOFF_STATUS");
    expect(screen.queryByRole("button", { name: "审阅这条 Codex 回复" })).toBeNull();
  });

  it("keeps a recovered approved manual reverse review reachable from the ordinary workspace", () => {
    const onSurfaceChange = vi.fn();
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} onSurfaceChange={onSurfaceChange} selectedWorkstreamId="work-1" items={[{ id: "work-1", name: "游戏开发", lifecycle: "ACTIVE" }]} draft={{ value: "" }} handoff={{ id: "review-approved", status: "APPROVED", direction: "CODEX_TO_CHATGPT", message: "approved exact payload", canApprove: false, canSend: false, requiresManualDispatch: true, destination: { provider: "CHATGPT", label: "游戏开发管理对话", externalId: "6ab86566-a260-83ec-a603-a930845e22c6" } }} />);

    fireEvent.click(screen.getByRole("button", { name: "查看已批准内容并手动发送" }));
    expect(onSurfaceChange).toHaveBeenCalledWith("HANDOFF_STATUS");
  });

  it("keeps a pending Codex review available without hiding the current ChatGPT review action", () => {
    const onSurfaceChange = vi.fn();
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} onPrepareHandoff={vi.fn()} onSurfaceChange={onSurfaceChange} selectedWorkstreamId="work-1" items={[{ id: "work-1", name: "游戏开发", lifecycle: "ACTIVE" }]} draft={{ value: "" }} reply={{ id: "chatgpt-reply", workstreamId: "work-1", provider: "CHATGPT", observedAt: 1, readState: "READ", text: "当前 ChatGPT 回复", completeness: "COMPLETE" }} handoff={{ id: "pending-codex-review", status: "READY", direction: "CODEX_TO_CHATGPT", message: "pending exact Codex payload", canApprove: true, canSend: false }} />);

    expect(screen.getByRole("button", { name: "审阅这条 ChatGPT 回复" })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "继续审阅待批准内容" }));
    expect(onSurfaceChange).toHaveBeenCalledWith("HANDOFF_REVIEW");
  });

  it("keeps the exact receiving Endpoint visible through an approved manual Codex-to-ChatGPT handoff", () => {
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} surface="HANDOFF_REVIEW" items={[]} draft={{ value: "" }} handoff={{ id: "handoff-manual-target", status: "APPROVED", direction: "CODEX_TO_CHATGPT", message: "approved exact payload", canApprove: false, canSend: false, requiresManualDispatch: true, destination: { provider: "CHATGPT", label: "游戏开发管理对话", externalId: "6ab86566-a260-83ec-a603-a930845e22c6" } }} />);
    const review = screen.getByRole("region", { name: "人工交接审阅" });
    expect(review).toHaveTextContent("内容已批准，等待你在 ChatGPT 手动发送");
    const destination = within(review).getByRole("group", { name: "本次接收端精确绑定" });
    expect(destination).toHaveTextContent("游戏开发管理对话");
    expect(destination).toHaveTextContent("6ab86566-a260-83ec-a603-a930845e22c6");
    expect(review).toHaveTextContent("Router 不会按名称或最近对话改写目标");
  });

  it("projects D01 as provider state, work title, and full reader while retaining exact source accessibility", () => {
    renderWorkbench();
    fireEvent.click(screen.getByRole("button", { name: /^收件箱/ }));
    const reader = screen.getByLabelText("完整回复");
    expect(within(reader).getByText("完整回复")).toBeVisible();
    expect(within(reader).getAllByText(/ChatGPT · 未读 ·/)).toHaveLength(2);
    expect(within(reader).getByText(/ChatGPT · 未读 ·/, { selector: ".v3-mobile-reply-meta" })).toBeInTheDocument();
    expect(within(reader).getByTitle("测试精确来源")).toBeVisible();
    expect(within(reader).getByText("完整结果")).toBeVisible();
  });

  it("requires an explicit confirmation for separate Goal controls", () => {
    const callbacks = renderWorkbench();
    fireEvent.click(screen.getByRole("button", { name: "运行环境" }));
    fireEvent.click(screen.getByRole("button", { name: "关闭全部前的影响确认" }));
    fireEvent.click(screen.getByRole("button", { name: "查看当前执行，再退出" }));
    expect(screen.getByRole("heading", { name: "当前目标与执行" })).toBeVisible();
    expect(screen.getByText("目标进行中")).toBeVisible();
    expect(screen.queryByRole("button", { name: "当前目标" })).toBeNull();
    expect(screen.getByRole("button", { name: "工作区" })).toHaveAttribute("aria-current", "page");
    const stop = screen.getByRole("button", { name: "停止本轮执行" });
    expect(stop).toHaveAttribute("disabled");
    expect(stop).toHaveAttribute("aria-describedby", "goal-control-unavailable-reason");
    expect(document.getElementById("goal-control-unavailable-reason")).toHaveTextContent("Router 尚未取得对应官方控制权");
    const unavailable = document.querySelector<HTMLButtonElement>(".v3-goal-unavailable-trigger");
    expect(unavailable).not.toBeNull();
    fireEvent.click(unavailable!);
    expect(screen.getByRole("status", { name: "控制不可用状态详情" })).toHaveTextContent("停止本轮执行");
    const pause = screen.getByRole("button", { name: "暂停目标" });
    fireEvent.click(pause);
    const dialog = screen.getByRole("alertdialog", { name: "暂停目标 确认" });
    const cancel = screen.getByRole("button", { name: "取消" });
    expect(dialog).toBeInTheDocument();
    expect(cancel).toHaveFocus();
    fireEvent.keyDown(cancel, { key: "Tab", shiftKey: true });
    expect(screen.getByRole("button", { name: "确认暂停目标" })).toHaveFocus();
    fireEvent.keyDown(dialog, { key: "Escape" });
    expect(screen.queryByRole("alertdialog", { name: "暂停目标 确认" })).toBeNull();
    expect(pause).toHaveFocus();
    fireEvent.click(pause);
    fireEvent.click(screen.getByRole("button", { name: "确认暂停目标" }));
    expect(callbacks.onGoalAction).toHaveBeenCalledWith("thread-exact", "PAUSE");
    fireEvent.click(screen.getByRole("button", { name: "返回 Codex" }));
    expect(screen.getByRole("heading", { name: "发布计划" })).toBeVisible();
  });

  it("keeps the M19 state disclosure truthful when every official Goal control is available", () => {
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} selectedWorkstreamId="work-1" items={[{ id: "work-1", name: "可控工作", lifecycle: "ACTIVE" }]} draft={{ value: "" }} surface="GOAL" goal={{ threadId: "thread-exact", text: "可控 Goal", status: "ACTIVE", activeTurnId: "turn-exact", controllableActions: ["PAUSE", "STOP_TURN", "DELETE"], readAt: Date.UTC(2026, 8, 10) }} />);
    const disclosure = document.querySelector<HTMLButtonElement>(".v3-goal-mobile-unavailable-trigger");
    expect(disclosure).not.toBeNull();
    fireEvent.click(disclosure!);
    expect(screen.getByRole("status", { name: "控制不可用状态详情" })).toHaveTextContent("当前所有适用控制均已获得官方控制权");
    expect(screen.getByRole("status", { name: "控制不可用状态详情" })).not.toHaveTextContent("当前无法控制");
  });

  it("keeps D20 purge confirmation scoped to an exact trashed workstream", () => {
    const onPurgeTrashedWorkstream = vi.fn();
    function PurgeHarness() {
      const [surface, setSurface] = useState<WorkbenchSurface>("RECYCLE");
      return <UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} onPurgeTrashedWorkstream={onPurgeTrashedWorkstream} selectedWorkstreamId="work-1" items={[{ id: "work-1", name: "待清理", lifecycle: "TRASHED" }]} draft={{ value: "" }} lifecycle={{ canPurge: true,canArchive:false,canTrash:false,canRestore:false }} surface={surface} onSurfaceChange={setSurface} />;
    }
    render(<PurgeHarness />);
    fireEvent.click(screen.getByRole("button", { name: "永久清除选中项" }));
    expect(screen.getByRole("region", { name: "永久清除前的最后确认" })).toHaveTextContent("待清理");
    fireEvent.click(screen.getByRole("button", { name: "取消，保留在回收站" }));
    expect(screen.getByRole("region", { name: "归档与回收站" })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "永久清除选中项" }));
    fireEvent.click(screen.getByRole("button", { name: "确认永久清除" }));
    expect(onPurgeTrashedWorkstream).toHaveBeenCalledWith("work-1");
  });

  it("uses a scoped responsive root and accessible status labels", () => {
    renderWorkbench();
    expect(screen.getByRole("main", { name: "AI Work Router 统一工作台" })).toHaveClass("unified-workbench");
    expect(screen.getByRole("navigation", { name: "工作区" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "当前工作区阅读" })).toHaveTextContent("发布计划");
    expect(screen.getByRole("button", { name: "查看两端绑定与详情" })).toBeVisible();
    expect(screen.getByLabelText("完整回复")).toBeInTheDocument();
    expect(screen.getByRole("complementary", { name: "当前交接上下文" })).toHaveTextContent("不会自动转发");
    expect(screen.queryByRole("button", { name: "移至回收站" })).toBeNull();
  });

  it("opens Codex results and attachments inside the shared main shell", () => {
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} items={[]} draft={{ value: "" }} reply={{ id: "reply-1", workstreamId: "work-1", provider: "CHATGPT", observedAt: Date.UTC(2026, 8, 10), readState: "READ", text: "已读内容", completeness: "COMPLETE" }} codexResultCount={1} codexResultsPanel={<section aria-label="Codex 结果与附件">完整结果与附件选择</section>} />);
    fireEvent.click(screen.getByRole("button", { name: "Codex 结果与附件 1" }));
    expect(screen.getByRole("region", { name: "Codex 结果与附件" })).toHaveTextContent("完整结果与附件选择");
    expect(screen.getByRole("main", { name: "AI Work Router 统一工作台" })).toContainElement(screen.getByRole("region", { name: "Codex 结果与附件" }));
  });

  it("uses one surface transition for New Work, Goal, Runtime, Project, Codex results, and the workspace return", () => {
    function NavigationHarness() {
  const [surface, setSurface] = useState<WorkbenchSurface>("WORKSPACE");
      return <UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} items={[]} draft={{ value: "" }} reply={{ id: "reply-1", workstreamId: "work-1", provider: "CHATGPT", observedAt: 1, readState: "READ", text: "工作区阅读器", completeness: "COMPLETE" }} goal={{ threadId: "thread-1", status: "ACTIVE", controllableActions: [], readAt: 1 }} runtime={{ checks: [] }} codexResultCount={1} codexResultsPanel={<section aria-label="Codex 结果与附件">Codex 结果</section>} newWorkPanel={<section aria-label="新建工作">项目续接或无项目对话</section>} projectHomePanel={<section aria-label="项目"><button type="button" onClick={() => setSurface("WORKSPACE")}>返回工作区</button></section>} projectPanel={<section aria-label="项目与对话连接"><button type="button" onClick={() => setSurface("WORKSPACE")}>返回工作区</button></section>} surface={surface} onSurfaceChange={setSurface} />;
    }
    render(<NavigationHarness />);
    fireEvent.click(screen.getByRole("button", { name: "运行环境" }));
    fireEvent.click(screen.getByRole("button", { name: "关闭全部前的影响确认" }));
    fireEvent.click(screen.getByRole("button", { name: "查看当前执行，再退出" }));
    expect(screen.getByRole("region", { name: "Goal 控制" })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "工作区" }));
    expect(screen.getByText("工作区阅读器")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "＋ 新建工作" }));
    expect(screen.getByRole("region", { name: "新建工作" })).toHaveTextContent("项目续接或无项目对话");
    fireEvent.click(screen.getByRole("button", { name: "工作区" }));
    fireEvent.click(screen.getByRole("button", { name: "收件箱" }));
    fireEvent.click(screen.getByRole("button", { name: "＋ 新建工作" }));
    expect(screen.getByRole("region", { name: "新建工作" })).toHaveTextContent("项目续接或无项目对话");
    fireEvent.click(screen.getByRole("button", { name: "工作区" }));
    expect(screen.getByText("工作区阅读器")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "运行环境" }));
    expect(screen.getByRole("region", { name: "工作区状态与运行环境" })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "关闭全部前的影响确认" }));
    expect(screen.getByRole("region", { name: "退出全部前确认影响" })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "查看当前执行，再退出" }));
    expect(screen.getByRole("region", { name: "Goal 控制" })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "工作区" }));
    fireEvent.click(screen.getByRole("button", { name: "运行环境" }));
    fireEvent.click(screen.getByRole("button", { name: "查看安装与更新" }));
    expect(screen.getByRole("region", { name: "安装与更新" })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "已有安装？查看当前运行环境" }));
    expect(screen.getByRole("region", { name: "工作区状态与运行环境" })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "＋ 新建工作" }));
    expect(screen.getByRole("region", { name: "新建工作" })).toHaveTextContent("项目续接或无项目对话");
    fireEvent.click(screen.getByRole("button", { name: "工作区" }));
    expect(screen.getByText("工作区阅读器")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "项目" }));
    fireEvent.click(screen.getByRole("button", { name: "返回工作区" }));
    expect(screen.getByText("工作区阅读器")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Codex 结果与附件 1" }));
    expect(screen.getByText("Codex 结果")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "工作区" }));
    expect(screen.getByText("工作区阅读器")).toBeVisible();
  });

  it("uses the host-provided hide-to-tray command without exposing a quit action", () => {
    const onHideToTray = vi.fn();
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} items={[]} draft={{ value: "" }} surface="EXIT_IMPACT" onHideToTray={onHideToTray} />);
    fireEvent.click(screen.getByRole("button", { name: "留在托盘运行" }));
    expect(onHideToTray).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("button", { name: /退出程序/ })).toBeNull();
  });

  it("keeps delivered handoff and execution state separate in the D16 focus", () => {
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} items={[]} draft={{ value: "" }} surface="HANDOFF_STATUS" handoff={{ id: "handoff-sent", status: "SENT", direction: "CHATGPT_TO_CODEX", message: "已批准的精确内容", canApprove: false, canSend: false, runStatus: "Codex Turn 已接受；执行完成状态独立刷新。" }} />);
    const focus = screen.getByRole("region", { name: "交付与执行状态" });
    expect(focus).toHaveTextContent("已送达 Codex");
    expect(focus).toHaveTextContent("正在执行");
    expect(focus).toHaveTextContent("不会把“已发送”显示成“已完成”");
    fireEvent.click(screen.getByRole("button", { name: "查看已发送的完整内容" }));
    expect(screen.getByRole("region", { name: "已发送内容" })).toHaveTextContent("已批准的精确内容");
  });

  it("keeps a 100-workstream index searchable without multiplying reply panes", () => {
    const callbacks = { onSelectWorkstream: vi.fn(), onDraftChange: vi.fn() };
    const items = Array.from({ length: 100 }, (_, index) => ({
      id: `work-${index}`,
      name: index === 77 ? "同名项目的目标工作" : `长名称工作区 ${index} ${"x".repeat(24)}`,
      projectName: index % 2 ? "项目 A" : "项目 B",
      lifecycle: "ACTIVE" as const,
      updatedAt: Date.UTC(2026, 8, 10) - index,
    }));
    render(<UnifiedWorkbench {...callbacks} items={items} selectedWorkstreamId="work-0" draft={{ value: "" }} />);
    fireEvent.change(screen.getByRole("textbox", { name: "搜索工作区" }), { target: { value: "目标工作" } });
    expect(screen.getByRole("button", { name: /同名项目的目标工作/ })).toBeInTheDocument();
    expect(screen.queryAllByLabelText("完整回复")).toHaveLength(1);
  });

  it("projects each concrete attention record into the inbox with its next action", () => {
    const onSelectWorkstream = vi.fn();
    const onSelectAttention = vi.fn();
    render(<UnifiedWorkbench onSelectWorkstream={onSelectWorkstream} onSelectAttention={onSelectAttention} onDraftChange={vi.fn()} surface="INBOX" items={[{ id: "attention-1", name: "待审阅工作", projectName: "Router", lifecycle: "ACTIVE", attentionCount: 2, attentionItems: [{ sourceId: "reply-exact", kind: "CHATGPT_REPLY_OBSERVED", priority: 3, message: "ChatGPT 有一条精确新回复", activityAt: 1 }, { sourceId: "handoff-exact", kind: "DELIVERY_UNCERTAIN", priority: 1, message: "这次交付尚未确认", activityAt: 2 }], statusLabel: "ChatGPT 有新回复" }, { id: "quiet-1", name: "普通工作", lifecycle: "ACTIVE", attentionCount: 0 }]} selectedWorkstreamId="attention-1" draft={{ value: "" }} />);
    const inbox = screen.getByRole("region", { name: "收件箱" });
    expect(inbox).toHaveTextContent("2 条具体事项等待你处理");
    const attentionList = within(inbox).getByRole("region", { name: "待处理列表" });
    expect(within(attentionList).getByRole("button", { name: /确认交付状态.*待审阅工作/ })).toBeVisible();
    const replyAttention = within(attentionList).getByRole("button", { name: /查看 ChatGPT 新回复.*待审阅工作/ });
    const deliveryAttention = within(attentionList).getByRole("button", { name: /确认交付状态.*待审阅工作/ });
    expect(replyAttention).toHaveTextContent("下一步：查看 ChatGPT 新回复");
    expect(deliveryAttention).toHaveTextContent("下一步：确认交付状态");
    expect(replyAttention).toHaveTextContent("精确来源：ChatGPT 精确外部回复 · reply-exact");
    expect(deliveryAttention).toHaveTextContent("精确来源：待确认的精确交接 · handoff-exact");
    expect(replyAttention).not.toHaveAttribute("aria-current");
    expect(deliveryAttention).not.toHaveAttribute("aria-current");
    fireEvent.click(replyAttention);
    expect(onSelectAttention).toHaveBeenCalledWith("attention-1", expect.objectContaining({ sourceId: "reply-exact", kind: "CHATGPT_REPLY_OBSERVED" }));
    expect(replyAttention).toHaveAttribute("aria-current", "true");
    expect(deliveryAttention).not.toHaveAttribute("aria-current");
    expect(within(attentionList).queryByRole("button", { name: /普通工作/ })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "＋ 新建工作" }));
    expect(onSelectWorkstream).not.toHaveBeenCalled();
  });

  it("does not turn a source-less aggregate count into a clickable attention record", () => {
    const onSurfaceChange = vi.fn();
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onSelectAttention={vi.fn()} onDraftChange={vi.fn()} onSurfaceChange={onSurfaceChange} surface="INBOX" items={[{ id: "legacy-count", name: "旧状态", lifecycle: "ACTIVE", attentionCount: 2, attentionItems: [] }]} selectedWorkstreamId="legacy-count" draft={{ value: "" }} />);
    const inbox = screen.getByRole("region", { name: "收件箱" });
    expect(inbox).toHaveTextContent("Router 报告有待处理状态，但尚未提供可安全打开的精确来源。");
    expect(within(screen.getByRole("region", { name: "待处理列表" })).queryByRole("button", { name: /查看此 Router 事项/ })).toBeNull();
    expect(screen.getByRole("alert")).toHaveTextContent("Router 还有 2 条状态未附精确来源。为避免打开错误的对话或结果，Router 没有把它们显示为可操作事项。");
    fireEvent.click(screen.getByRole("button", { name: "查看运行环境并重新读取" }));
    expect(onSurfaceChange).toHaveBeenCalledWith("RUNTIME");
  });

  it("gives every provider-run attention state an exact source and a concrete next action", () => {
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onSelectAttention={vi.fn()} onDraftChange={vi.fn()} surface="INBOX" items={[{ id: "run-attention", name: "执行状态", lifecycle: "ACTIVE", attentionCount: 3, attentionItems: [{ sourceId: "run-failed", kind: "PROVIDER_RUN_FAILED", priority: 2 }, { sourceId: "run-cancelled", kind: "PROVIDER_RUN_CANCELLED", priority: 2 }, { sourceId: "run-unknown", kind: "UNKNOWN_RUN", priority: 3 }] }]} selectedWorkstreamId="run-attention" draft={{ value: "" }} />);
    const attentionList = screen.getByRole("region", { name: "待处理列表" });
    expect(within(attentionList).getByRole("button", { name: /查看失败执行状态.*执行状态/ })).toHaveTextContent("精确来源：失败的精确 Provider 执行 · run-failed");
    expect(within(attentionList).getByRole("button", { name: /查看已取消执行状态.*执行状态/ })).toHaveTextContent("精确来源：已取消的精确 Provider 执行 · run-cancelled");
    expect(within(attentionList).getByRole("button", { name: /核对未知执行状态.*执行状态/ })).toHaveTextContent("精确来源：状态未知的精确 Provider 执行 · run-unknown");
  });

  it("renders every Core attention kind, plus the mobile structured request, with an exact source and next action", () => {
    const coverage: Array<[string, string, string]> = [
      ["DELIVERY_UNCERTAIN", "确认交付状态", "待确认的精确交接"],
      ["HANDOFF_FAILED", "检查交付失败", "失败的精确交接"],
      ["CHATGPT_REPLY_OBSERVED", "查看 ChatGPT 新回复", "ChatGPT 精确外部回复"],
      ["CODEX_REPLY_OBSERVED", "查看 Codex 新回复", "Codex 精确外部回复"],
      ["CHATGPT_RESULT_READY", "审阅 ChatGPT 结果", "ChatGPT 保留执行结果"],
      ["CODEX_RESULT_READY", "审阅 Codex 结果", "Codex 保留执行结果"],
      ["PROVIDER_RUN_FAILED", "查看失败执行状态", "失败的精确 Provider 执行"],
      ["PROVIDER_RUN_CANCELLED", "查看已取消执行状态", "已取消的精确 Provider 执行"],
      ["UNKNOWN_RUN", "核对未知执行状态", "状态未知的精确 Provider 执行"],
      ["MISSING_CHATGPT_BINDING", "补充 ChatGPT 绑定", "当前工作区的 ChatGPT 绑定"],
      ["MISSING_CODEX_BINDING", "补充 Codex 绑定", "当前工作区的 Codex 绑定"],
      ["CODEX_STRUCTURED_REQUEST", "处理 Codex 请求", "Codex 精确结构化请求"],
    ];
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onSelectAttention={vi.fn()} onDraftChange={vi.fn()} surface="INBOX" selectedWorkstreamId="all-kinds" items={[{ id: "all-kinds", name: "类型覆盖", lifecycle: "ACTIVE", attentionCount: coverage.length, attentionItems: coverage.map(([kind], index) => ({ sourceId: `source-${index}`, kind, priority: index })) }]} draft={{ value: "" }} />);
    const attentionList = screen.getByRole("region", { name: "待处理列表" });
    coverage.forEach(([kind, action, source], index) => {
      const item = within(attentionList).getByRole("button", { name: new RegExp(`${action}.*类型覆盖`) });
      expect(item).toHaveTextContent(`下一步：${action}`);
      expect(item).toHaveTextContent(`精确来源：${source} · source-${index}`);
      expect(item).toHaveTextContent("Router 尚未提供这条待办的说明。");
    });
  });

  it("does not offer a misleading ChatGPT reply action while an exact Codex reply is open in the inbox", () => {
    const onSurfaceChange = vi.fn();
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} onSurfaceChange={onSurfaceChange} surface="INBOX" selectedWorkstreamId="work-1" items={[{ id: "work-1", name: "Codex 待审阅", lifecycle: "ACTIVE", attentionItems: [{ sourceId: "codex-reply-1", kind: "CODEX_REPLY_OBSERVED", priority: 1 }] }]} draft={{ value: "" }} reply={{ id: "codex-reply-1", workstreamId: "work-1", provider: "CODEX", observedAt: 1, readState: "UNREAD", text: "当前精确 Codex 回复", completeness: "COMPLETE" }} />);
    expect(screen.queryByRole("button", { name: "回复 ChatGPT" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "前往 Codex 审阅" }));
    expect(onSurfaceChange).toHaveBeenCalledWith("WORKSPACE");
  });

  it("states the owner-controlled review, approval, and separate send sequence in the handoff surfaces", () => {
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} onAddSelectionToHandoff={vi.fn()} selectedWorkstreamId="work-1" items={[{ id: "work-1", name: "范围与批准", lifecycle: "ACTIVE" }]} draft={{ value: "" }} reply={{ id: "reply-1", workstreamId: "work-1", provider: "CHATGPT", observedAt: 1, readState: "READ", text: "可交接内容", completeness: "COMPLETE" }} />);
    fireEvent.click(screen.getByRole("button", { name: "选择范围" }));
    const selection = screen.getByRole("region", { name: "交接指令范围" });
    expect(selection).toHaveTextContent("步骤 1/3 · 确认范围");
    expect(selection).toHaveTextContent("批准此版本（不会发送） → 单独发送");
    cleanup();
    render(<UnifiedWorkbench onSelectWorkstream={vi.fn()} onDraftChange={vi.fn()} surface="HANDOFF_REVIEW" selectedWorkstreamId="work-1" items={[{ id: "work-1", name: "范围与批准", lifecycle: "ACTIVE" }]} draft={{ value: "" }} handoff={{ id: "handoff-1", status: "READY", direction: "CHATGPT_TO_CODEX", message: "待批准内容", canApprove: true, canSend: false }} />);
    const review = screen.getByRole("region", { name: "人工交接审阅" });
    expect(review).toHaveTextContent("步骤 2/3 · 审阅并批准");
    expect(review).toHaveTextContent("批准不会发送，下一步仍需单独确认发送");
    expect(within(review).getByRole("button", { name: "批准此版本（不会发送）" })).toBeVisible();
  });
});
