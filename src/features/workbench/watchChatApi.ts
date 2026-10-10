import { invoke } from "@tauri-apps/api/core";
import { request } from "../../mobile/api";
import {readModels,chatCacheKey,historyCacheKey} from "./readModelCache";
import type { CodexWatch } from "./CodexNotifications";
import type { MobileCodexGoal, MobileCodexRequest } from "../../mobile/api";

export type ReplyOptions = { model?: string | null; effort?: string | null; attachments: string[] };
export type WatchReply = { id: string; threadId: string; sourceSequence: number | null; expectedTurnId: string | null; mode: "SEND" | "QUEUE" | "STEER"; text: string; status: "QUEUED" | "SENDING" | "SENT" | "UNKNOWN" | "ACKNOWLEDGED" | "CANCELLED" | "FAILED"; turnId: string | null; errorCode: string | null; createdAt: number; options: ReplyOptions };
export type GoalTarget={threadId:string;generation:number;workstreamId:string|null;bindingRevision:number|null;role:"DECISION"|"EXECUTION"|null;endpointId:string|null};
export type GoalControls={target:GoalTarget;canPause:boolean;canResume:boolean;resumeRequiresOwnerAnswer:boolean;blockedReason:string|null};
export type GoalControlReceipt={id:string;threadId:string;payloadHash:string;status:"READY"|"SENDING"|"APPLIED"|"REJECTED"|"UNKNOWN";result:MobileCodexGoal|null;errorCode:string|null};
export type NativeTurnDiagnostic={threadId:string;turnId:string;status:string;errorCode:string|null;willRetry:boolean|null;confirmedTerminal:boolean};
export type WatchChatState = {latestTurn?:NativeTurnDiagnostic|null;goalControls?:GoalControls|null; publicMessages?:ChatMessage[]; activity?:"THINKING"|"EXECUTING"|null; checkedAt?:number; watch: CodexWatch; host: string; ownedTurnId: string | null; controllableTurnId?:string|null; externalBusy: boolean; replies: WatchReply[]; requests: MobileCodexRequest[]; goal: MobileCodexGoal | null };
export type ChatCommand = { action: string; threadId: string; [key: string]: unknown };
export type ChatMessage = { id: string; turnId: string; role: "user" | "assistant"; text: string };
export type ChatHistory = { messages: ChatMessage[]; nextCursor: string | null };
export type ChatModel = { id: string; model: string; displayName: string; supportedReasoningEfforts: { reasoningEffort: string; description: string }[] };
export interface WatchChatApi { state(threadId: string): Promise<WatchChatState>; command<T>(input: ChatCommand): Promise<T>; cachedState?(threadId:string):WatchChatState|undefined; cachedHistory?(threadId:string):ChatHistory|undefined; subscribe?(threadId:string,listener:()=>void):()=>void }
const revision=(s:WatchChatState)=>JSON.stringify([s.watch.generation,s.watch.cwd,s.watch.snapshot.turnId,s.watch.snapshot.itemId,s.watch.snapshot.state]);
export function cachedWatchChatApi(raw:WatchChatApi):WatchChatApi {
 const state=async(threadId:string)=>{const next=await raw.state(threadId);if(next.watch.threadId!==threadId)throw Error("REPLY_TARGET_CHANGED_REFRESH");const old=readModels.peek<WatchChatState>(chatCacheKey(threadId));if(old&&revision(old)!==revision(next))readModels.invalidate(historyCacheKey(threadId));return next;};
 return {cachedState:id=>readModels.peek(chatCacheKey(id)),cachedHistory:id=>readModels.peek(historyCacheKey(id)),subscribe:(id,fn)=>{const a=readModels.subscribe(chatCacheKey(id),fn),b=readModels.subscribe(historyCacheKey(id),fn);return()=>{a();b();};},
 state:id=>readModels.read(chatCacheKey(id),()=>state(id)),
 command:async<T>(input:ChatCommand)=>{
  if(input.action==="HISTORY"&&!input.cursor)return readModels.read<T>(historyCacheKey(input.threadId),()=>raw.command<T>(input),15000);
  const result=await raw.command<T>(input);
  if(input.action==="GOAL_CONTROL"){readModels.invalidate("bridge:");readModels.invalidate("bridge-activity:");}
  if(!["HISTORY","OPTIONS","RECEIPT","GOAL_RECEIPT"].includes(input.action)){readModels.invalidate(chatCacheKey(input.threadId));readModels.invalidate(historyCacheKey(input.threadId));readModels.invalidate("notifications:");}
  return result;
 }};
}
export const desktopWatchChatApi = cachedWatchChatApi({state:threadId=>invoke("codex_watch_chat",{threadId}),command:input=>invoke("codex_watch_chat_command",{input})});
export const webWatchChatApi = cachedWatchChatApi({state:threadId=>request(`/codex-watches/chat/${encodeURIComponent(threadId)}`),command:input=>request("/codex-watches/chat",{method:"POST",body:JSON.stringify(input)})});
