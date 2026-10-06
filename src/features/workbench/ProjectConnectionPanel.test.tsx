import type { ComponentProps } from "react";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ProjectConnectionPanel } from "./ProjectConnectionPanel";

afterEach(cleanup);

function panel(overrides: Partial<ComponentProps<typeof ProjectConnectionPanel>> = {}) {
  const onPair = vi.fn();
  render(<ProjectConnectionPanel
    projectName="V3.1" links={[]} choices={[]}
    codexThreadId="" codexThreadLabel=""
    onCodexThreadIdChange={vi.fn()} onCodexThreadLabelChange={vi.fn()}
    onPair={onPair}
    {...overrides}
  />);
  return onPair;
}

describe("ProjectConnectionPanel", () => {
  it("routes the simple project entry into the shared guide and keeps advanced connection available", () => {
    const manage=vi.fn(); const onPair=panel({guidedBindingEntry:true,onManageBindings:manage,workstreamName:"当前工作"});
    expect(screen.getByRole("heading",{name:"控制端"})).toBeVisible();
    fireEvent.click(screen.getByRole("button",{name:"选择控制端与执行端"}));
    expect(manage).toHaveBeenCalledOnce();expect(onPair).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button",{name:"项目链接与高级连接 ›"}));
    expect(screen.getByLabelText("具体 ChatGPT 对话链接")).toBeVisible();
    fireEvent.click(screen.getByRole("button",{name:"返回简洁绑定"}));
    expect(screen.getByRole("button",{name:"选择控制端与执行端"})).toBeVisible();
  });
  it("keeps an existing unsaved selection visible before opening another binding flow", () => {
    panel({guidedBindingEntry:true,onManageBindings:vi.fn(),codexThreadId:"qa-thread",codexThreadLabel:"已核对的选择",selectedExistingCodexThread:{id:"qa-thread",label:"已核对的选择"}});
    expect(screen.getByRole("button",{name:"选择控制端与执行端"})).toBeDisabled();
    fireEvent.click(screen.getByRole("button",{name:"继续核对现有选择"}));
    expect(screen.getByRole("heading",{name:"确认这一组对话"})).toBeVisible();
  });
  it("returns an incomplete ChatGPT-only candidate to selection before any pair save", () => {
    const onPair=panel({guidedBindingEntry:true,onManageBindings:vi.fn(),explicitChatGptCandidate:{workstreamId:"work-a",externalId:"chat-only",label:"未保存的对话",verification:"OWNER_CONFIRMED_EXACT_URL",expectedBindingRevision:3}});
    fireEvent.click(screen.getByRole("button",{name:"继续核对现有选择"}));
    expect(screen.queryByRole("button",{name:"保存并进入工作"})).not.toBeInTheDocument();
    expect(screen.getByText("chat-only")).toBeVisible();expect(onPair).not.toHaveBeenCalled();
  });
  it("blocks an incomplete pair entered directly through advanced review", () => {
    const onPair=panel({initialScreen:"PAIR_REVIEW"});
    expect(screen.getByRole("button",{name:"保存并进入工作"})).toBeDisabled();
    expect(onPair).not.toHaveBeenCalled();
  });
  it("uses an exact ChatGPT link as the only ChatGPT binding entry", () => {
    panel();
    expect(screen.getByLabelText("具体 ChatGPT 对话链接")).toBeVisible();
    expect(screen.getByText(/不会读取、控制或自动验证该页面/)).toBeVisible();
    expect(screen.queryByText("添加 ChatGPT 项目")).toBeNull();
    expect(screen.queryByText("读取项目内对话")).toBeNull();
  });

  it("uses owner-confirmed default-browser verification without exposing carrier setup", () => {
    const prepareOwnerConfirmed = vi.fn();
    panel({
      explicitChatGptUrl: "https://chatgpt.com/c/conversation-a",
      onPrepareOwnerConfirmedChatGptBinding: prepareOwnerConfirmed,
      onOpenHostChatGptSetup: vi.fn(),
    });
    fireEvent.click(screen.getByRole("button", { name: "我已在默认浏览器核对，准备绑定" }));
    expect(prepareOwnerConfirmed).toHaveBeenCalledOnce();
    expect(screen.queryByRole("button", { name: "验证具体对话" })).toBeNull();
    expect(screen.queryByRole("button", { name: "打开专用浏览器登录" })).toBeNull();
  });

  it("shows a completed current pair instead of a replacement form", () => {
    const back = vi.fn();
    panel({ workstreamName: "PRE-FLIGHT", activeChatGptLabel: "ChatGPT conversation", activeCodexLabel: "Complete Desktop Relay V0.1", onBack: back });
    expect(screen.getByRole("heading", { name: "当前对话已绑定" })).toBeVisible();
    expect(screen.getByText("Complete Desktop Relay V0.1")).toBeVisible();
    expect(screen.queryByLabelText("具体 ChatGPT 对话链接")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "返回工作区" }));
    expect(back).toHaveBeenCalledOnce();
    fireEvent.click(screen.getByRole("button", { name: "更换这组对话" }));
    expect(screen.getByLabelText("具体 ChatGPT 对话链接")).toBeVisible();
  });

  it("offers a current-link repair without disguising it as a conversation replacement", () => {
    const prepare = vi.fn();
    panel({
      workstreamName: "游戏开发",
      activeChatGptLabel: "游戏开发管理对话",
      activeChatGptId: "6ab7e39e-d0d0-83ec-87da-31d699cedfd3",
      activeCodexLabel: "游戏开发 Codex",
      activeCodexThreadId: "00000000-0000-7000-8000-897e16603e6e",
      explicitChatGptUrl: "https://chatgpt.com/g/g-game/c/6ab7e39e-d0d0-83ec-87da-31d699cedfd3",
      onPrepareOwnerConfirmedChatGptBinding: prepare,
    });
    fireEvent.click(screen.getByRole("button", { name: "补充已核对的 ChatGPT 链接" }));
    expect(screen.getByRole("heading", { name: "补充当前 ChatGPT 精确链接" })).toBeVisible();
    expect(screen.getByText(/不会更换 ChatGPT 或 Codex 对话/)).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "我已在默认浏览器核对，准备保存当前链接" }));
    expect(prepare).toHaveBeenCalledOnce();
  });

  it("rejects a different conversation from the current-link repair path", () => {
    const prepare = vi.fn();
    panel({
      activeChatGptLabel: "当前 ChatGPT 对话",
      activeChatGptId: "current-conversation",
      activeCodexLabel: "当前 Codex 对话",
      explicitChatGptUrl: "https://chatgpt.com/c/different-conversation",
      onPrepareOwnerConfirmedChatGptBinding: prepare,
    });
    fireEvent.click(screen.getByRole("button", { name: "补充已核对的 ChatGPT 链接" }));
    fireEvent.click(screen.getByRole("button", { name: "我已在默认浏览器核对，准备保存当前链接" }));
    expect(screen.getByRole("alert")).toHaveTextContent("这不是当前已绑定的 ChatGPT 对话");
    expect(prepare).not.toHaveBeenCalled();
  });

  it("opens the exact ChatGPT replacement form directly without exposing Codex changes", () => {
    panel({
      initialScreen: "REPLACE_CHATGPT",
      workstreamName: "PRE-FLIGHT",
      activeChatGptLabel: "旧 ChatGPT 对话",
      activeCodexLabel: "Complete Desktop Relay V0.1",
      activeCodexThreadId: "00000000-0000-7000-8000-b196d3c6853c",
    });
    expect(screen.getByRole("heading", { name: "更换当前 ChatGPT 对话" })).toBeVisible();
    expect(screen.getByText("当前工作：PRE-FLIGHT")).toBeVisible();
    expect(screen.getByText(/项目关联和已有 Codex 对话均不会改动/)).toBeVisible();
    expect(screen.getByLabelText("具体 ChatGPT 对话链接")).toBeVisible();
    expect(screen.queryByRole("button", { name: "绑定已有 Codex 对话" })).toBeNull();
    expect(screen.queryByRole("button", { name: "更换这组对话" })).toBeNull();
  });

  it("requires explicit confirmation before replacing the active ChatGPT endpoint", () => {
    const confirm = vi.fn();
    panel({
      explicitChatGptCandidate: { workstreamId: "work-a", externalId: "conversation-a", label: "已验证 ChatGPT 对话", verification: "PLAYWRIGHT_EXACT_ROUTE", expectedOldEndpointId: "endpoint-old", expectedBindingRevision: 3 },
      explicitChatGptConfirmationState: "IDLE",
      onConfirmExplicitChatGptBinding: confirm,
    });
    expect(screen.getByText("确认后会替换当前 ACTIVE ChatGPT 对话，并保留历史关联。")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "确认绑定" }));
    expect(confirm).toHaveBeenCalledOnce();
  });

  it("labels an owner-confirmed URL honestly and exposes its exact conversation ID", () => {
    panel({
      explicitChatGptCandidate: { workstreamId: "work-a", externalId: "6ab86566-a260-83ec-a603-a930845e22c6", label: "用户已在默认浏览器核对的 ChatGPT 对话", verification: "OWNER_CONFIRMED_EXACT_URL", expectedOldEndpointId: "endpoint-old", expectedBindingRevision: 3 },
    });
    expect(screen.getByText("你已人工核对的具体 ChatGPT 对话")).toBeVisible();
    expect(screen.getByText("6ab86566-a260-83ec-a603-a930845e22c6")).toBeVisible();
    expect(screen.getByText(/Router 没有读取或控制该页面/)).toBeVisible();
    expect(screen.getByRole("button", { name: "确认绑定" })).toBeVisible();
  });

  it("renders confirmation success and failure inside the exact ChatGPT candidate card", () => {
    const candidate = { workstreamId: "work-a", externalId: "conversation-a", label: "已验证 ChatGPT 对话", verification: "PLAYWRIGHT_EXACT_ROUTE" as const, expectedOldEndpointId: null, expectedBindingRevision: 3 };
    const { rerender } = render(<ProjectConnectionPanel
      projectName="V3.1" links={[]} choices={[]} codexThreadId="" codexThreadLabel=""
      onCodexThreadIdChange={vi.fn()} onCodexThreadLabelChange={vi.fn()} onPair={vi.fn()}
      explicitChatGptCandidate={candidate} explicitChatGptConfirmationState="SUCCEEDED" explicitChatGptConfirmationMessage="ChatGPT 对话已绑定。正在刷新当前工作区。"
    />);
    expect(screen.getByRole("status")).toHaveTextContent("ChatGPT 对话已绑定");
    expect(screen.getByRole("button", { name: "ChatGPT 对话已绑定" })).toBeDisabled();
    rerender(<ProjectConnectionPanel
      projectName="V3.1" links={[]} choices={[]} codexThreadId="" codexThreadLabel=""
      onCodexThreadIdChange={vi.fn()} onCodexThreadLabelChange={vi.fn()} onPair={vi.fn()}
      explicitChatGptCandidate={candidate} explicitChatGptConfirmationState="FAILED" explicitChatGptConfirmationMessage="当前绑定没有改变。"
    />);
    expect(screen.getByRole("alert")).toHaveTextContent("当前绑定没有改变");
  });

  it("keeps Codex candidates selectable without a ChatGPT directory", () => {
    const select = vi.fn();
    panel({ choices: [{ id: "codex-candidate", provider: "CODEX", title: "可验证候选", detail: "标题 · 日期 · 来源", onSelect: select }] });
    fireEvent.click(screen.getByRole("button", { name: /可验证候选/ }));
    expect(select).toHaveBeenCalledOnce();
  });

  it("keeps the no-project Codex route available", () => {
    panel();
    fireEvent.click(screen.getByRole("button", { name: "＋ 新建无项目 Codex 对话" }));
    expect(screen.getByRole("heading", { name: "无项目 Codex 对话" })).toBeVisible();
  });

  it("binds an existing catalog thread without any Codex Project directory", async () => {
    const load = vi.fn().mockResolvedValue(undefined);
    const verify = vi.fn().mockResolvedValue(undefined);
    panel({
      existingCodexThreads: [
        { id: "duplicate-a", label: "重复标题", preview: "第一个", updatedAt: "2026-09-26", projectProvenance: null },
        { id: "duplicate-b", label: "重复标题", preview: "第二个", updatedAt: "2026-09-25", projectProvenance: null },
      ],
      onLoadExistingCodexThreads: load,
      onSelectExistingCodexThread: verify,
    });
    fireEvent.click(screen.getByRole("button", { name: "绑定已有 Codex 对话" }));
    expect(await screen.findByRole("heading", { name: "绑定已有 Codex 对话" })).toBeVisible();
    expect(load).toHaveBeenCalledOnce();
    fireEvent.change(screen.getByLabelText("绑定已有 Codex 对话时间范围"), {target:{value:"ALL"}});
    expect(screen.getByRole("heading", {name:/项目归属未确认/})).toBeVisible();
    expect(screen.queryByText("项目目录当前不可用")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /重复标题.*第一个/ }));
    await waitFor(() => expect(verify).toHaveBeenCalledWith("duplicate-a"));
    expect(await screen.findByRole("heading", { name: "确认这一组对话" })).toBeVisible();
    expect(screen.queryByText("duplicate-a")).toBeNull();
    expect(screen.queryByText("duplicate-b")).toBeNull();
  });

  it("requires a Codex exact ID and label before the independent pair review", () => {
    const onPair = panel({ codexThreadId: "codex-exact", codexThreadLabel: "Codex 标题 · 09-10 · 已核验" });
    fireEvent.click(screen.getByRole("button", { name: "核对这组配对" }));
    expect(screen.getByRole("heading", { name: "确认这一组对话" })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "保存并进入工作" }));
    expect(onPair).toHaveBeenCalledOnce();
  });
});
