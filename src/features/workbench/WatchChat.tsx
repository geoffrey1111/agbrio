import {CodexGoalController} from "./CodexGoalController";
import {t as uiText,useLanguage,getLanguage} from "../../i18n";
import {chatTimeline,chronologicalHistoryPage,mergeHistory} from "./chatTimeline";
import {useBackLayer} from "./navigationHistory";
import {recordSheetEvidence} from "./displayDiagnostics";
import {MarkdownMessage} from "../codex/MarkdownMessage";
import {messageMedia} from "../codex/messageMedia";
import {useMemo} from "react";
import { useEffect, useRef, useState } from "react";
import {ArrowUp,ChevronLeft,Copy,Ellipsis,Plus,ShieldCheck,SlidersHorizontal,X,Square,LoaderCircle} from "lucide-react";
import * as Dialog from "@radix-ui/react-dialog";
import type {ReactNode} from "react";
import type { CodexWatch, WatchEvent } from "./CodexNotifications";
import type { ChatHistory, ChatMessage, ChatModel, ReplyOptions, WatchChatApi, WatchChatState, WatchReply } from "./watchChatApi";
import {CodexRequestCard as RequestCard} from "./CodexRequestCard";

const states: Record<string, string> = { get IDLE(){return uiText("尚无任务");}, get RUNNING(){return uiText("正在执行");}, get ACTION_REQUIRED(){return uiText("需要你确认或回答");}, get RESULT_READY(){return uiText("结果已到达");}, get RESULT_PENDING(){return uiText("正在读取结果");}, get FAILED(){return uiText("执行失败");}, get INTERRUPTED(){return uiText("已停止");}, get INCOMPLETE(){return uiText("执行状态待确认");}, get UNKNOWN(){return uiText("状态待确认");} };
const replyStates: Record<string, string> = { get QUEUED(){return uiText("已排队");}, get SENDING(){return uiText("正在发送");}, get SENT(){return uiText("已发送");}, get UNKNOWN(){return uiText("送达尚未确认");}, get ACKNOWLEDGED(){return uiText("已检查，送达状态仍未确定");}, get CANCELLED(){return uiText("已取消排队");}, get FAILED(){return uiText("未发送成功");} };
const key = (id: string) => `aiwr-watch-draft:${id}`;
type FileRef = { id: string; name: string; size: number };
type Draft = { text: string; files: FileRef[]; model: string; effort: string };
type ObservedPublicMessage=ChatMessage&{seenAt:number};
function loadPublicMessages(id:string):ObservedPublicMessage[]{try{const v=JSON.parse(sessionStorage.getItem(`aiwr-watch-public:${id}`)??"[]");return Array.isArray(v)?v.filter(m=>typeof m.id==="string"&&typeof m.turnId==="string"&&typeof m.text==="string"&&m.role==="assistant"&&typeof m.seenAt==="number").slice(-20):[];}catch{return [];}}
function loadDraft(id: string): Draft { try { const v = JSON.parse(sessionStorage.getItem(key(id)) ?? "null"); if (v && typeof v.text === "string" && Array.isArray(v.files)) return { ...v, model: v.model ?? "", effort: v.effort ?? "" }; } catch { /* private/session storage unavailable */ } return { text: "", files: [], model: "", effort: "" }; }
type PendingReply={id:string;text:string;options:ReplyOptions;mode?:WatchReply["mode"];createdAt?:number;sourceSequence?:number|null;expectedTurnId?:string|null;files?:FileRef[];composerCleared?:boolean};
function loadReceipt(id: string): PendingReply|null {try{return JSON.parse(sessionStorage.getItem(`${key(id)}:pending`)??"null");}catch{return null;}}
function ChatSheet({title,open,onOpenChange,children}:{title:string;open:boolean;onOpenChange:(open:boolean)=>void;children:ReactNode}){
 useLanguage();useEffect(()=>{if(!open)return;const timer=setTimeout(()=>recordSheetEvidence(title),300);return()=>clearTimeout(timer);},[open,title]);return <Dialog.Root open={open} onOpenChange={onOpenChange}><Dialog.Portal><Dialog.Overlay className="v4-chat-sheet-overlay"/><div className="v4-chat-sheet-frame"><Dialog.Content className="v4-chat-sheet" aria-describedby={undefined}><header><Dialog.Title>{uiText(title)}</Dialog.Title><Dialog.Close asChild><button type="button" aria-label={uiText("关闭{0}", title)}><X size={24} aria-hidden="true"/></button></Dialog.Close></header><div className="v4-chat-sheet-body">{children}</div></Dialog.Content></div></Dialog.Portal></Dialog.Root>;}
