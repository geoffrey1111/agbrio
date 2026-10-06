import type { ChatGptStreamEvent, FeedEvent } from "../features/codex/types";

/** Dashboard data changes only at execution lifecycle boundaries, not feed deltas. */
export function shouldRefreshDashboardForCodexEvent(event: FeedEvent): boolean {
  return event.method === "turn/started" || event.method === "turn/completed";
}

/** ChatGPT deltas are transcript presentation only; terminal events change the projection. */
export function shouldRefreshDashboardForChatGptEvent(event: ChatGptStreamEvent): boolean {
  return event.completed || event.eventType === "request.result";
}
