import type { AttentionItem } from "../features/codex/types";

export type AttentionTarget = "provider-run" | "handoff" | "bindings";

export function attentionTarget(kind: AttentionItem["kind"]): AttentionTarget {
  if (kind === "MISSING_CHATGPT_BINDING" || kind === "MISSING_CODEX_BINDING") return "bindings";
  if (kind === "DELIVERY_UNCERTAIN" || kind === "HANDOFF_FAILED") return "handoff";
  return "provider-run";
}

export function isReviewableProviderRun(kind: AttentionItem["kind"]): boolean {
  return kind === "CODEX_RESULT_READY" || kind === "CHATGPT_RESULT_READY" || kind === "PROVIDER_RUN_FAILED" || kind === "PROVIDER_RUN_CANCELLED" || kind === "UNKNOWN_RUN";
}
