import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke, isTauri: () => true }));

import { codexApi } from "./api";

describe("Codex adapter lifecycle commands", () => {
  beforeEach(() => invoke.mockReset());

  it("reads only the exact requested thread identity", async () => {
    invoke.mockResolvedValue({ messages: [], protocolEvents: [] });
    await codexApi.readThreadHistory("thread-a");
    expect(invoke).toHaveBeenCalledWith("read_thread_history", { threadId: "thread-a" });
  });

  it("selects a Workstream without acquiring a writer", async () => {
    invoke.mockResolvedValue({});
    await codexApi.selectWorkspace("project-a", "workstream-a");
    expect(invoke.mock.calls).toEqual([["select_workspace", { projectId: "project-a", workstreamId: "workstream-a" }]]);
    expect(invoke).not.toHaveBeenCalledWith("resume_thread", expect.anything());
    expect(invoke).not.toHaveBeenCalledWith("send_turn", expect.anything());
  });

  it("keeps resume and explicit send as separate lazy operations", async () => {
    invoke.mockResolvedValue({});
    await codexApi.resumeThread("thread-a");
    await codexApi.sendTurn("thread-a", "Reply with exactly:\nV0_007_COMPOSER_OK");
    expect(invoke.mock.calls).toEqual([
      ["resume_thread", { threadId: "thread-a" }],
      ["send_turn", { threadId: "thread-a", text: "Reply with exactly:\nV0_007_COMPOSER_OK" }],
    ]);
  });

  it("uses separate explicit review and acknowledge mutations", async () => {
    invoke.mockResolvedValue(undefined);
    await codexApi.markProviderRunReviewed("run-a");
    await codexApi.acknowledgeHandoffAttention("handoff-a");
    expect(invoke.mock.calls).toEqual([
      ["mark_provider_run_reviewed", { runId: "run-a" }],
      ["acknowledge_handoff_attention", { handoffId: "handoff-a" }],
    ]);
  });

  it("keeps continuation candidate, verification, confirmation, and cancel as separate explicit calls", async () => {
    invoke.mockResolvedValue({});
    await codexApi.beginCodexRollover("workstream-a");
    await codexApi.initializeCodexRollover("workstream-a", "Visible initialization only");
    await codexApi.verifyCodexRollover("workstream-a");
    await codexApi.confirmRollover("workstream-a");
    await codexApi.cancelRollover("workstream-a");
    expect(invoke.mock.calls).toEqual([
      ["begin_codex_rollover", { workstreamId: "workstream-a" }],
      ["initialize_codex_rollover", { workstreamId: "workstream-a", text: "Visible initialization only" }],
      ["verify_codex_rollover", { workstreamId: "workstream-a" }],
      ["confirm_rollover", { workstreamId: "workstream-a" }],
      ["cancel_rollover", { workstreamId: "workstream-a" }],
    ]);
  });
});
