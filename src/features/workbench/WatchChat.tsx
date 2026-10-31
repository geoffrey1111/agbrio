import {threadProviderLabel,threadAvatarKind} from "./watchPresentation";
import {useImeSubmitGuard} from "./useImeSubmitGuard";
import {AttachmentName} from "./AttachmentName";
import {MessageDelivery} from './MessageDelivery';
import {NativeTurnActions} from './NativeTurnActions';
import {ClaudeDecisionCards} from './ClaudeDecisionCards';
import {ClaudeTaskStatus,ClaudeChildReader,claudeTaskSummary} from "./ClaudeTaskStatus";
import {SelectionIndicator} from "./InteractionMotion";
import {useId} from "react";
import {CodexGoalController} from "./CodexGoalController";
import {t as uiText,useLanguage,getLanguage} from "../../i18n";
import {chatTimeline,chronologicalHistoryPage,mergeHistory,ownerMessageText,ownerAttachmentText,nativeCardUnavailable} from "./chatTimeline";
import {useBackLayer} from "./navigationHistory";
import {recordSheetEvidence} from "./displayDiagnostics";
import {MarkdownMessage} from "../codex/MarkdownMessage";
import {messageMedia} from "../codex/messageMedia";
import {useMemo} from "react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import {ArrowUp,ChevronLeft,Copy,Ellipsis,Plus,ShieldCheck,SlidersHorizontal,X,Square,LoaderCircle} from "lucide-react";
import * as Dialog from "@radix-ui/react-dialog";
import type {ReactNode} from "react";
import type { CodexWatch, WatchEvent } from "./CodexNotifications";
import type { ChatHistory, ChatMessage, ChatModel, ReplyOptions, ClaudeTaskTree, WatchChatApi, WatchChatState, WatchReply } from "./watchChatApi";
import {CodexRequestCard as RequestCard} from "./CodexRequestCard";
import {ConversationAvatar} from "./ConversationHome";
import {ModelBadge} from "./ModelBadge";
import {chatRuntime} from "./modelLabel";

const states: Record<string, string> = { get IDLE(){return uiText("尚无任务");}, get RUNNING(){return uiText("正在执行");}, get ACTION_REQUIRED(){return uiText("需要你确认或回答");}, get RESULT_READY(){return uiText("结果已到达");}, get RESULT_PENDING(){return uiText("正在读取结果");}, get FAILED(){return uiText("执行失败");}, get INTERRUPTED(){return uiText("已停止");}, get INCOMPLETE(){return uiText("执行状态待确认");}, get UNKNOWN(){return uiText("状态待确认");} };
const key = (id: string) => `aiwr-watch-draft:${id}`;
type FileRef = { id: string; name: string; size: number };
type Draft = { text: string; files: FileRef[]; model: string; effort: string };
type ObservedPublicMessage=ChatMessage&{seenAt:number};
function loadPublicMessages(id:string):ObservedPublicMessage[]{try{const v=JSON.parse(sessionStorage.getItem(`aiwr-watch-public-v3:${id}`)??"[]");return Array.isArray(v)?v.filter(m=>typeof m.id==="string"&&typeof m.turnId==="string"&&typeof m.text==="string"&&m.role==="assistant"&&typeof m.seenAt==="number").slice(-20):[];}catch{return [];}}
function loadDraft(id: string): Draft { try { const v = JSON.parse(sessionStorage.getItem(key(id)) ?? "null"); if (v && typeof v.text === "string" && Array.isArray(v.files)) return { ...v, model: v.model ?? "", effort: v.effort ?? "" }; } catch { /* private/session storage unavailable */ } return { text: "", files: [], model: "", effort: "" }; }
type PendingReply={id:string;text:string;options:ReplyOptions;mode?:WatchReply["mode"];createdAt?:number;sourceSequence?:number|null;expectedTurnId?:string|null;files?:FileRef[];composerCleared?:boolean};
function loadReceipt(id: string): PendingReply|null {try{return JSON.parse(sessionStorage.getItem(`${key(id)}:pending`)??"null");}catch{return null;}}
function ChatSheet({title,open,onOpenChange,children}:{title:string;open:boolean;onOpenChange:(open:boolean)=>void;children:ReactNode}){
 useLanguage();useEffect(()=>{if(!open)return;const timer=setTimeout(()=>recordSheetEvidence(title),300);return()=>clearTimeout(timer);},[open,title]);return <Dialog.Root open={open} onOpenChange={onOpenChange}><Dialog.Portal><Dialog.Overlay className="v4-chat-sheet-overlay"/><div className="v4-chat-sheet-frame"><Dialog.Content className="v4-chat-sheet" aria-describedby={undefined}><header><Dialog.Title>{uiText(title)}</Dialog.Title><Dialog.Close asChild><button type="button" aria-label={uiText("关闭{0}", title)}><X size={24} aria-hidden="true"/></button></Dialog.Close></header><div className="v4-chat-sheet-body">{children}</div></Dialog.Content></div></Dialog.Portal></Dialog.Root>;}