export function chatError(error: unknown): string {
 const text = String(error);
 const messages: Record<string, string> = { PUBLIC_CHAT_READ_UNAVAILABLE: uiText("暂时无法读取对话历史，当前通知和草稿已保留。"), REPLY_TARGET_ALREADY_RUNNING: uiText("这条对话正在执行。请排队，或调整当前任务。"), REPLY_TURN_CHANGED_REFRESH: uiText("原对话已有变化。请刷新状态后再决定是否发送。"), REPLY_TARGET_CHANGED_REFRESH: uiText("项目或监听信息已变化。请重新打开原对话。"), REPLY_PREVIOUS_PENDING_CHECK_FIRST: uiText("已有排队或送达未确认的回复。请先查看它的状态。"), REPLY_REQUEST_EXPIRED: uiText("这条执行请求已失效，请查看当前请求。"), REPLY_ATTACHMENT_TOO_LARGE: uiText("单个附件最多 8 MB。"), REPLY_ATTACHMENT_LIMIT: uiText("最多选择 4 个附件，总大小不超过 16 MB。"), REPLY_MODEL_UNAVAILABLE: uiText("所选模型目前不可用，请在对话选项中重新选择。"), REPLY_EFFORT_UNAVAILABLE: uiText("当前模型不支持这项思考强度。"), REPLY_ATTACHMENT_CHANGED: uiText("附件在上传后已变化，请重新选择。"), BRIDGE_TARGET_GOAL_ACTIVE: uiText("这条对话的 Goal 仍在执行。请先在原 Codex 中暂停 Goal，或调整当前任务。") };
 if(text.includes("REPLY_EXTERNAL_STATE_UNCONFIRMED"))return uiText("原 Codex 的任务是否结束尚未确认。请先在原对话确认完成，再刷新状态。");
 if(text.includes("WATCH_EVENT_NOT_FOUND"))return uiText("这条通知已清理，请从监听对话打开当前内容后再回复。草稿已保留。");
 for (const [code, copy] of Object.entries(messages)) if (text.includes(code)) return copy;
 return text.includes("disconnected") || text.includes("Failed to fetch") || text.includes("NetworkError") ? uiText("暂时连不上这台电脑。草稿已保留，恢复连接后再发送。") : text;
}
export function WatchChat({ original, api, onBack,backLabel=uiText("最近通知") }: { original: WatchEvent | CodexWatch; api: WatchChatApi; onBack: () => void;backLabel?:string }) {
 useLanguage();
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
 useEffect(()=>{const text=JSON.stringify(publicMessages);if(text.length<2_000_000)try{sessionStorage.setItem(`aiwr-watch-public:${original.threadId}`,text);}catch{/* full messages still retained in memory and native public history */}},[publicMessages,original.threadId]);
 const [state, setState] = useState<WatchChatState | null>(()=>{const cached=api.cachedState?.(original.threadId);return cached?.watch.cwd===original.cwd?cached:null;}), [error, setError] = useState<string | null>(null), [notice, setNotice] = useState<string | null>(null), [busy, setBusy] = useState(false);
 // Normal sends report their state on the message; transient feedback never requires acknowledgement.
 useEffect(()=>{if(!notice)return;const timer=setTimeout(()=>setNotice(null),5000);return()=>clearTimeout(timer);},[notice]);
 const media=useMemo(()=>messageMedia({kind:"WATCH",threadId:original.threadId,sequence:"sequence" in original?original.sequence:null,turnId:original.snapshot.turnId,itemId:original.snapshot.itemId}),[original.threadId,"sequence" in original?original.sequence:null,original.snapshot.turnId,original.snapshot.itemId]);
 const [optionsOpen, setOptionsOpen] = useState(false), [models, setModels] = useState<ChatModel[] | null>(null), [history, setHistory] = useState<ChatMessage[]>(()=>api.cachedHistory?.(original.threadId)?.messages??[]), [historyCursor, setHistoryCursor] = useState<string | null>(()=>api.cachedHistory?.(original.threadId)?.nextCursor??null), [historyLoaded, setHistoryLoaded] = useState(()=>Boolean(api.cachedHistory?.(original.threadId)));
 const [attachmentOpen,setAttachmentOpen]=useState(false),[permissionsOpen,setPermissionsOpen]=useState(false),[infoOpen,setInfoOpen]=useState(false),[followupMode,setFollowupMode]=useState<"QUEUE"|"STEER">("QUEUE");
 const mounted = useRef(true), loading = useRef(false), actionRunning = useRef(false), reader = useRef<HTMLDivElement>(null), input = useRef<HTMLTextAreaElement>(null), fileInput = useRef<HTMLInputElement>(null),photoInput=useRef<HTMLInputElement>(null), requestSequence = useRef(0);
 const heading=useRef<HTMLHeadingElement>(null);
 useBackLayer(attachmentOpen,()=>setAttachmentOpen(false));
 useBackLayer(optionsOpen,()=>setOptionsOpen(false));
 useBackLayer(permissionsOpen,()=>setPermissionsOpen(false));
 useBackLayer(infoOpen,()=>setInfoOpen(false));
 const shell=useRef<HTMLElement>(null),syncViewport=useRef<()=>void>(()=>{});
 useEffect(()=>{let width=window.innerWidth,baseline=window.visualViewport?.height??window.innerHeight;const update=()=>{const viewport=window.visualViewport;const height=viewport?.height??window.innerHeight;if(window.innerWidth!==width){width=window.innerWidth;baseline=height;}baseline=Math.max(baseline,height);if((width<=600||window.matchMedia?.("(hover:none) and (pointer:coarse) and (max-width:1100px)")?.matches)&&shell.current){shell.current.style.setProperty("--aiwr-chat-height",`${height}px`);shell.current.dataset.keyboard=String(baseline-height>100&&document.activeElement?.tagName==="TEXTAREA");}else if(shell.current){shell.current.style.removeProperty("--aiwr-chat-height");shell.current.dataset.keyboard="false";}};syncViewport.current=update;update();window.addEventListener("resize",update);window.visualViewport?.addEventListener("resize",update);window.visualViewport?.addEventListener("scroll",update);return()=>{window.removeEventListener("resize",update);window.visualViewport?.removeEventListener("resize",update);window.visualViewport?.removeEventListener("scroll",update);};},[]);
 useEffect(()=>{heading.current?.focus({preventScroll:true});},[]);
 const pending = state?.replies.find(r => ["QUEUED", "SENDING", "UNKNOWN"].includes(r.status));
 const blocked=Boolean(pending||unconfirmed);
 const active = Boolean(state?.ownedTurnId || state?.externalBusy || state?.watch.snapshot.state === "RUNNING");
 const waitingExternal=Boolean(state&&["INCOMPLETE","UNKNOWN","RESULT_PENDING"].includes(state.watch.snapshot.state));
 const queueNext=active||waitingExternal;
 const pendingRequests=state?.requests.filter(r=>!r.responseSent)??[];
 const controlTurn=state?.controllableTurnId??state?.ownedTurnId;
 const nextMode=queueNext?(controlTurn?followupMode:"QUEUE"):"SEND";
 useEffect(()=>{if(window.innerWidth<=600&&input.current){input.current.style.height="54px";input.current.style.height=`${Math.min(144,Math.max(54,input.current.scrollHeight))}px`;}},[draft.text]);
 useEffect(() => { try { sessionStorage.setItem(key(original.threadId), JSON.stringify(draftRef.current)); } catch { /* memory state still preserves current input */ } }, [draft, original.threadId]);
 async function refresh() { if (loading.current) return; loading.current = true; const seq = ++requestSequence.current; try { const next = await api.state(original.threadId); if (next.watch.threadId !== original.threadId || next.watch.cwd !== original.cwd) throw new Error("REPLY_TARGET_CHANGED_REFRESH"); if (mounted.current && seq === requestSequence.current) { setState(next);const snap=next.watch.snapshot;if(next.publicMessages?.length){setPublicMessages(old=>{const merged=new Map(old.map(m=>[JSON.stringify([m.turnId,m.id]),m]));for(const m of next.publicMessages!){if(m.role!=="assistant"||!m.id||!m.turnId)continue;const k=JSON.stringify([m.turnId,m.id]);merged.set(k,{...m,seenAt:merged.get(k)?.seenAt??Date.now()});}return mergeHistory(old,next.publicMessages!).map(m=>({...m,seenAt:merged.get(JSON.stringify([m.turnId,m.id]))?.seenAt??Date.now()})).slice(-40);});}if(snap.turnId&&snap.itemId&&snap.text){setPublicMessages(old=>{const prior=old.find(m=>m.turnId===snap.turnId&&m.id===snap.itemId);const message:ObservedPublicMessage={id:snap.itemId!,turnId:snap.turnId!,role:"assistant",text:snap.text,seenAt:prior?.seenAt??Date.now()};return [...old.filter(m=>m.turnId!==message.turnId||m.id!==message.id),message].slice(-20);});}setError(null); } } catch (e) { if (mounted.current && seq === requestSequence.current) setError(chatError(e)); } finally { loading.current = false; } }
 useEffect(() => { mounted.current = true; const unsubscribe=api.subscribe?.(original.threadId,()=>{const next=api.cachedState?.(original.threadId);if(next?.watch.cwd===original.cwd&&mounted.current)setState(next);const h=api.cachedHistory?.(original.threadId);if(h&&mounted.current){setHistory(old=>mergeHistory(old,chronologicalHistoryPage(h.messages)));setHistoryCursor(h.nextCursor);setHistoryLoaded(true);}});void refresh();const wake=()=>{if(document.visibilityState!=="hidden")void refresh();};const timer=setInterval(wake,5000);document.addEventListener("visibilitychange",wake);window.addEventListener("online",wake);return()=>{mounted.current=false;requestSequence.current++;clearInterval(timer);unsubscribe?.();document.removeEventListener("visibilitychange",wake);window.removeEventListener("online",wake);}; }, [api, original.threadId]);
 async function act(work: () => Promise<void>) { if (actionRunning.current) return; actionRunning.current = true; setBusy(true); setError(null); try { await work(); } catch (e) { if (mounted.current) setError(chatError(e)); } finally { actionRunning.current = false; if (mounted.current) setBusy(false); } }
 function command<T>(action: string, extra: object = {}) { return api.command<T>({ action, threadId: original.threadId, ...extra }); }
 function clearReceipt(){setUnconfirmed(null);try{sessionStorage.removeItem(`${key(original.threadId)}:pending`);}catch{/* no credentials stored */}}
 async function checkReceipt(){if(!unconfirmed)return;const r=await command<WatchReply|null>("RECEIPT",{id:unconfirmed.id});if(!mounted.current)return;if(r&&(r.id!==unconfirmed.id||r.threadId!==original.threadId))throw Error("REPLY_TARGET_CHANGED_REFRESH");if(r&&["SENT","QUEUED"].includes(r.status)){const sent=unconfirmed;if(!sent.composerCleared)setDraft(d=>d.text===sent.text&&JSON.stringify(d.files.map(f=>f.id))===JSON.stringify(sent.options.attachments)?{...d,text:"",files:[]}:d);clearReceipt();setNotice(null);}else if(r&&r.status==="ACKNOWLEDGED"){clearReceipt();setNotice(uiText("这条回复已结束送达检查，送达状态仍未确定，可以输入新的要求。"));}else if(r&&["FAILED","CANCELLED"].includes(r.status)){restoreAttempt(unconfirmed);clearReceipt();setNotice(uiText("已确认这次没有发送。草稿保留，可检查后重新发送。"));}else setNotice(uiText("送达尚未确认。不会自动重发；请查看原对话或取消尚未发送的尝试。"));await refresh();}
 async function acknowledgeUnknown(r:WatchReply){await command("ACKNOWLEDGE_UNKNOWN",{id:r.id,confirmed:true});currentAttemptIds.current.add(r.id);if(unconfirmed?.id===r.id)clearReceipt();if(!unconfirmed?.composerCleared)setDraft(d=>d.text===r.text?{...d,text:"",files:[]}:d);setNotice(uiText("已结束这条回复的送达检查，原文仍保留。可以输入新的要求。"));await refresh();input.current?.focus();}
 async function send(mode: "SEND" | "QUEUE" | "STEER") {
  const submitted=draftRef.current;
  if (!state || !submitted.text.trim() || blocked) return;
  const options: ReplyOptions = { model: mode === "STEER" ? null : submitted.model || null, effort: mode === "STEER" ? null : submitted.effort || null, attachments: submitted.files.map(f => f.id) };
  const request = { id: crypto.randomUUID(), generation: state.watch.generation, sourceSequence: "sequence" in original ? original.sequence : null, expectedTurnId: mode === "STEER" ? controlTurn : state.watch.snapshot.turnId, mode, text: submitted.text, options };
  currentAttemptIds.current.add(request.id);
  // Retain the exact id after an ambiguous HTTP outcome; never mint a retry.
  const receiptKey = `${key(original.threadId)}:pending`;
  const attempt={...request,createdAt:Date.now(),files:submitted.files,composerCleared:true};
  try { sessionStorage.setItem(receiptKey, JSON.stringify(attempt)); } catch { /* current request remains in memory */ }
  setUnconfirmed(attempt);
  followLatest.current=true;
  setDraft({...submitted,text:"",files:[]});
  input.current?.focus({preventScroll:true});
  let reply: WatchReply;
  try { reply = await command<WatchReply>("SEND", request); } catch (e) { if (mounted.current) { setError(chatError(e)); setNotice(uiText("回复是否送达尚未确认。请检查发送记录；草稿已保留，不会自动重发。")); await refresh(); } return; }
  if (!mounted.current) return;
  if(reply.id!==request.id||reply.threadId!==original.threadId)throw Error("REPLY_TARGET_CHANGED_REFRESH");
  setState(current=>current?{...current,replies:[reply,...current.replies.filter(r=>r.id!==reply.id)]}:current);
  if (reply.status === "SENT" || reply.status === "QUEUED") { clearReceipt(); setNotice(null); }
  else if(["FAILED","CANCELLED"].includes(reply.status)){restoreAttempt(attempt);clearReceipt();setNotice(uiText("已确认这次没有发送。草稿保留，可检查后重新发送。"));}
  else setNotice(uiText("回复送达尚未确认。请检查原对话，避免重复发送。"));
  await refresh();
 }
 async function upload(selected: FileList | null) {
  if (!selected?.length) return; const list = Array.from(selected);
  if (draft.files.length + list.length > 4 || list.some(f => f.size > 8 * 1024 * 1024) || list.reduce((n, f) => n + f.size, draft.files.reduce((n, f) => n + f.size, 0)) > 16 * 1024 * 1024) throw new Error("REPLY_ATTACHMENT_LIMIT");
  for (const f of list) { const data = await new Promise<string>((resolve, reject) => { const r = new FileReader(); r.onerror = () => reject(new Error(uiText("无法读取所选文件"))); r.onload = () => resolve(String(r.result).split(",")[1]); r.readAsDataURL(f); }); const ref = await command<FileRef>("UPLOAD", { name: f.name, data }); if (mounted.current) setDraft(d => ({ ...d, files: [...d.files, ref] })); }
 }
 const latest = state?.watch.snapshot;
 const sourceMessage={id:original.snapshot.itemId??'notification',turnId:original.snapshot.turnId??'notification',role:'assistant' as const,text:original.snapshot.text||uiText("暂时没有可读取的公开消息。"),seenAt:('observedAt' in original?original.observedAt:original.checkedAt)??Date.now()};
 const currentMessage=latest?.turnId&&latest?.itemId&&latest.text?{id:latest.itemId,turnId:latest.turnId,role:'assistant' as const,text:latest.text}:undefined;
 const localPending:WatchReply|undefined=unconfirmed&&!state?.replies.some(r=>r.id===unconfirmed.id)?{id:unconfirmed.id,threadId:original.threadId,sourceSequence:unconfirmed.sourceSequence??null,expectedTurnId:unconfirmed.expectedTurnId??null,mode:unconfirmed.mode??"SEND",text:unconfirmed.text,options:unconfirmed.options,status:busy?"SENDING":"UNKNOWN",turnId:null,errorCode:null,createdAt:unconfirmed.createdAt??Date.now()}:undefined;
 const timeline=chatTimeline(history,publicMessages,[...(state?.replies??[]),...(localPending?[localPending]:[])],sourceMessage,historyLoaded,currentMessage,"sequence" in original&&latest?.turnId!==sourceMessage.turnId,currentAttemptIds.current);
 const historyReading=useRef(false),historyRevision=useRef(''),followLatest=useRef(true),initialScroll=useRef(true);
 async function readHistory(cursor:string|null=null){
  if(historyReading.current)return;historyReading.current=true;
  const scroll=reader.current,previousHeight=scroll?.scrollHeight??0,previousTop=scroll?.scrollTop??0;
  try{const v=await command<ChatHistory>('HISTORY',{cursor});if(!mounted.current||!v||!Array.isArray(v.messages))return;
   const page=chronologicalHistoryPage(v.messages);
   setHistory(old=>cursor?mergeHistory(page,old):mergeHistory(old,page));setHistoryCursor(v.nextCursor);setHistoryLoaded(true);
   if(cursor)requestAnimationFrame(()=>{if(scroll)scroll.scrollTop=previousTop+scroll.scrollHeight-previousHeight;});
  }catch(e){if(mounted.current)setError(chatError(e));}finally{historyReading.current=false;}
 }
 useEffect(()=>{const snap=state?.watch.snapshot??original.snapshot;
  const revision=JSON.stringify([snap.turnId,snap.itemId,snap.state]);
  if(historyRevision.current===revision||historyReading.current)return;
  const previous=historyRevision.current?JSON.parse(historyRevision.current):[];
  if(previous[0]===snap.turnId&&['RUNNING','RESULT_PENDING'].includes(snap.state))return;
  historyRevision.current=revision;void readHistory();
 },[state?.watch.snapshot.turnId,state?.watch.snapshot.itemId,state?.watch.snapshot.state]);
 useEffect(()=>{if(!initialScroll.current&&!followLatest.current)return;const scroll=reader.current;if(scroll){scroll.scrollTop=scroll.scrollHeight;initialScroll.current=false;}},[historyLoaded,history,publicMessages,state?.replies,unconfirmed]);
 return <section ref={shell} className="v4-watch-chat" aria-label={uiText("通知原对话与回复")} onFocus={()=>syncViewport.current()} onBlur={()=>queueMicrotask(()=>syncViewport.current())}>
  <header className="v4-chat-header"><button type="button" className="v4-chat-back" aria-label={`‹ ${backLabel}`} onClick={onBack}><ChevronLeft size={24} aria-hidden="true"/><span className="v4-chat-desktop-label">{backLabel}</span></button><div className="v4-chat-identity"><h2 ref={heading} tabIndex={-1}>{original.label}</h2><p className="v4-meta">{original.cwd.split(/[\\/]/).filter(Boolean).at(-1)} · {state?.host ?? uiText("这台电脑")}</p></div><button type="button" className="v4-chat-more" aria-label={uiText("对话信息")} onClick={()=>setInfoOpen(true)}><Ellipsis size={24} aria-hidden="true"/></button></header>
  <div className="v4-chat-live" role="status" aria-live="polite" data-active={active}>{active&&<LoaderCircle size={14} aria-hidden="true"/>}<span>{pendingRequests.length?uiText("等待确认"):error?uiText("连接中断"):active?(state?.activity==="THINKING"?uiText("正在思考"):uiText("正在执行")):(states[latest?.state??original.snapshot.state]??uiText("状态待确认"))}</span>{pendingRequests.some(r=>r.kind!=="USER_INPUT")&&<button type="button" onClick={()=>setPermissionsOpen(true)}>{uiText("查看")}</button>}</div>
  <div className="v4-chat-reader" ref={reader} onScroll={()=>{const scroll=reader.current;if(scroll)followLatest.current=scroll.scrollHeight-scroll.scrollTop-scroll.clientHeight<80;}}>
   {error && <p role="alert" className="v4-chat-alert">{uiText(error)}<button type="button" disabled={busy} onClick={() => void act(refresh)}>{uiText("重新连接")}</button></p>}

   <div className="v4-chat-history">{historyCursor&&<button type="button" disabled={busy||historyReading.current} onClick={()=>void readHistory(historyCursor)}>{uiText("加载更早消息")}</button>}{!historyLoaded&&<button type="button" disabled={busy||historyReading.current} onClick={()=>void readHistory()}>{uiText("读取对话消息")}</button>}</div>
   {timeline.filter(m=>m.id!=="notification"||!historyLoaded||!history.length).map(m=>{const r=m.receipt;return <article key={`${m.turnId}:${m.id}`} className="v4-chat-message" data-role={m.role} data-message-kind={m.id.startsWith("receipt:")?"receipt":"native"} aria-label={m.source?uiText("完整通知内容"):undefined}>
    <p className="v4-meta">{m.role==='user'?uiText("你{0}", r?' · '+replyStates[r.status]:''):m.source&&'sequence' in original?uiText("Codex · 通知原文"):latest?.itemId===m.id?uiText("Codex · 最新公开回复"):'Codex'}</p>
    <MarkdownMessage text={m.text} media={m.source?media:messageMedia({kind:'WATCH',threadId:original.threadId,turnId:m.turnId,itemId:m.id})}/>
    {r?.status==='QUEUED'&&<button type="button" disabled={busy} onClick={()=>void act(async()=>{await command('CANCEL',{id:r.id});await refresh();})}>{uiText("取消排队")}</button>}
    {r?.status==='UNKNOWN'&&r.id!==localPending?.id&&<><p role="status">{uiText("回复可能已发出。请先检查原对话，不会自动重发。")}</p><button type="button" disabled={busy} onClick={()=>void act(()=>acknowledgeUnknown(r))}>{uiText("我已检查原对话，继续输入新回复")}</button></>}
    {r?.status==='FAILED'&&<p role="status">{uiText("这条回复没有完成发送。请检查当前对话后重新输入。")}</p>}
    {m.source&&<div className="v4-chat-message-tools"><button type="button" aria-label={uiText("复制原文")} onClick={()=>void act(async()=>{await navigator.clipboard.writeText(m.text);setNotice(uiText("原文已复制。"));})}><Copy size={24} aria-hidden="true"/></button>{'sequence' in original&&<time className="v4-meta">{new Date(sourceMessage.seenAt).toLocaleString(getLanguage())}</time>}</div>}
   </article>;})}
   {state?.requests.filter(r=>r.kind!=="USER_INPUT").map(r => <RequestCard key={`${r.requestId}:${r.revision}`} request={r} disabled={busy} respond={extra => act(async () => { await command("RESPOND", { requestId: r.requestId, input: extra }); await refresh(); })} />)}
   {unconfirmed&&!busy&&<div className="v4-chat-alert" role="status"><p>{uiText("上一次回复仍需确认，草稿已保留。")}</p><div className="v4-chat-actions"><button type="button" disabled={busy} onClick={()=>void act(checkReceipt)}>{uiText("检查发送记录")}</button><button type="button" disabled={busy} onClick={()=>void act(async()=>{const v=await command<{cancelled:boolean}>("ABANDON",{id:unconfirmed.id});if(v.cancelled){restoreAttempt(unconfirmed);clearReceipt();setNotice(uiText("已取消尚未发送的尝试。草稿保留。"));}else setNotice(uiText("这条回复已进入发送，请查看原对话，不会重复发送。"));await refresh();})}>{uiText("取消尚未发送的尝试")}</button></div></div>}
  </div>
  <div className="v4-chat-dock">
   {notice && <p role="status" className="v4-chat-feedback">{uiText(notice)}</p>}
   {state?.goal&&<CodexGoalController goal={state.goal} controls={state.goalControls} diagnostic={state.latestTurn} api={api} checkedAt={state.checkedAt} disabled={busy||blocked} refresh={refresh}/>}
   {state?.requests.filter(r=>r.kind==="USER_INPUT").map(r=><RequestCard key={`${r.requestId}:${r.revision}`} request={r} disabled={busy} respond={extra=>act(async()=>{await command("RESPOND",{requestId:r.requestId,input:extra});await refresh();})}/>)}
   {controlTurn&&<div className="v4-followup-toggle" role="group" aria-label={uiText("运行中跟进方式")}><button type="button" aria-pressed={followupMode==="QUEUE"} disabled={busy} onClick={()=>setFollowupMode("QUEUE")}>{uiText("完成后发送")}</button><button type="button" aria-pressed={followupMode==="STEER"} disabled={busy} onClick={()=>setFollowupMode("STEER")}>{uiText("调整当前任务")}</button></div>}<form className="v4-chat-composer" aria-busy={busy} onSubmit={e => { e.preventDefault(); void act(() => send(nextMode)); }}>
   <label htmlFor="watch-chat-reply">{uiText("回复这个 Codex 对话")}</label><textarea id="watch-chat-reply" ref={input} placeholder={uiText("在 {0} 上工作", state?.host??uiText("这台电脑"))} value={draft.text} maxLength={100000} rows={2} onChange={e => setDraft(d => ({ ...d, text: e.target.value }))} onKeyDown={e => { if ((e.ctrlKey || e.metaKey) && e.key === "Enter" && !e.nativeEvent.isComposing) { e.preventDefault(); if (!busy) void act(() => send(nextMode)); } }} />
   {draft.files.length > 0 && <ul className="v4-chat-files">{draft.files.map(f => <li key={f.id}>{f.name} · {Math.ceil(f.size / 1024)} KB <button type="button" aria-label={uiText("移除 {0}", f.name)} disabled={busy} onClick={() => setDraft(d => ({ ...d, files: d.files.filter(v => v.id !== f.id) }))}>{uiText("移除")}</button></li>)}</ul>}
   <div className="v4-chat-composer-tools"><input ref={fileInput} type="file" multiple hidden onChange={e => { const fs = e.currentTarget.files; void act(() => upload(fs)); e.currentTarget.value = ""; }} /><input ref={photoInput} type="file" accept="image/*" multiple hidden onChange={e=>{const fs=e.currentTarget.files;void act(()=>upload(fs));e.currentTarget.value="";}}/><button type="button" className="v4-chat-icon" aria-label={uiText("＋ 附件")} disabled={busy || draft.files.length >= 4} onClick={() => {if(window.matchMedia?.("(max-width:760px),(hover:none) and (pointer:coarse) and (max-width:1100px)").matches)setAttachmentOpen(true);else fileInput.current?.click();}}><Plus size={24} aria-hidden="true"/><span className="v4-chat-desktop-label">{uiText("附件")}</span></button><button type="button" className="v4-chat-icon v4-chat-permissions" aria-label={uiText("权限与执行请求{0}", pendingRequests.length?uiText("，{0}条待处理", pendingRequests.length):"")} onClick={()=>setPermissionsOpen(true)}><ShieldCheck size={24} aria-hidden="true"/>{pendingRequests.length>0&&<span className="v4-chat-request-count" aria-hidden="true">{pendingRequests.length}</span>}</button><span className="v4-chat-tool-space"/><button type="button" className="v4-chat-icon" aria-label={uiText("对话选项")} aria-expanded={optionsOpen} disabled={busy} onClick={() => { setOptionsOpen(v => !v); if (!models) void act(async () => { const v = await command<{ models: ChatModel[] }>("OPTIONS"); if (mounted.current) setModels(v.models); }); }}><SlidersHorizontal size={24} aria-hidden="true"/><span className="v4-chat-desktop-label">{uiText("对话选项")}</span></button>{controlTurn&&!draft.text.trim()?<button type="button" className="v4-chat-send" aria-label={uiText("停止本次执行")} disabled={busy} onClick={()=>void act(async()=>{await command("STOP",{turnId:controlTurn});setNotice(uiText("已请求停止，等待 Codex 确认。"));await refresh();})}><Square size={24} aria-hidden="true"/></button>:<button type="submit" className="v3-primary v4-chat-send" aria-label={nextMode==="STEER"?uiText("调整当前任务"):queueNext?uiText("排队，完成后发送"):uiText("发送回复")} disabled={busy || !state || !draft.text.trim() || blocked}><ArrowUp size={24} aria-hidden="true"/><span className="v4-chat-desktop-label">{busy?uiText("正在处理…"):nextMode==="STEER"?uiText("调整当前任务"):queueNext?uiText("排队，完成后发送"):uiText("发送回复")}</span></button>}</div>
   {queueNext&&draft.text.trim()&&<p className="v4-meta v4-chat-mode">{nextMode==="STEER"?uiText("调整当前任务"):uiText("当前任务完成后发送")} {uiText("· 在对话选项中选择")}</p>}
   {pending && <p className="v4-meta">{uiText(replyStates[pending.status])} {uiText("· 请先处理这条回复。")}</p>}
  </form><p className="v4-meta v4-chat-dictation">{uiText("手机键盘可直接听写")}</p></div>
  <ChatSheet title={uiText("添加附件")} open={attachmentOpen} onOpenChange={setAttachmentOpen}><div className="v4-chat-sheet-list"><button type="button" onClick={()=>{photoInput.current?.click();setAttachmentOpen(false);}}>{uiText("照片或截图")}</button><button type="button" onClick={()=>{fileInput.current?.click();setAttachmentOpen(false);}}>{uiText("选择文件")}</button></div><p className="v4-meta">{uiText("最多 4 个附件，每个 8 MB，总计 16 MB。上传后可移除，发送时才交给这个对话。")}</p></ChatSheet>
  <ChatSheet title={uiText("对话选项")} open={optionsOpen} onOpenChange={setOptionsOpen}><fieldset className="v4-chat-options"><legend>{uiText("本次回复设置")}</legend><label>{uiText("模型")}<select aria-label={uiText("模型")} value={draft.model} onChange={e=>setDraft(d=>({...d,model:e.target.value,effort:""}))}><option value="">{uiText("沿用原对话")}</option>{models?.map(m=><option key={m.id} value={m.id}>{m.displayName||m.model||m.id}</option>)}</select></label><label>{uiText("思考强度")}<select aria-label={uiText("思考强度")} value={draft.effort} disabled={!draft.model} onChange={e=>setDraft(d=>({...d,effort:e.target.value}))}><option value="">{uiText("沿用原对话")}</option>{models?.find(m=>m.id===draft.model)?.supportedReasoningEfforts.map(e=><option key={e.reasoningEffort} value={e.reasoningEffort}>{e.reasoningEffort}</option>)}</select></label>{controlTurn&&<label>{uiText("运行中跟进")}<select aria-label={uiText("运行中跟进方式")} value={followupMode} onChange={e=>setFollowupMode(e.target.value as "QUEUE"|"STEER")}><option value="QUEUE">{uiText("完成后发送")}</option><option value="STEER">{uiText("调整当前任务")}</option></select></label>}<p className="v4-meta">{uiText("权限沿用本机设置；具体请求到达时确认。调整当前任务时沿用当前模型。")}</p></fieldset></ChatSheet>
  <ChatSheet title={uiText("权限与执行请求")} open={permissionsOpen} onOpenChange={setPermissionsOpen}><p>{uiText("权限沿用这台电脑上的 Codex 设置。")}</p>{state?.requests.length?state.requests.filter(r=>r.kind!=="USER_INPUT").map(r=><RequestCard key={`${r.requestId}:${r.revision}`} request={r} disabled={busy} respond={extra=>act(async()=>{await command("RESPOND",{requestId:r.requestId,input:extra});await refresh();})}/>):<p className="v4-meta">{uiText("目前没有这个 Router 接收到的待确认请求。原 Codex 正在运行的任务，请在原对话处理请求。")}</p>}</ChatSheet>
  <ChatSheet title={uiText("对话信息")} open={infoOpen} onOpenChange={setInfoOpen}><p>{original.label}</p><p>{original.cwd}</p><code>{original.threadId}</code><p className="v4-meta">{uiText("回复会发到这个原对话。")}</p></ChatSheet>
 </section>;
}
