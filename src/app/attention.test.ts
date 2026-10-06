import { describe, expect, it } from "vitest";
import { attentionTarget, isReviewableProviderRun } from "./attention";

describe("attention routing", () => {
  it("targets the exact depth context without performing a state action", () => {
    expect(attentionTarget("CODEX_RESULT_READY")).toBe("provider-run");
    expect(attentionTarget("DELIVERY_UNCERTAIN")).toBe("handoff");
    expect(attentionTarget("MISSING_CHATGPT_BINDING")).toBe("bindings");
    expect(attentionTarget("MISSING_CODEX_BINDING")).toBe("bindings");
  });

  it("keeps review semantics limited to ProviderRun attention", () => {
    expect(isReviewableProviderRun("CODEX_RESULT_READY")).toBe(true);
    expect(isReviewableProviderRun("UNKNOWN_RUN")).toBe(true);
    expect(isReviewableProviderRun("HANDOFF_FAILED")).toBe(false);
    expect(isReviewableProviderRun("MISSING_CODEX_BINDING")).toBe(false);
  });
});