export function chatError(error: unknown,provider="Codex"): string {
 const piErrors:Record<string,string>={PI_ADMIN_BACKEND_PREPARE_ON_DESKTOP:'请先在电脑的“设置 → Pi 子任务”准备管理员后台，并确认 Windows 弹窗，然后继续原派单。',PI_ADMIN_BACKEND_DISCONNECTED:'管理员后台已断开。请在电脑的 Pi 子任务设置重新准备，再核对原动作状态。',PI_CONTEXT_CHANGED_BEFORE_SEND:'规则文件已更新，本条还没有发送。请关闭并恢复原 Pi 会话，重新加载规则后继续。',PI_UAC_CANCELLED:'Windows 管理员授权未完成，Pi 没有按普通权限继续启动。',PI_ELEVATION_NOT_COMPLETED:'尚未确认管理员启动结果，请查看原创建动作的回执。',PI_SCOPE_NOT_DELEGATED_TO_ASSISTANT:'这个助手连接还没有工作范围授权。请在电脑的 Pi 子任务设置中选择对应连接。',PI_CLOSE_BEFORE_ACCESS_CHANGE:'请先关闭这个 Pi 进程，再由管理助手重新分配目录权限。',PI_CONTEXT_NOT_LOADED:'规则没有完整载入，Pi 已停止本次执行。请恢复原会话后核对规则来源。',PI_CONTEXT_CHANGED:'规则在准备期间发生了变化，Pi 未继续执行。请检查发送记录并恢复原会话。',PI_SESSION_NOT_LOADED:'Pi 会话已关闭，请先恢复原会话。',PI_ORIGINAL_SESSION_NOT_DURABLE:'空 Pi 对话还未保存，不能恢复。请让管理对话重新创建。',PI_IMAGE_REQUIRES_PNG_OR_JPEG:'Pi 暂时支持 PNG 和 JPEG 图片，请换一张图片后再发送。',PI_PREVIOUS_COMMAND_UNCONFIRMED:'上一条消息的送达仍待核对，请先查看发送记录。',PI_SCOPE_CHANGED:'项目目录授权已变化，请在电脑端 Pi 设置中确认。',PI_SETTINGS_MISMATCH:'Pi 的实际模型设置与选择不符，任务尚未发送，请检查创建回执。',PI_GUARD_LOAD_UNVERIFIED:'Pi 的目录保护组件未通过检查，尚未发送任务。请检查电脑端 Pi 安装。',PI_PROXY_CREDENTIAL_INHERITANCE_DISABLED:'Pi 无法使用当前代理配置，请在电脑端检查网络连接。'};
 for(const[code,copy]of Object.entries(piErrors))if(String(error).includes(code))return uiText(copy);

 const text = String(error);
 const messages: Record<string, string> = { PUBLIC_CHAT_READ_UNAVAILABLE: uiText("暂时无法读取对话历史，当前通知和草稿已保留。"), REPLY_TARGET_ALREADY_RUNNING: uiText("这条对话正在执行，请使用引导跟进。"), REPLY_EXPLICIT_QUEUE_REQUIRED: uiText("排队需要明确选择，请重新打开对话后重试。"), REPLY_STEER_UNAVAILABLE_CHOOSE_QUEUE: uiText("当前暂不支持引导，可选择排队"), REPLY_TURN_CHANGED_REFRESH: uiText("原对话已有变化。请刷新状态后再决定是否发送。"), REPLY_TARGET_CHANGED_REFRESH: uiText("项目或监听信息已变化。请重新打开原对话。"), REPLY_PREVIOUS_PENDING_CHECK_FIRST: uiText("已有排队或送达未确认的回复。请先查看它的状态。"), REPLY_REQUEST_EXPIRED: uiText("这条执行请求已失效，请查看当前请求。"), REPLY_ATTACHMENT_TOO_LARGE: uiText("单个附件最多 10 MB。"), REPLY_ATTACHMENT_LIMIT: uiText("最多选择 4 个附件，总大小不超过 40 MB。"), REPLY_MODEL_UNAVAILABLE: uiText("所选模型目前不可用，请在对话选项中重新选择。"), REPLY_EFFORT_UNAVAILABLE: uiText("当前模型不支持这项思考强度。"), REPLY_ATTACHMENT_CHANGED: uiText("附件在上传后已变化，请重新选择。"), BRIDGE_TARGET_GOAL_ACTIVE: uiText("这条对话的 Goal 仍在执行。请先在原 Codex 中暂停 Goal，或调整当前任务。") };
 if(text.includes("REPLY_EXTERNAL_STATE_UNCONFIRMED"))return uiText("原 Codex 的任务是否结束尚未确认。请先在原对话确认完成，再刷新状态。");
 if(text.includes("WATCH_EVENT_NOT_FOUND"))return uiText("这条通知已清理，请从监听对话打开当前内容后再回复。草稿已保留。");
 for (const [code, copy] of Object.entries(messages)) if (text.includes(code)) return copy;
 for(const [code,copy] of Object.entries({CLAUDE_INPUT_PLUGIN_NOT_READY:"发送连接尚未就绪，请检查原 Claude Code 对话。草稿已保留。",CLAUDE_BUSY_BEFORE_DELIVERY:"原 Claude Code 已开始新任务，本条未发送。草稿已保留。",CLAUDE_NATIVE_APPROVAL_PENDING:"Claude Code 正在等待权限确认，请先在原对话处理。草稿已保留。",CLAUDE_TEXT_REPLY_ONLY:"Claude Code 当前支持文字回复，模型和权限沿用原设置。",CLAUDE_COMMAND_PUBLISHER_CHANGED:"Claude Code 会话连接已变化，请重新检查原对话。不会自动重发。"}))if(text.includes(code))return uiText(copy);
 if(text.includes("ROUTER_READ_TIMEOUT"))return uiText("读取对话超时，已有内容和草稿已保留。请重试。");
 if(text.toLowerCase().includes("disconnected"))return uiText(provider==="Claude Code"?"Claude Code 读取连接已断开，请检查原客户端后重试。草稿已保留。":"Codex 读取连接已断开，正在重新连接。已有内容和草稿已保留。");
 return text.includes("Failed to fetch")||text.includes("NetworkError")?uiText(location.pathname.startsWith("/mobile")?"暂时无法连接电脑。已有内容和草稿已保留，请重试。":"暂时无法读取对话，已有内容和草稿已保留。请重试。"):text;
}
// A local queued bubble can stay last while fresh native progress arrives.
// Compare only public content; observation timestamps never trigger the indicator.
function publicContentChanged(previous:ChatMessage[],next:ChatMessage[]){
 if(previous===next)return false;
 const a=previous.filter(m=>m.kind!=='COLLABORATION'),b=next.filter(m=>m.kind!=='COLLABORATION');
 return a.length!==b.length||a.some((m,i)=>m.id!==b[i]?.id||m.turnId!==b[i]?.turnId||m.text!==b[i]?.text);
}
/** Optional watch lifecycle actions offered from the conversation menu. */
export type WatchControls={enabled:boolean;toggle:()=>Promise<void>;remove:()=>Promise<void>};
export function WatchChat({ original, api, onBack,backLabel=uiText("最近通知"),controls }: { original: WatchEvent | CodexWatch; api: WatchChatApi; onBack: () => void;backLabel?:string;controls?:WatchControls }) {
 useLanguage();
 const ime=useImeSubmitGuard();
 const modeGroup=useId();
 const [sendInFlight,setSendInFlight]=useState(false);
 const claude=original.threadId.startsWith("claude_code:");
 const pi=original.threadId.startsWith("pi:");
 const provider=threadProviderLabel(original.threadId);
 const piBranch=useRef<number|undefined>(undefined);
 const [draft, setDraftState] = useState(() => loadDraft(original.threadId));
 const draftRef=useRef(draft);
 // Persist the new composer before retiring its request receipt, even if a
 // navigation/background event unmounts this reader before React effects run.
 function setDraft(update:Draft|((current:Draft)=>Draft)){
  const next=typeof update==="function"?update(draftRef.current):update;
  draftRef.current=next;setDraftState(next);
  try{sessionStorage.setItem(key(original.threadId),JSON.stringify(next));}catch{/* memory preserves it */}
 }
 function restoreAttempt(attempt:PendingReply){
  if(draftRef.current.text||draftRef.current.files.length)return;
  const known=attempt.options.attachments.map(id=>attempt.files?.find(f=>f.id===id)??{id,name:uiText("附件"),size:0});
  setDraft({...draftRef.current,text:attempt.text,files:known});
 }

 const [unconfirmed,setUnconfirmed]=useState(()=>loadReceipt(original.threadId));
 // Keep feedback for this visit, including an exact pending request restored on
 // reopen, without bringing old terminal delivery records back to the tail.
 const currentAttemptIds=useRef(new Set(unconfirmed?[unconfirmed.id]:[]));
 const [publicMessages,setPublicMessages]=useState(()=>loadPublicMessages(original.threadId));
 useEffect(()=>{const text=JSON.stringify(publicMessages);if(text.length<2_000_000)try{sessionStorage.setItem(`aiwr-watch-public-v3:${original.threadId}`,text);}catch{/* full messages still retained in memory and native public history */}},[publicMessages,original.threadId]);
 const [state, setState] = useState<WatchChatState | null>(()=>{const cached=api.cachedState?.(original.threadId);return cached?.watch.cwd===original.cwd?cached:null;}), [error, setError] = useState<string | null>(null), [notice, setNotice] = useState<string | null>(null), [busy, setBusy] = useState(false);
 const listIsNewer=!('sequence' in original)&&original.checkedAt>(state?.watch.checkedAt??0);
 // Normal sends report their state on the message; transient feedback never requires acknowledgement.
 useEffect(()=>{if(!notice)return;const timer=setTimeout(()=>setNotice(null),5000);return()=>clearTimeout(timer);},[notice]);
 const media=useMemo(()=>messageMedia({kind:"WATCH",threadId:original.threadId,sequence:"sequence" in original?original.sequence:null,turnId:original.snapshot.turnId,itemId:original.snapshot.itemId}),[original.threadId,"sequence" in original?original.sequence:null,original.snapshot.turnId,original.snapshot.itemId]);
 const [optionsOpen, setOptionsOpen] = useState(false), [models, setModels] = useState<ChatModel[] | null>(null), [history, setHistory] = useState<ChatMessage[]>(()=>chronologicalHistoryPage(api.cachedHistory?.(original.threadId)?.messages??[])), [historyCursor, setHistoryCursor] = useState<string | null>(()=>api.cachedHistory?.(original.threadId)?.nextCursor??null), [historyLoaded, setHistoryLoaded] = useState(()=>Boolean(api.cachedHistory?.(original.threadId)));
 const [attachmentOpen,setAttachmentOpen]=useState(false),[permissionsOpen,setPermissionsOpen]=useState(false),[infoOpen,setInfoOpen]=useState(false),[followupMode,setFollowupMode]=useState<"QUEUE"|"STEER">("STEER");
 const mounted = useRef(true), loading = useRef(false), actionRunning = useRef(false), reader = useRef<HTMLDivElement>(null), input = useRef<HTMLTextAreaElement>(null), fileInput = useRef<HTMLInputElement>(null),photoInput=useRef<HTMLInputElement>(null), requestSequence = useRef(0);
 const [stateReadError,setStateReadError]=useState<string|null>(null),[historyReadError,setHistoryReadError]=useState<{text:string;disconnected:boolean}|null>(null);
 const [childConversation,setChildConversation]=useState<CodexWatch|null>(null);
 const parentScrollPosition=useRef<number|null>(null);
 const [taskSheetOpen,setTaskSheetOpen]=useState(false),[selectedChild,setSelectedChild]=useState<ClaudeTaskTree['nodes'][number]|null>(null);
 useBackLayer(taskSheetOpen,()=>{if(selectedChild)setSelectedChild(null);else setTaskSheetOpen(false);});
 useEffect(()=>{if(!childConversation&&parentScrollPosition.current!==null)requestAnimationFrame(()=>{if(reader.current&&parentScrollPosition.current!==null){reader.current.scrollTop=parentScrollPosition.current;parentScrollPosition.current=null;}});},[childConversation]);
 async function openChildConversation(node:ClaudeTaskTree['nodes'][number]){if(!state)return;await act(async()=>{const child=await command<WatchChatState>('CHILD_OPEN',{generation:state.watch.generation,childId:node.sessionId});if(child.watch.threadId!==`claude_code:${node.sessionId}`)throw Error('REPLY_TARGET_CHANGED_REFRESH');parentScrollPosition.current=reader.current?.scrollTop??0;followLatest.current=false;setTaskSheetOpen(false);setSelectedChild(null);setChildConversation(child.watch);});}
 const heading=useRef<HTMLHeadingElement>(null);
 useBackLayer(attachmentOpen,()=>setAttachmentOpen(false));
 useBackLayer(optionsOpen,()=>setOptionsOpen(false));
 useBackLayer(permissionsOpen,()=>setPermissionsOpen(false));
 useBackLayer(infoOpen,()=>setInfoOpen(false));
 const shell=useRef<HTMLElement>(null),syncViewport=useRef<()=>void>(()=>{});
 useEffect(()=>{
  let width=window.innerWidth,baseline=window.visualViewport?.height??window.innerHeight;
  let settleTimer:ReturnType<typeof setTimeout>|undefined;
  const update=()=>{
   const viewport=window.visualViewport,element=shell.current;if(!element)return;
   if(viewport&&Math.abs(viewport.scale-1)>.05){element.style.removeProperty('--aiwr-chat-height');element.dataset.keyboard='false';return;}
   const height=viewport?.height??window.innerHeight;
   if(window.innerWidth!==width){width=window.innerWidth;baseline=height;}
   baseline=Math.max(baseline,height);
   if(width<=600||window.matchMedia?.('(hover:none) and (pointer:coarse) and (max-width:1100px)')?.matches){
    element.style.setProperty('--aiwr-chat-height',`${height}px`);
    element.dataset.keyboard=String(baseline-height>100&&document.activeElement?.tagName==='TEXTAREA');
   }else{element.style.removeProperty('--aiwr-chat-height');element.dataset.keyboard='false';}
  };
  const settle=()=>{update();clearTimeout(settleTimer);settleTimer=setTimeout(update,350);};
  syncViewport.current=update;update();window.addEventListener('resize',update);window.visualViewport?.addEventListener('resize',update);window.visualViewport?.addEventListener('scroll',update);document.addEventListener('focusout',settle);window.addEventListener('pageshow',settle);
  return()=>{clearTimeout(settleTimer);syncViewport.current=()=>{};window.removeEventListener('resize',update);window.visualViewport?.removeEventListener('resize',update);window.visualViewport?.removeEventListener('scroll',update);document.removeEventListener('focusout',settle);window.removeEventListener('pageshow',settle);};
 },[]);
 useEffect(()=>{heading.current?.focus({preventScroll:true});},[]);
 useLayoutEffect(()=>{const next=state?.piTask?.nativeBranch?.revision;if(!pi||next===undefined)return;if(piBranch.current!==undefined&&piBranch.current!==next){setHistory([]);setPublicMessages([]);void readHistory(null,true);}piBranch.current=next;},[pi,state?.piTask?.nativeBranch?.revision]);
 const pending = state?.replies.find(r => ["SENDING", "UNKNOWN"].includes(r.status)||(r.status==="QUEUED"&&(claude||pi||r.mode!=="QUEUE")));
 // Running follow-ups default to steering. Queueing always needs an explicit choice.
 const statusUnconfirmed=Boolean(stateReadError||historyReadError?.disconnected||state?.watch.errorCode);
 const active = (!listIsNewer&&Boolean(state?.ownedTurnId||state?.externalBusy)) || (listIsNewer?original.snapshot:state?.watch.snapshot??original.snapshot).state === "RUNNING";
 const waitingExternal=!pi&&Boolean(state&&["INCOMPLETE","UNKNOWN","RESULT_PENDING"].includes(state.watch.snapshot.state));
 const claudeNotReady=claude&&state?.inputReady===false;
 const piNotReady=pi&&state?.inputReady===false;
 const queueNext=piNotReady?false:active||waitingExternal||claudeNotReady;
 const pendingRequests=[...(state?.requests??[]),...(state?.asyncQuestions??[])].filter(r=>!r.responseSent);
 const controlTurn=state?.controllableTurnId??state?.ownedTurnId;
 const nextMode=queueNext?followupMode:"SEND";
 const blocked=Boolean(statusUnconfirmed||pending||unconfirmed||(nextMode==="SEND"&&state?.replies.some(r=>r.status==="QUEUED"))||state?.ownerRequests?.some(r=>["PENDING","READY","OFFERED"].includes(r.status)));
 const steerUnavailable=queueNext&&nextMode==="STEER"&&!controlTurn;
 useEffect(()=>{if(window.innerWidth<=600&&input.current){input.current.style.height="54px";input.current.style.height=`${Math.min(144,Math.max(54,input.current.scrollHeight))}px`;}},[draft.text]);
 useEffect(() => { try { sessionStorage.setItem(key(original.threadId), JSON.stringify(draftRef.current)); } catch { /* memory state still preserves current input */ } }, [draft, original.threadId]);
 async function refresh(force=false) { if (loading.current) return; loading.current = true; const seq = ++requestSequence.current; try { const next = await (force&&api.refreshState?api.refreshState(original.threadId):api.state(original.threadId)); if (next.watch.threadId !== original.threadId || next.watch.cwd !== original.cwd) throw new Error("REPLY_TARGET_CHANGED_REFRESH"); if (mounted.current && seq === requestSequence.current) { setState(next);const snap=next.watch.snapshot;if(next.publicMessages?.length){if(claude||pi)setHistory(old=>mergeHistory(old,next.publicMessages!));setPublicMessages(old=>{const merged=new Map(old.map(m=>[JSON.stringify([m.turnId,m.id]),m]));for(const m of next.publicMessages!){if(!m.id||!m.turnId)continue;const k=JSON.stringify([m.turnId,m.id]);merged.set(k,{...m,seenAt:merged.get(k)?.seenAt??Date.now()});}return mergeHistory(old,next.publicMessages!).map(m=>({...m,seenAt:merged.get(JSON.stringify([m.turnId,m.id]))?.seenAt??Date.now()})).slice(-40);});}if(snap.turnId&&snap.itemId&&snap.text){setPublicMessages(old=>{const prior=old.find(m=>m.turnId===snap.turnId&&m.id===snap.itemId);const message:ObservedPublicMessage={...prior,...next.publicMessages?.find(m=>m.turnId===snap.turnId&&m.id===snap.itemId),id:snap.itemId!,turnId:snap.turnId!,role:"assistant",text:snap.text,seenAt:prior?.seenAt??Date.now()};return [...old.filter(m=>m.turnId!==message.turnId||m.id!==message.id),message].slice(-20);});}setStateReadError(null);if(historyRetry.current)await readHistory(null,force); } } catch (e) { if (mounted.current && seq === requestSequence.current) setStateReadError(chatError(e,provider)); } finally { loading.current = false; } }
 useEffect(() => { mounted.current = true; const unsubscribe=api.subscribe?.(original.threadId,()=>{const next=api.cachedState?.(original.threadId);if(next?.watch.cwd===original.cwd&&mounted.current){setState(next);if(claude&&next.publicMessages?.length)setHistory(old=>mergeHistory(old,next.publicMessages!));}const h=api.cachedHistory?.(original.threadId);if(h&&mounted.current){setHistory(old=>mergeHistory(old,chronologicalHistoryPage(h.messages)));if(!olderPagesLoaded.current)setHistoryCursor(h.nextCursor);setHistoryLoaded(true);}});void refresh(true);const poll=()=>{if(document.visibilityState!=="hidden")void refresh();};const wake=()=>{if(document.visibilityState!=="hidden"){void refresh(true);void readHistory(null,true);}};const timer=setInterval(poll,5000);document.addEventListener("visibilitychange",wake);window.addEventListener("online",wake);window.addEventListener("pageshow",wake);window.addEventListener("focus",wake);return()=>{mounted.current=false;requestSequence.current++;clearInterval(timer);unsubscribe?.();document.removeEventListener("visibilitychange",wake);window.removeEventListener("online",wake);window.removeEventListener("pageshow",wake);window.removeEventListener("focus",wake);}; }, [api, original.threadId]);
 async function act<T,>(work: () => Promise<T>):Promise<T|undefined> { if (actionRunning.current) return; actionRunning.current = true; setBusy(true); setError(null); try { return await work(); } catch (e) { if (mounted.current) setError(chatError(e,provider)); } finally { actionRunning.current = false; if (mounted.current) setBusy(false); } }
 function command<T>(action: string, extra: object = {}) { return api.command<T>({ action, threadId: original.threadId, ...extra }); }
 function clearReceipt(){setFollowupMode("STEER");setUnconfirmed(null);try{sessionStorage.removeItem(`${key(original.threadId)}:pending`);}catch{/* no credentials stored */}}
 async function checkReceipt(){if(!unconfirmed)return;const r=await command<WatchReply|null>("RECEIPT",{id:unconfirmed.id});if(!mounted.current)return;if(r&&(r.id!==unconfirmed.id||r.threadId!==original.threadId))throw Error("REPLY_TARGET_CHANGED_REFRESH");if(r&&["SENT","QUEUED"].includes(r.status)){const sent=unconfirmed;if(!sent.composerCleared)setDraft(d=>d.text===sent.text&&JSON.stringify(d.files.map(f=>f.id))===JSON.stringify(sent.options.attachments)?{...d,text:"",files:[]}:d);clearReceipt();setNotice(null);}else if(r&&r.status==="ACKNOWLEDGED"){clearReceipt();setNotice(uiText("这条回复已结束送达检查，送达状态仍未确定，可以输入新的要求。"));}else if(r&&["FAILED","CANCELLED"].includes(r.status)){restoreAttempt(unconfirmed);clearReceipt();setNotice(uiText("已确认这次没有发送。草稿保留，可检查后重新发送。"));}else if(!r){
  // Agbrio has no record of this attempt (for example the request was lost while
  // Agbrio restarted). Abandoning records a block, so a delayed copy can never be
  // accepted later; the draft comes back for an explicit resend.
  const v=await command<{cancelled:boolean}>("ABANDON",{id:unconfirmed.id});if(!mounted.current)return;
  if(v.cancelled){restoreAttempt(unconfirmed);clearReceipt();setNotice(uiText("Agbrio 没有收到这次发送，已取消；草稿保留，可以重新发送。"));}
  else setNotice(uiText("这条回复已进入发送，请查看原对话，不会重复发送。"));
 }else setNotice(uiText("送达尚未确认。不会自动重发；请查看原对话或取消尚未发送的尝试。"));await refresh();}
 // Settle a leftover unconfirmed attempt as soon as the conversation opens.
 const autoChecked=useRef<string|null>(null);
 useEffect(()=>{
  if(!unconfirmed||busy||!state)return;
  const receipt=state.replies.find(r=>r.id===unconfirmed.id&&r.threadId===original.threadId);
  // Polling can prove acceptance after an earlier receipt lookup was uncertain.
  // Reconcile only this attempt; never issue SEND or manufacture a retry id.
  if(receipt&&['SENT','QUEUED','ACKNOWLEDGED','FAILED','CANCELLED'].includes(receipt.status)){
   if(['FAILED','CANCELLED'].includes(receipt.status))restoreAttempt(unconfirmed);
   else if(!unconfirmed.composerCleared)setDraft(d=>d.text===unconfirmed.text&&JSON.stringify(d.files.map(f=>f.id))===JSON.stringify(unconfirmed.options.attachments)?{...d,text:'',files:[]}:d);
   clearReceipt();setNotice(null);return;
  }
  if(autoChecked.current===unconfirmed.id)return;
  autoChecked.current=unconfirmed.id;void checkReceipt().catch(()=>{});
 },[unconfirmed,state,busy]);
 async function acknowledgeUnknown(r:WatchReply){await command("ACKNOWLEDGE_UNKNOWN",{id:r.id,confirmed:true});currentAttemptIds.current.add(r.id);if(unconfirmed?.id===r.id)clearReceipt();if(!unconfirmed?.composerCleared)setDraft(d=>d.text===r.text?{...d,text:"",files:[]}:d);setNotice(uiText("已结束这条回复的送达检查，原文仍保留。可以输入新的要求。"));await refresh();input.current?.focus();}
 async function send(mode: "SEND" | "QUEUE" | "STEER",selectedText?:string) {
  if(mode==="STEER"&&!controlTurn)throw Error("REPLY_STEER_UNAVAILABLE_CHOOSE_QUEUE");
  const submitted=selectedText===undefined?draftRef.current:{text:selectedText,files:[],model:"",effort:""};
  if (!state || (!submitted.text.trim()&&(!pi||!submitted.files.length)) || blocked || piNotReady) return;
  const options: ReplyOptions = { model: claude||pi||mode === "STEER" ? null : submitted.model || null, effort: claude||pi||mode === "STEER" ? null : submitted.effort || null, attachments: submitted.files.map(f => f.id) };
  const request = { id: crypto.randomUUID(), generation: state.watch.generation, sourceSequence: "sequence" in original ? original.sequence : null, expectedTurnId: mode === "STEER" ? controlTurn : state.watch.snapshot.turnId, mode, queueRequested:mode === "QUEUE", text: submitted.text, options };
  currentAttemptIds.current.add(request.id);
  // Retain the exact id after an ambiguous HTTP outcome; never mint a retry.
  const receiptKey = `${key(original.threadId)}:pending`;
  const attempt={...request,createdAt:Date.now(),files:submitted.files,composerCleared:selectedText===undefined};
  try { sessionStorage.setItem(receiptKey, JSON.stringify(attempt)); } catch { /* current request remains in memory */ }
  setUnconfirmed(attempt);
  followLatest.current=true;
  if(selectedText===undefined)setDraft({...submitted,text:"",files:[]});
  input.current?.focus({preventScroll:true});
  let reply: WatchReply;setSendInFlight(true);
  try { reply = await command<WatchReply>("SEND", request); } catch (e) {
   if (!mounted.current) return;
   // A refusal usually means nothing was stored. Ask the Host for this exact id: with
   // no record, cancel it (a delayed copy can then never be accepted), give the text
   // back and show the real reason. Only an unanswerable check stays unconfirmed.
   const settled=await (async()=>{try{
    const r=await command<WatchReply|null>("RECEIPT",{id:request.id});
    if(r)return false;
    const v=await command<{cancelled:boolean}>("ABANDON",{id:request.id});
    if(!v.cancelled)return false;
    restoreAttempt(attempt);clearReceipt();return true;
   }catch{return false;}})();
   if (!mounted.current) return;
   setError(chatError(e,provider));
   setNotice(settled?uiText("这条没有发出，原文已放回输入框。"):uiText("回复是否送达尚未确认。请检查发送记录；草稿已保留，不会自动重发。"));
   await refresh(); return settled?undefined:request.id; } finally {if(mounted.current)setSendInFlight(false);}
  if (!mounted.current) return;
  if(reply.id!==request.id||reply.threadId!==original.threadId)throw Error("REPLY_TARGET_CHANGED_REFRESH");
  setState(current=>current?{...current,replies:[reply,...current.replies.filter(r=>r.id!==reply.id)]}:current);
  if (reply.status === "SENT" || reply.status === "QUEUED") { clearReceipt(); setNotice(null); }
  else if(["FAILED","CANCELLED"].includes(reply.status)){restoreAttempt(attempt);clearReceipt();setNotice(uiText("已确认这次没有发送。草稿保留，可检查后重新发送。"));}
  else setNotice(uiText("回复送达尚未确认。请检查原对话，避免重复发送。"));
  await refresh();
  return request.id;
 }
 async function upload(selected: FileList | null) {
  if (!selected?.length) return; const list = Array.from(selected);
  if (draft.files.length + list.length > 4 || list.some(f => f.size > 10 * 1024 * 1024) || list.reduce((n, f) => n + f.size, draft.files.reduce((n, f) => n + f.size, 0)) > 40 * 1024 * 1024) throw new Error("REPLY_ATTACHMENT_LIMIT");
  for (const f of list) { const data = await new Promise<string>((resolve, reject) => { const r = new FileReader(); r.onerror = () => reject(new Error(uiText("无法读取所选文件"))); r.onload = () => resolve(String(r.result).split(",")[1]); r.readAsDataURL(f); }); const ref = await command<FileRef>("UPLOAD", { name: f.name, data }); if (mounted.current) setDraft(d => ({ ...d, files: [...d.files, ref] })); }
 }
 const latest = state?.watch.snapshot;
 const childNotification=claude&&original.snapshot.itemId?.startsWith("child_result:");
 const sourceSnapshot=listIsNewer?original.snapshot:childNotification?(state?.watch.snapshot??{turnId:null,itemId:null,text:''}):!('sequence' in original)&&state?state.watch.snapshot:original.snapshot;
 const sourceMessage={id:sourceSnapshot.itemId??'notification',turnId:sourceSnapshot.turnId??'notification',role:'assistant' as const,text:sourceSnapshot.text||uiText("暂时没有可读取的公开消息。"),seenAt:('observedAt' in original?original.observedAt:original.checkedAt)??Date.now()};
 const currentMessage=!listIsNewer&&latest?.turnId&&latest?.itemId&&latest.text?(state?.publicMessages?.find(m=>m.turnId===latest.turnId&&m.id===latest.itemId)??{id:latest.itemId,turnId:latest.turnId,role:'assistant' as const,text:latest.text}):undefined;
 const localPending:WatchReply|undefined=unconfirmed&&!state?.replies.some(r=>r.id===unconfirmed.id)?{id:unconfirmed.id,threadId:original.threadId,sourceSequence:unconfirmed.sourceSequence??null,expectedTurnId:unconfirmed.expectedTurnId??null,mode:unconfirmed.mode??"SEND",text:unconfirmed.text,options:unconfirmed.options,status:sendInFlight?"SENDING":"UNKNOWN",turnId:null,errorCode:null,createdAt:unconfirmed.createdAt??Date.now()}:undefined;
 const visibleHistory=listIsNewer&&sourceSnapshot.text?history.map(m=>m.id===sourceSnapshot.itemId&&m.turnId===sourceSnapshot.turnId?{...m,text:sourceSnapshot.text}:m):history;
 const timeline=chatTimeline(visibleHistory,publicMessages,[...(state?.replies??[]),...(localPending?[localPending]:[])],sourceMessage,historyLoaded,currentMessage,"sequence" in original&&latest?.turnId!==sourceMessage.turnId,currentAttemptIds.current);
 const [historyCoverage,setHistoryCoverage]=useState<string>();
 const [newMessages,setNewMessages]=useState(false);
 const [awayFromLatest,setAwayFromLatest]=useState(false);
 const scrollIntent=useRef<"up"|"down"|null>(null),touchY=useRef<number|null>(null);
 const olderPagesLoaded=useRef(false);
 const historyRetry=useRef(false);
 const historyReading=useRef(false),historyRevision=useRef(''),followLatest=useRef(true),initialScroll=useRef(true);
 function userScroll(direction:'up'|'down'){scrollIntent.current=direction;if(direction==='up'){followLatest.current=false;initialScroll.current=false;}}
 function readerScrolled(){const scroll=reader.current;if(!scroll)return;const distance=scroll.scrollHeight-scroll.scrollTop-scroll.clientHeight;
  setAwayFromLatest(distance>72);
  if(distance>24)followLatest.current=false;
  else if(scrollIntent.current!=='up')followLatest.current=true;
  if(followLatest.current)setNewMessages(false);
  if(!initialScroll.current&&scroll.scrollTop<80&&historyCursor&&!historyReading.current)void readHistory(historyCursor);
 }

 async function readHistory(cursor:string|null=null,force=false){
  if(historyReading.current)return;historyReading.current=true;
  const scroll=reader.current,previousHeight=scroll?.scrollHeight??0,previousTop=scroll?.scrollTop??0;
  try{const v=await(force&&!cursor&&api.refreshHistory?api.refreshHistory(original.threadId):command<ChatHistory>('HISTORY',{cursor}));if(!mounted.current||!v||!Array.isArray(v.messages))return;
   historyRetry.current=false;setHistoryReadError(null);const page=chronologicalHistoryPage(v.messages);setHistoryCoverage(v.coverage);
   setHistory(old=>cursor?mergeHistory(page,old):mergeHistory(old,page));if(cursor)olderPagesLoaded.current=true;if(cursor||!olderPagesLoaded.current)setHistoryCursor(v.nextCursor);setHistoryLoaded(true);
   if(cursor)requestAnimationFrame(()=>{if(scroll)scroll.scrollTop=previousTop+scroll.scrollHeight-previousHeight;});
  }catch(e){historyRetry.current=true;if(mounted.current)setHistoryReadError({text:chatError(e,provider),disconnected:String(e).toLowerCase().includes("disconnected")});}finally{historyReading.current=false;}
 }
 useEffect(()=>{const snap=state?.watch.snapshot??original.snapshot;
  const revision=JSON.stringify([snap.turnId,snap.itemId,snap.state,state?.historyRevision]);
  if(historyRevision.current===revision||historyReading.current)return;
  historyRevision.current=revision;void readHistory(null,true);
 },[state?.watch.snapshot.turnId,state?.watch.snapshot.itemId,state?.watch.snapshot.state,state?.historyRevision,history]);
 const lastVisible=timeline.at(-1);
 const latestVisible=JSON.stringify([lastVisible?.turnId,lastVisible?.id,lastVisible?.text,lastVisible?.receipt?.status]);
 const seenLatest=useRef(latestVisible),seenPublicContent=useRef(publicMessages),seenHistoryContent=useRef(history);
 useLayoutEffect(()=>{const changed=seenLatest.current!==latestVisible||publicContentChanged(seenPublicContent.current,publicMessages)||publicContentChanged(seenHistoryContent.current,history);seenLatest.current=latestVisible;seenPublicContent.current=publicMessages;seenHistoryContent.current=history;if(!initialScroll.current&&!followLatest.current){if(changed)setNewMessages(true);return;}setNewMessages(false);const scroll=reader.current;if(scroll){scroll.scrollTop=scroll.scrollHeight;initialScroll.current=false;}},[historyLoaded,history,publicMessages,state?.replies,unconfirmed,latestVisible]);
 const readWarning=stateReadError??historyReadError?.text;
 if(childConversation)return <WatchChat key={childConversation.threadId} original={childConversation} api={api} onBack={()=>setChildConversation(null)} backLabel={uiText('返回主对话')}/>;
 return <section ref={shell} className="v4-watch-chat v6-chat" aria-label={uiText("通知原对话与回复")} onFocus={()=>syncViewport.current()} onBlur={()=>queueMicrotask(()=>syncViewport.current())}>
  <header className="v4-chat-header"><button type="button" className="v4-chat-back" aria-label={`‹ ${backLabel}`} onClick={onBack}><ChevronLeft size={24} aria-hidden="true"/><span className="v4-chat-desktop-label">{backLabel}</span></button><ConversationAvatar id={`w:${original.threadId}`} kind={threadAvatarKind(original.threadId)}/><div className="v4-chat-identity"><h2 ref={heading} tabIndex={-1}>{childNotification?(state?.watch.label??original.label):original.label}</h2><p className="v4-meta v7-chat-meta"><span>{provider}</span><ModelBadge runtime={chatRuntime(state)}/><span>{original.cwd.split(/[\\/]/).filter(Boolean).at(-1)}</span><span>{state?.host ?? uiText("这台电脑")}</span></p></div>
  <div className="v4-chat-live" role="status" aria-live="polite" data-active={!statusUnconfirmed&&(active||Boolean(state?.taskTree?.running))}>{!statusUnconfirmed&&(active||Boolean(state?.taskTree?.running))&&<LoaderCircle size={14} aria-hidden="true"/>}<span>{statusUnconfirmed?uiText("状态待确认"):claude&&state?.taskTree&&(!active||state.taskTree.attention>0)&&claudeTaskSummary(state.taskTree)?claudeTaskSummary(state.taskTree):pendingRequests.length?uiText("等待确认"):active?(state?.activity==="THINKING"?uiText("正在思考"):uiText("正在执行")):(claude&&(latest?.state??original.snapshot.state)==="RESULT_READY"?uiText("本轮已结束"):(states[latest?.state??original.snapshot.state]??uiText("状态待确认")))}</span>{claude&&state?.taskTree&&(state.taskTree.nodes.length>0||state.taskTree.limited)&&<button type="button" aria-label={uiText('查看子对话')} onClick={()=>{setSelectedChild(null);setTaskSheetOpen(true);}}>{uiText('查看子对话')} ›</button>}{pendingRequests.length>0&&<button type="button" onClick={()=>{const card=reader.current?.querySelector(".v4-question-card");if(card)card.scrollIntoView({block:"end"});else setPermissionsOpen(true);}}>{uiText(pendingRequests.some(r=>r.kind==="USER_INPUT")?"回答问题":"查看")}</button>}</div><button type="button" className="v4-chat-more" aria-label={uiText("对话信息")} onClick={()=>setInfoOpen(true)}><Ellipsis size={24} aria-hidden="true"/></button></header>
  <div className="v4-chat-reader" ref={reader} onScroll={readerScrolled} onPointerDown={e=>{if(e.target===e.currentTarget)scrollIntent.current=null;}} onWheel={e=>{if(e.deltaY)userScroll(e.deltaY<0?'up':'down');}} onTouchStart={e=>{touchY.current=e.touches[0]?.clientY??null;}} onTouchMove={e=>{const y=e.touches[0]?.clientY;if(y!==undefined&&touchY.current!==null&&Math.abs(y-touchY.current)>2){userScroll(y>touchY.current?'up':'down');touchY.current=y;}}} onKeyDown={e=>{if(['ArrowUp','PageUp','Home'].includes(e.key))userScroll('up');if(['ArrowDown','PageDown','End'].includes(e.key))userScroll('down');}}>
   {(error||readWarning) && <p role="alert" className="v4-chat-alert">{uiText(error??readWarning)}<button type="button" disabled={busy} onClick={() => void act(()=>refresh(true))}>{uiText("检查状态")}</button></p>}

   {childNotification&&<details className="v4-chat-collaboration" open><summary>{uiText('子对话结果')} · {original.label}</summary><MarkdownMessage text={original.snapshot.text}/></details>}
   {!!state?.piChildren?.length&&<details className="v4-chat-collaboration pi-delegations"><summary>{uiText("委派的 Pi 任务")} · {state.piChildren.length}</summary>{state.piChildren.map(child=><button type="button" key={child.watch.threadId} onClick={()=>setChildConversation(child.watch)}><ConversationAvatar id={`w:${child.watch.threadId}`} kind="pi"/><span><strong>{child.watch.label}</strong><small>{uiText(({RUNNING:'执行中',RESULT_READY:'结果已到达',FAILED:'执行失败',IDLE:'等待任务',INTERRUPTED:'已停止'} as Record<string,string>)[child.watch.snapshot.state]??'状态待核对')}</small></span></button>)}</details>}
   {piNotReady&&<div className="v4-chat-reader-status"><p>{uiText(state?.piTask?.canRestore?'Pi 会话已关闭，恢复原会话后可以继续跟进。':'这个 Pi 对话尚未形成持久历史，请让管理对话重新创建任务。')}</p>{state?.piTask?.canRestore&&<button type="button" disabled={busy} onClick={()=>void act(async()=>{await command('RESTORE',{generation:state.watch.generation});await refresh(true);})}>{uiText('恢复 Pi 会话')}</button>}</div>}
   {claude&&state?.taskTree&&<ClaudeTaskStatus tree={state.taskTree} api={api} threadId={original.threadId} generation={state.watch.generation} notificationsEnabled={state.childResultsEnabled??false} onNotificationsChanged={refresh} onOpenChild={node=>void openChildConversation(node)}/>}
   <div className="v4-chat-history">{historyCursor&&<button type="button" disabled={busy||historyReading.current} onClick={()=>void readHistory(historyCursor)}>{uiText("加载更早消息")}</button>}{historyCoverage==="ADAPTER_RECEIVED_ONLY"&&<p role="status">{uiText("暂时只能读取已收到的消息，完整历史尚不可用。")}</p>}{!historyLoaded&&historyRetry.current&&!readWarning&&<button type="button" disabled={busy||historyReading.current} onClick={()=>void readHistory(null,true)}>{uiText("读取对话消息")}</button>}</div>
   {timeline.filter(m=>m.id!=="notification"||listIsNewer||!historyLoaded||!history.length).map(m=>{const r=m.receipt,attachmentText=m.role==='user'?ownerAttachmentText(m.text):'',attachmentCount=r?.options.attachments.length||((attachmentText.match(/SHA256：[a-f0-9]{64}/g)||[]).length);if(m.kind==="TOOL")return <details key={`${m.turnId}:${m.id}`} className="v4-chat-collaboration pi-tool-result"><summary>{uiText(m.isError?'工具报错':'工具输出')} · {m.toolName??'Pi'}</summary><MarkdownMessage text={m.text} media={messageMedia({kind:'WATCH',threadId:original.threadId,turnId:m.turnId,itemId:m.id})}/></details>;if(m.kind==="COLLABORATION")return <details key={`${m.turnId}:${m.id}`} className="v4-chat-collaboration"><summary>{uiText("内部协作消息")}{m.timestamp?` · ${new Date(m.timestamp).toLocaleTimeString(getLanguage(),{hour:"2-digit",minute:"2-digit"})}`:""}</summary><MarkdownMessage text={m.text}/></details>;return <div key={`${m.turnId}:${m.id}`} className="v4-chat-message-row" data-role={m.role}>{r&&<MessageDelivery status={r.status}>{r.status==='QUEUED'&&!pi&&<button type="button" disabled={busy} onClick={()=>void act(async()=>{await command('CANCEL',{id:r.id});await refresh();})}>{uiText("取消排队")}</button>}{r.status==='UNKNOWN'&&<><p>{uiText("回复可能已发出。请先检查原对话，不会自动重发。")}</p>{unconfirmed?.id===r.id&&<button type="button" disabled={busy} onClick={()=>void act(checkReceipt)}>{uiText("检查发送记录")}</button>}{unconfirmed?.id===r.id&&<button type="button" disabled={busy} onClick={()=>void act(async()=>{const v=await command<{cancelled:boolean}>("ABANDON",{id:unconfirmed.id});if(v.cancelled){restoreAttempt(unconfirmed);clearReceipt();}await refresh();})}>{uiText("取消尚未发送的尝试")}</button>}{r.id!==localPending?.id&&<button type="button" disabled={busy} onClick={()=>void act(()=>acknowledgeUnknown(r))}>{uiText("我已检查原对话，继续输入新回复")}</button>}</>}{r.status==='FAILED'&&<p>{uiText("这条回复没有完成发送。请检查当前对话后重新输入。")}</p>}</MessageDelivery>}<article className="v4-chat-message" data-role={m.role} data-message-kind={m.id.startsWith("receipt:")?"receipt":"native"} aria-label={m.source?uiText("完整通知内容"):undefined}>
    {(()=>{const meta=m.role==='user'?'':m.source&&'sequence' in original&&latest?.turnId!==m.turnId?`${provider} · ${uiText("通知原文")}`:'';return meta?<p className="v4-meta">{meta}</p>:null;})()}
    <MarkdownMessage text={m.role==='user'?ownerMessageText(r&&!pi?r.text:m.text):m.text} media={m.source?media:messageMedia({kind:'WATCH',threadId:original.threadId,turnId:m.turnId,itemId:m.id})}/>
    {attachmentCount>0&&!(pi&&!attachmentText&&m.text.includes("](pi-media/"))&&<details className="v4-chat-attachments"><summary>{uiText('附件 · {0} 项',attachmentCount)}</summary><MarkdownMessage text={attachmentText||uiText('附件预览暂不可用。')} media={messageMedia({kind:'WATCH',threadId:original.threadId,turnId:m.turnId,itemId:m.id})}/></details>}
    {m.card&&<NativeTurnActions card={m.card} replies={state?.replies??[]} disabled={busy||blocked||steerUnavailable} unavailable={nativeCardUnavailable(m,timeline)} submit={text=>act(()=>send(nextMode,text))}/>}
    {m.source&&<div className="v4-chat-message-tools"><button type="button" aria-label={uiText("复制原文")} onClick={()=>void act(async()=>{await navigator.clipboard.writeText(m.text);setNotice(uiText("原文已复制。"));})}><Copy size={24} aria-hidden="true"/></button>{'sequence' in original&&<time className="v4-meta">{new Date(sourceMessage.seenAt).toLocaleString(getLanguage())}</time>}</div>}
   </article></div>;})}
   {state?.requests.filter(r=>r.kind!=="USER_INPUT").map(r => <RequestCard key={`${r.requestId}:${r.revision}`} request={r} disabled={busy} respond={extra => act(async () => { await command("RESPOND", { requestId: r.requestId, input: extra }); await refresh(); })} />)}
   {state?.requests.filter(r=>r.kind==="USER_INPUT").map(r=><RequestCard key={`${r.requestId}:${r.revision}`} request={r} disabled={busy} respond={extra=>act(async()=>{await command("RESPOND",{requestId:r.requestId,input:extra});await refresh();})}/>)}
   {state?.asyncQuestions?.map(r=><RequestCard key={`async:${r.requestId}`} request={r} disabled={busy||blocked} allowSkip={false} lockAfterSubmit persistAnswers={false} respond={extra=>act(async()=>{const input=extra as {revision:number;answers:Record<string,string>};const id=r.answerId;currentAttemptIds.current.add(id);await command("ASYNC_RESPOND",{id,requestId:r.requestId,revision:input.revision,answers:input.answers});await refresh();})}/>)}

  </div>
  <div className="v4-chat-dock">
   {(newMessages||awayFromLatest)&&<button type="button" className="v4-chat-new-messages" onClick={()=>{scrollIntent.current='down';followLatest.current=true;setNewMessages(false);setAwayFromLatest(false);reader.current?.scrollTo({top:reader.current.scrollHeight,behavior:"instant"});}}>{uiText("回到最新消息")}</button>}
   {notice&&!error&&!pending&&!unconfirmed&&<p role="status" className="v4-chat-feedback">{uiText(notice)}</p>}
   {claude&&state?.ownerRequests&&<ClaudeDecisionCards requests={state.ownerRequests} api={api} threadId={original.threadId} generation={state.watch.generation} refresh={refresh}/>}
   {state?.goal&&<CodexGoalController goal={state.goal} controls={state.goalControls} diagnostic={state.latestTurn} api={api} checkedAt={state.checkedAt} disabled={busy||blocked} refresh={refresh}/>}

   <form className="v4-chat-composer" aria-busy={busy} onSubmit={e => { e.preventDefault(); void act(() => send(nextMode)); }}>
   <label htmlFor="watch-chat-reply">{uiText(claude?"回复这个 Claude Code 对话":"回复这个 Codex 对话")}</label><textarea id="watch-chat-reply" ref={input} placeholder={queueNext?(nextMode==="STEER"?uiText("告诉它现在要怎么调整"):uiText("写下一条，完成后自动发送")):uiText("回复 {0}", childNotification?(state?.watch.label??original.label):original.label)} value={draft.text} maxLength={100000} rows={2} onChange={e => setDraft(d => ({ ...d, text: e.target.value }))} onCompositionStart={ime.onCompositionStart} onCompositionEnd={ime.onCompositionEnd} onKeyDown={e => { if ((e.ctrlKey || e.metaKey) && e.key === "Enter" && !ime.blocks(e.nativeEvent)) { e.preventDefault(); if (!busy) void act(() => send(nextMode)); } }} />
   {draft.files.length > 0 && <ul className="v4-chat-files">{draft.files.map(f => <li key={f.id}><AttachmentName name={f.name}/><small className="v4-file-size">{Math.ceil(f.size / 1024)} KB</small><button type="button" aria-label={uiText("移除 {0}", f.name)} disabled={busy} onClick={() => setDraft(d => ({ ...d, files: d.files.filter(v => v.id !== f.id) }))}>{uiText("移除")}</button></li>)}</ul>}
   <div className="v4-chat-composer-tools"><input ref={fileInput} type="file" accept={pi?"image/png,image/jpeg":undefined} multiple hidden onChange={e => { const fs = e.currentTarget.files; void act(() => upload(fs)); e.currentTarget.value = ""; }} /><input ref={photoInput} type="file" accept={pi?"image/png,image/jpeg":"image/png,image/jpeg,image/webp,image/gif"} multiple hidden onChange={e=>{const fs=e.currentTarget.files;void act(()=>upload(fs));e.currentTarget.value="";}}/><button type="button" className="v4-chat-icon" aria-label={uiText("＋ 附件")} disabled={busy || draft.files.length >= 4} onClick={() => {if(window.matchMedia?.("(max-width:760px),(hover:none) and (pointer:coarse) and (max-width:1100px)").matches)setAttachmentOpen(true);else fileInput.current?.click();}}><Plus size={24} aria-hidden="true"/><span className="v4-chat-desktop-label">{uiText("附件")}</span></button><button type="button" className="v4-chat-icon v4-chat-permissions" aria-label={uiText("权限与执行请求{0}", pendingRequests.length?uiText("，{0}条待处理", pendingRequests.length):"")} onClick={()=>setPermissionsOpen(true)}><ShieldCheck size={24} aria-hidden="true"/>{pendingRequests.length>0&&<span className="v4-chat-request-count" aria-hidden="true">{pendingRequests.length}</span>}</button>{queueNext&&<div className="v6-followup" data-steer={controlTurn?"ready":"unavailable"}><div className="v4-followup-toggle agbrio-segmented" role="group" aria-label={uiText("运行中跟进方式")}><button type="button" aria-pressed={followupMode==="STEER"} disabled={busy||!controlTurn} onClick={()=>setFollowupMode("STEER")}><SelectionIndicator group={modeGroup} active={followupMode==="STEER"}/>{uiText("引导")}</button><button type="button" aria-pressed={followupMode==="QUEUE"} disabled={busy} onClick={()=>setFollowupMode("QUEUE")}><SelectionIndicator group={modeGroup} active={followupMode==="QUEUE"}/>{uiText("排队")}</button></div><span>{followupMode==="QUEUE"?uiText("等这一轮结束再发"):!controlTurn?uiText(claudeNotReady?"发送连接尚未就绪，可选择排队":"当前暂不支持引导，可选择排队"):uiText("立刻插入当前这一轮")}</span></div>}<span className="v4-chat-tool-space"/><button type="button" className="v4-chat-icon" aria-label={uiText("对话选项")} aria-expanded={optionsOpen} disabled={busy} onClick={() => { setOptionsOpen(v => !v); if (!models) void act(async () => { const v = await command<{ models: ChatModel[] }>("OPTIONS"); if (mounted.current) setModels(v.models); }); }}><SlidersHorizontal size={24} aria-hidden="true"/><span className="v4-chat-desktop-label">{uiText("对话选项")}</span></button>{controlTurn&&!draft.text.trim()?<button type="button" className="v4-chat-send" aria-label={uiText("停止本次执行")} disabled={busy} onClick={()=>void act(async()=>{await command("STOP",{turnId:controlTurn});setNotice(uiText("已请求停止，等待 {0} 确认。",provider));await refresh();})}><Square size={24} aria-hidden="true"/></button>:<button type="submit" className="v3-primary v4-chat-send" aria-label={nextMode==="STEER"?uiText("调整当前任务"):queueNext?uiText("排队，完成后发送"):uiText("发送回复")} disabled={busy || !state || (!draft.text.trim()&&(!pi||!draft.files.length)) || blocked || steerUnavailable || piNotReady}>{busy?<LoaderCircle size={24} className="v4-progress-spinner" aria-hidden="true"/>:<ArrowUp size={24} aria-hidden="true"/>}<span className="v4-chat-desktop-label">{busy?uiText("正在处理…"):nextMode==="STEER"?uiText("调整当前任务"):queueNext?uiText("排队，完成后发送"):uiText("发送回复")}</span></button>}</div>
  </form></div>
  {claude&&state?.taskTree&&<ChatSheet title={selectedChild?.title??uiText('子对话')} open={taskSheetOpen} onOpenChange={open=>{if(!open&&selectedChild)setSelectedChild(null);else setTaskSheetOpen(open);}}>{selectedChild?<><button type="button" onClick={()=>setSelectedChild(null)}>‹ {uiText('返回子对话列表')}</button><ClaudeChildReader key={selectedChild.sessionId} standalone node={state.taskTree.nodes.find(n=>n.sessionId===selectedChild.sessionId)??selectedChild} tree={state.taskTree} api={api} threadId={original.threadId} generation={state.watch.generation}/></>:<ClaudeTaskStatus expanded tree={state.taskTree} api={api} threadId={original.threadId} generation={state.watch.generation} notificationsEnabled={state.childResultsEnabled??false} onNotificationsChanged={refresh} onOpenChild={node=>void openChildConversation(node)}/>}</ChatSheet>}
  <ChatSheet title={uiText("添加附件")} open={attachmentOpen} onOpenChange={setAttachmentOpen}><div className="v4-chat-sheet-list"><button type="button" onClick={()=>{photoInput.current?.click();setAttachmentOpen(false);}}>{uiText("照片或截图")}</button><button type="button" onClick={()=>{fileInput.current?.click();setAttachmentOpen(false);}}>{uiText("选择文件")}</button></div><p className="v4-meta">{uiText(pi?"Pi 支持 PNG 和 JPEG 图片，最多 4 张，每张 10 MB。发送时才交给这个对话。":"最多 4 个附件，每个 10 MB，总计 40 MB。上传后可移除，发送时才交给这个对话。")}</p></ChatSheet>
  <ChatSheet title={uiText("对话选项")} open={optionsOpen} onOpenChange={setOptionsOpen}><fieldset className="v4-chat-options"><legend>{uiText("本次回复设置")}</legend>{pi&&state?.piTask?.settings&&<p>{state.piTask.settings.provider} · {state.piTask.settings.model} · {state.piTask.settings.thinking}<small> · {uiText(state.piTask.selectionSource==='DEFAULT'?'创建时默认':'明确选择')}</small></p>}<label>{uiText("模型")}<select aria-label={uiText("模型")} disabled={claude||pi} value={draft.model} onChange={e=>setDraft(d=>({...d,model:e.target.value,effort:""}))}><option value="">{uiText("沿用原对话")}</option>{models?.map(m=><option key={m.id} value={m.id}>{m.displayName||m.model||m.id}</option>)}</select></label><label>{uiText("思考强度")}<select aria-label={uiText("思考强度")} value={draft.effort} disabled={claude||pi||!draft.model} onChange={e=>setDraft(d=>({...d,effort:e.target.value}))}><option value="">{uiText("沿用原对话")}</option>{models?.find(m=>m.id===draft.model)?.supportedReasoningEfforts.map(e=><option key={e.reasoningEffort} value={e.reasoningEffort}>{e.reasoningEffort}</option>)}</select></label>{queueNext&&<label>{uiText("运行中跟进")}<select aria-label={uiText("运行中跟进方式")} value={followupMode} onChange={e=>setFollowupMode(e.target.value as "QUEUE"|"STEER")}><option value="STEER" disabled={!controlTurn}>{uiText("调整当前任务")}</option><option value="QUEUE">{uiText("完成后发送")}</option></select></label>}<p className="v4-meta">{uiText(pi?"Pi 沿用创建时核对过的模型与推理强度。如需更换，请让管理对话新建任务。":"权限沿用本机设置；具体请求到达时确认。调整当前任务时沿用当前模型。")}</p></fieldset></ChatSheet>
  <ChatSheet title={uiText("权限与执行请求")} open={permissionsOpen} onOpenChange={setPermissionsOpen}><p>{uiText(pi?"Pi 使用管理助手分配的工作范围。你在电脑端授权一次，管理助手随后为任务选择目录和能力。":claude?"权限沿用原 Claude Code 设置。请在原对话处理权限请求。":"权限沿用这台电脑上的 Codex 设置。")}</p>{state?.requests.length?state.requests.filter(r=>r.kind!=="USER_INPUT").map(r=><RequestCard key={`${r.requestId}:${r.revision}`} request={r} disabled={busy} respond={extra=>act(async()=>{await command("RESPOND",{requestId:r.requestId,input:extra});await refresh();})}/>):<p className="v4-meta">{uiText(pi?"目前没有待确认的 Pi 执行请求。":"目前没有这个 Router 接收到的待确认请求。原 Codex 正在运行的任务，请在原对话处理请求。")}</p>}</ChatSheet>
  <ChatSheet title={uiText("对话信息")} open={infoOpen} onOpenChange={setInfoOpen}>{controls&&!pi&&<div className="v4-chat-sheet-list v6-watch-controls"><button type="button" disabled={busy} onClick={()=>void act(async()=>{await controls.toggle();setInfoOpen(false);})}>{controls.enabled?uiText("暂停监听"):uiText("恢复监听")}</button><button type="button" className="v6-danger" disabled={busy} onClick={()=>void act(async()=>{await controls.remove();})}>{uiText("停止监听")}</button></div>}{pi&&state?.piTask?.loaded&&<div className="v4-chat-sheet-list"><button type="button" disabled={busy} onClick={()=>void act(async()=>{await command('STOP',{turnId:state.piTask!.executionEpoch});setInfoOpen(false);await refresh(true);})}>{uiText(active?'停止并关闭 Pi 会话':'关闭 Pi 进程')}</button><p className="v4-meta">{uiText('已保存的消息会保留。需要继续时，恢复原 Pi 会话即可。')}</p></div>}{pi&&state?.piTask?.execution&&<section className="pi-execution-info"><h3>{uiText('任务工作范围')}</h3><p>{uiText(state.piTask.loaded?(state.piTask.execution.process.administrator?'管理员执行':'普通权限执行'):(state.piTask.execution.process.administrator?'上次执行：管理员':'上次执行：普通权限'))}</p><p>{uiText(state.piTask.execution.access.writable?'可读取与修改':'只读')} · {uiText(state.piTask.execution.access.terminal?'可执行任务命令':'文件工具')}</p><ul>{state.piTask.execution.access.roots.map(root=><li key={root}><code>{root}</code></li>)}</ul><details><summary>{uiText(!state.piTask.execution.access.inheritContext?'未启用规则与 skills 继承':state.piTask.execution.contextApplied?'已载入的规则与可用技能':'规则与 skills 来源')}</summary><p>{uiText('规则文件')}</p><ul>{(state.piTask.execution.contextApplied?state.piTask.execution.loadedResources?.instructions??state.piTask.execution.resources.instructions:state.piTask.execution.resources.instructions).map(file=><li key={file.path}><code>{file.path}</code></li>)}</ul><p>{uiText('可用 skills 目录（按任务需要读取）')}</p><ul>{state.piTask.execution.resources.skillRoots.map(root=><li key={root}><code>{root}</code></li>)}</ul></details></section>}<p>{original.label}</p><p>{original.cwd}</p><code>{original.threadId}</code><p className="v4-meta">{controls?uiText("停止监听只是不再跟踪，原对话不受影响。回复会发到这个原对话。"):uiText("回复会发到这个原对话。")}</p></ChatSheet>
 </section>;
}
