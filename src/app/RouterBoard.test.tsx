import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { RouterBoard } from "./RouterBoard";
import type { DashboardProjection } from "../features/codex/types";

const dashboard: DashboardProjection = {
  attentionItems: [],
  workstreams: [
    {
      projectId: "project-a",
      projectName: "Northstar",
      lastActivityAt: 1,
      workstream: {
        id: "ws-a",
        projectId: "project-a",
        name: "报价校验",bindingRevision:0,
        status: "ACTIVE",
        createdAt: 1,
        updatedAt: 1,
      },
      chatgptEndpoint: {
        id: "chat-a",
        workstreamId: "ws-a",
        provider: "CHATGPT",
        externalId: "conversation-a",
        label: "报价讨论",
        status: "ACTIVE",
        createdAt: 1,
      },
      codexEndpoint: {
        id: "codex-a",
        workstreamId: "ws-a",
        provider: "CODEX",
        externalId: "thread-a",
        label: "实现线程",
        status: "ACTIVE",
        createdAt: 1,
      },
      codexRun: {
        id: "run-a",
        workstreamId: "ws-a",
        endpointId: "codex-a",
        provider: "CODEX",
        originHandoffId: "handoff-a",
        status: "STARTING",
        updatedAt: 1,
      },
      attentionItems: [],
    },
  ],
};

describe("Router Board", () => {
  it("makes the provider relationship and sent-but-not-complete state explicit", () => {
    const onOpen = vi.fn();
    const { container } = render(
      <RouterBoard
        dashboard={dashboard}
        codex={{ connected: true }}
        chatGpt={{ connected: false, clients: 0, needsSelection: false }}
        onOpen={onOpen}
      />,
    );
    expect(screen.getByText("下一步是谁？")).toBeInTheDocument();
    expect(screen.getByText("ChatGPT · 未连接")).toBeInTheDocument();
    expect(screen.getAllByText("交接已发送 · Codex 执行中")).not.toHaveLength(
      0,
    );
    expect(screen.getAllByText("报价讨论")).not.toHaveLength(0);
    expect(screen.getAllByText("实现线程")).not.toHaveLength(0);
    fireEvent.click(
      screen.getAllByRole("button", { name: /打开会话/ })[0],
    );
    expect(onOpen).toHaveBeenCalledWith(dashboard.workstreams[0]);
  });

  it("names Codex as the next owner after a completed ChatGPT phase", () => {
    const awaitingCodex: DashboardProjection = {
      ...dashboard,
      workstreams: [
        {
          ...dashboard.workstreams[0],
          codexRun: null,
          chatgptRun: {
            id: "chat-run-a",
            workstreamId: "ws-a",
            endpointId: "chat-a",
            provider: "CHATGPT",
            status: "COMPLETED",
            updatedAt: 2,
          },
        },
      ],
    };
    render(
      <RouterBoard
        dashboard={awaitingCodex}
        codex={{ connected: true }}
        chatGpt={{ connected: true, clients: 1, needsSelection: false }}
        onOpen={() => {}}
      />,
    );
    expect(screen.getAllByText("等待 Codex")).not.toHaveLength(0);
  });

  it("keeps visual-only Board lanes read-only", () => {
    const onOpen = vi.fn();
    const { container } = render(
      <RouterBoard
        dashboard={dashboard}
        codex={{ connected: true }}
        chatGpt={{ connected: true, clients: 1, needsSelection: false }}
        onOpen={onOpen}
        readOnly
      />,
    );
    const open = container.querySelector<HTMLButtonElement>(".lane-open");
    expect(open).not.toBeNull();
    expect(open).toBeDisabled();
    fireEvent.click(open!);
    expect(onOpen).not.toHaveBeenCalled();
  });
});
