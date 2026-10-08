import { invoke } from "@tauri-apps/api/core";
import { request } from "../../mobile/api";
import type { CodexWatch } from "./CodexNotifications";
import type { MobileCodexGoal, MobileCodexRequest } from "../../mobile/api";

export type ReplyOptions = { model?: string | null; effort?: string | null; attachments: string[] };
export type WatchReply = { id: string; threadId: string; sourceSequence: number | null; expectedTurnId: string | null; mode: "SEND" | "QUEUE" | "STEER"; text: string; status: "QUEUED" | "SENDING" | "SENT" | "UNKNOWN" | "ACKNOWLEDGED" | "CANCELLED" | "FAILED"; turnId: string | null; errorCode: string | null; createdAt: number; options: ReplyOptions };
export type WatchChatState = { publicMessages?:ChatMessage[]; activity?:"THINKING"|"EXECUTING"|null; checkedAt?:number; watch: CodexWatch; host: string; ownedTurnId: string | null; controllableTurnId?:string|null; externalBusy: boolean; replies: WatchReply[]; requests: MobileCodexRequest[]; goal: MobileCodexGoal | null };
export type ChatCommand = { action: string; threadId: string; [key: string]: unknown };
export type ChatMessage = { id: string; turnId: string; role: "user" | "assistant"; text: string };
export type ChatHistory = { messages: ChatMessage[]; nextCursor: string | null };
export type ChatModel = { id: string; model: string; displayName: string; supportedReasoningEfforts: { reasoningEffort: string; description: string }[] };
export interface WatchChatApi { state(threadId: string): Promise<WatchChatState>; command<T>(input: ChatCommand): Promise<T> }
export const desktopWatchChatApi: WatchChatApi = { state: threadId => invoke("codex_watch_chat", { threadId }), command: input => invoke("codex_watch_chat_command", { input }) };
export const webWatchChatApi: WatchChatApi = { state: threadId => request(`/codex-watches/chat/${encodeURIComponent(threadId)}`), command: input => request("/codex-watches/chat", { method: "POST", body: JSON.stringify(input) }) };
