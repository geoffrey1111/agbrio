import { describe, expect, it } from "vitest";
import { shouldRefreshDashboardForChatGptEvent, shouldRefreshDashboardForCodexEvent } from "./lifecycle";

describe("projection lifecycle refresh", () => {
  it("refreshes a selected Codex projection at turn boundaries, never on agent deltas", () => {
    expect(shouldRefreshDashboardForCodexEvent({ id: "start", kind: "Progress", method: "turn/started", threadId: "thread-a" })).toBe(true);
    expect(shouldRefreshDashboardForCodexEvent({ id: "end", kind: "Completion", method: "turn/completed", threadId: "thread-a" })).toBe(true);
    expect(shouldRefreshDashboardForCodexEvent({ id: "delta", kind: "AgentMessage", method: "item/agentMessage/delta", threadId: "thread-a", text: "partial" })).toBe(false);
  });

  it("refreshes ChatGPT state only for a terminal provider event", () => {
    expect(shouldRefreshDashboardForChatGptEvent({ eventType: "answer.delta", text: "partial", completed: false })).toBe(false);
    expect(shouldRefreshDashboardForChatGptEvent({ eventType: "request.result", completed: true })).toBe(true);
  });
});
