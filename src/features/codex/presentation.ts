import type { FeedEvent } from "./types";

export type ConversationRole = "user" | "assistant";

export interface ConversationMessage {
  id: string;
  role: ConversationRole;
  text: string;
  streaming?: boolean;
  relaySource?: "ChatGPT";
}

export type ExecutionDetailKind = "Reasoning" | "Progress" | "Command" | "FileChange" | "Approval" | "Warning";

export interface ExecutionDetail {
  id: string;
  kind: ExecutionDetailKind;
  text: string;
}

export interface ProtocolDebugEvent {
  id: string;
  method: string;
  kind: FeedEvent["kind"];
  detail: string;
  raw?: string;
}

export interface PresentationUpdate {
  conversation?: ConversationMessage;
  execution?: ExecutionDetail;
  debug?: ProtocolDebugEvent;
}

const protocolOnlyMethods = new Set([
  "remoteControl/status/changed",
  "mcpServer/startupStatus/updated",
  "thread/tokenUsage/updated",
  "account/rateLimits/updated",
  "thread/status/changed",
  "turn/started",
  "turn/completed",
]);

function debug(event: FeedEvent): ProtocolDebugEvent {
  return {
    id: event.id,
    method: event.method,
    kind: event.kind,
    detail: event.detail ?? event.text ?? "No displayable detail",
    raw: event.raw,
  };
}

function executionKind(event: FeedEvent): ExecutionDetailKind | undefined {
  if (event.kind === "Command" || event.kind === "FileChange" || event.kind === "Approval" || event.kind === "Warning") return event.kind;
  if (event.kind === "Progress") return event.method.includes("reasoning") ? "Reasoning" : "Progress";
  return undefined;
}

/**
 * The app-server protocol remains intact at this boundary. This function owns
 * the conservative, UI-only mapping from raw feed events to presentation data.
 */
export function normalizeFeedEvent(event: FeedEvent): PresentationUpdate {
  if (event.kind === "UserMessage" && event.method === "turn/start" && event.text) {
    return { conversation: { id: event.id, role: "user", text: event.text } };
  }

  if (event.kind === "UserMessage" && event.method === "relay/chatgpt-to-codex" && event.text) {
    return { conversation: { id: event.id, role: "user", text: event.text, relaySource: "ChatGPT" } };
  }

  if (event.kind === "UserMessage" && event.method === "thread/read" && event.text) {
    return { conversation: { id: event.itemId ?? event.id, role: "user", text: event.text } };
  }

  if (event.kind === "AgentMessage" && event.method === "item/agentMessage/delta" && event.itemId) {
    return { conversation: { id: event.itemId, role: "assistant", text: event.text ?? "", streaming: true } };
  }

  if (event.kind === "AgentMessage" && (event.method === "item/completed" || event.method === "thread/read") && event.text) {
    return { conversation: { id: event.itemId ?? event.id, role: "assistant", text: event.text } };
  }

  if (protocolOnlyMethods.has(event.method)) return { debug: debug(event) };

  const kind = executionKind(event);
  if (kind) {
    return {
      execution: {
        id: event.id,
        kind,
        text: event.detail ?? event.text ?? event.method,
      },
      debug: debug(event),
    };
  }

  // Lifecycle, account, rate-limit, MCP, and unrecognized protocol traffic is
  // intentionally inspectable but never part of the human conversation.
  return { debug: debug(event) };
}

export function upsertConversation(current: ConversationMessage[], incoming: ConversationMessage): ConversationMessage[] {
  const index = current.findIndex((message) => message.id === incoming.id && message.role === incoming.role);
  if (index < 0) return [...current, incoming];

  const next = [...current];
  const existing = next[index];
  next[index] = incoming.streaming
    ? { ...existing, text: `${existing.text}${incoming.text}`, streaming: true }
    : { ...incoming, streaming: false };
  return next;
}

export function appendExecution(current: ExecutionDetail[], incoming: ExecutionDetail): ExecutionDetail[] {
  return current.some((detail) => detail.id === incoming.id) ? current : [...current, incoming];
}

export function appendDebug(current: ProtocolDebugEvent[], incoming: ProtocolDebugEvent): ProtocolDebugEvent[] {
  return current.some((event) => event.id === incoming.id) ? current : [...current, incoming];
}
