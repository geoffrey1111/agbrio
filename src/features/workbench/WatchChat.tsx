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
import type { MobileCodexRequest } from "../../mobile/api";

const states: Record<string, string> = { IDLE: "尚无任务", RUNNING: "正在执行", ACTION_REQUIRED:"需要你确认或回答", RESULT_READY: "结果已到达", RESULT_PENDING: "正在读取结果", FAILED: "执行失败", INTERRUPTED: "已停止", INCOMPLETE: "执行状态待确认", UNKNOWN: "状态待确认" };
const replyStates: Record<string, string> = { QUEUED: "已排队", SENDING: "正在发送", SENT: "已发送", UNKNOWN: "送达尚未确认", ACKNOWLEDGED:"已检查，送达状态仍未确定", CANCELLED: "已取消排队", FAILED: "未发送成功" };
const key = (id: string) => `aiwr-watch-draft:${id}`;
type FileRef = { id: string; name: string; size: number };
type Draft = { text: string; files: FileRef[]; model: string; effort: string };
type ObservedPublicMessage=ChatMessage&{seenAt:number};
function loadPublicMessages(id:string):ObservedPublicMessage[]{try{const v=JSON.parse(sessionStorage.getItem(`aiwr-watch-public:${id}`)??"[]");return Array.isArray(v)?v.filter(m=>typeof m.id==="string"&&typeof m.turnId==="string"&&typeof m.text==="string"&&m.role==="assistant"&&typeof m.seenAt==="number").slice(-20):[];}catch{return [];}}
function loadDraft(id: string): Draft { try { const v = JSON.parse(sessionStorage.getItem(key(id)) ?? "null"); if (v && typeof v.text === "string" && Array.isArray(v.files)) return { ...v, model: v.model ?? "", effort: v.effort ?? "" }; } catch { /* private/session storage unavailable */ } return { text: "", files: [], model: "", effort: "" }; }
function loadReceipt(id: string): {id:string;text:string;options:ReplyOptions}|null {try{return JSON.parse(sessionStorage.getItem(`${key(id)}:pending`)??"null");}catch{return null;}}
function ChatSheet({title,open,onOpenChange,children}:{title:string;open:boolean;onOpenChange:(open:boolean)=>void;children:ReactNode}){useEffect(()=>{if(!open)return;const timer=setTimeout(()=>recordSheetEvidence(title),300);return()=>clearTimeout(timer);},[open,title]);return <Dialog.Root open={open} onOpenChange={onOpenChange}><Dialog.Portal><Dialog.Overlay className="v4-chat-sheet-overlay"/><div className="v4-chat-sheet-frame"><Dialog.Content className="v4-chat-sheet" aria-describedby={undefined}><header><Dialog.Title>{title}</Dialog.Title><Dialog.Close asChild><button type="button" aria-label={`关闭${title}`}><X size={24} aria-hidden="true"/></button></Dialog.Close></header><div className="v4-chat-sheet-body">{children}</div></Dialog.Content></div></Dialog.Portal></Dialog.Root>;}
export function chatError(error: unknown): string {
 const text = String(error);
 const messages: Record<string, string> = { PUBLIC_CHAT_READ_UNAVAILABLE: "暂时无法读取对话历史，当前通知和草稿已保留。", REPLY_TARGET_ALREADY_RUNNING: "这条对话正在执行。请排队，或调整当前任务。", REPLY_TURN_CHANGED_REFRESH: "原对话已有变化。请刷新状态后再决定是否发送。", REPLY_TARGET_CHANGED_REFRESH: "项目或监听信息已变化。请重新打开原对话。", REPLY_PREVIOUS_PENDING_CHECK_FIRST: "已有排队或送达未确认的回复。请先查看它的状态。", REPLY_REQUEST_EXPIRED: "这条执行请求已失效，请查看当前请求。", REPLY_ATTACHMENT_TOO_LARGE: "单个附件最多 8 MB。", REPLY_ATTACHMENT_LIMIT: "最多选择 4 个附件，总大小不超过 16 MB。", REPLY_MODEL_UNAVAILABLE: "所选模型目前不可用，请在对话选项中重新选择。", REPLY_EFFORT_UNAVAILABLE: "当前模型不支持这项思考强度。", REPLY_ATTACHMENT_CHANGED: "附件在上传后已变化，请重新选择。", BRIDGE_TARGET_GOAL_ACTIVE: "这条对话的 Goal 仍在执行。请先在原 Codex 中暂停 Goal，或调整当前任务。" };
 if(text.includes("REPLY_EXTERNAL_STATE_UNCONFIRMED"))return "原 Codex 的任务是否结束尚未确认。请先在原对话确认完成，再刷新状态。";
 if(text.includes("WATCH_EVENT_NOT_FOUND"))return "这条通知已清理，请从监听对话打开当前内容后再回复。草稿已保留。";
 for (const [code, copy] of Object.entries(messages)) if (text.includes(code)) return copy;
 return text.includes("disconnected") || text.includes("Failed to fetch") || text.includes("NetworkError") ? "暂时连不上这台电脑。草稿已保留，恢复连接后再发送。" : text;
}
function RequestCard({ request, disabled, respond }: { request: MobileCodexRequest; disabled: boolean; respond: (input: object) => Promise<void> }) {
 const answerKey=`aiwr-request-answer:${request.requestId}:${request.revision}`;
 const [answers, setAnswers] = useState<Record<string, string>>(()=>{try{return JSON.parse(sessionStorage.getItem(answerKey)??"{}");}catch{return {};}});
 useEffect(()=>{try{sessionStorage.setItem(answerKey,JSON.stringify(answers));}catch{/* preserve memory state */}},[answerKey,answers]);
 return <section className="v4-chat-request" aria-label="Codex 执行请求"><h3>{request.kind === "USER_INPUT" ? "Codex 需要你的回答" : "Codex 需要你确认"}</h3>{request.reason && <p>{request.reason}</p>}{(request.questions??[]).map(q => <label key={q.id}>{q.label}<textarea value={answers[q.id] ?? ""} placeholder={q.placeholder} required={q.required} onChange={e => setAnswers(old => ({ ...old, [q.id]: e.target.value }))} /></label>)}{request.responseSent ? <p role="status">已答复，等待 Codex 确认。</p> : <div className="v4-chat-actions">{(request.choices??[]).map(c => <button type="button" key={c.id} disabled={disabled} onClick={() => void respond({ revision: request.revision, decision: c.id })}>{c.id === "accept" ? "允许这次请求" : c.id === "decline" ? "拒绝" : "取消"}</button>)}{(request.questions??[]).length > 0 && <button type="button" className="v3-primary" disabled={disabled || (request.questions??[]).some(q => q.required && !answers[q.id]?.trim())} onClick={() => void respond({ revision: request.revision, answers })}>提交回答</button>}</div>}</section>;
}
export function WatchChat({ original, api, onBack,backLabel="最近通知" }: { original: WatchEvent | CodexWatch; api: WatchChatApi; onBack: () => void;backLabel?:string }) {
 const [draft, setDraft] = useState(() => loadDraft(original.threadId));
 const [unconfirmed,setUnconfirmed]=useState(()=>loadReceipt(original.threadId));
 const [publicMessages,setPublicMessages]=useState(()=>loadPublicMessages(original.threadId));
 useEffect(()=>{const text=JSON.stringify(publicMessages);if(text.length<2_000_000)try{sessionStorage.setItem(`aiwr-watch-public:${original.threadId}`,text);}catch{/* full messages still retained in memory and native public history */}},[publicMessages,original.threadId]);
 const [state, setState] = useState<WatchChatState | null>(null), [error, setError] = useState<string | null>(null), [notice, setNotice] = useState<string | null>(null), [busy, setBusy] = useState(false);
 const media=useMemo(()=>messageMedia({kind:"WATCH",threadId:original.threadId,sequence:"sequence" in original?original.sequence:null,turnId:original.snapshot.turnId,itemId:original.snapshot.itemId}),[original.threadId,"sequence" in original?original.sequence:null,original.snapshot.turnId,original.snapshot.itemId]);
 const [optionsOpen, setOptionsOpen] = useState(false), [models, setModels] = useState<ChatModel[] | null>(null), [history, setHistory] = useState<ChatMessage[]>([]), [historyCursor, setHistoryCursor] = useState<string | null>(null), [historyLoaded, setHistoryLoaded] = useState(false);
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
 const nextMode=queueNext?(state?.ownedTurnId?followupMode:"QUEUE"):"SEND";
 useEffect(()=>{if(window.innerWidth<=600&&input.current){input.current.style.height="54px";input.current.style.height=`${Math.min(144,Math.max(54,input.current.scrollHeight))}px`;}},[draft.text]);
 useEffect(() => { try { sessionStorage.setItem(key(original.threadId), JSON.stringify(draft)); } catch { /* memory state still preserves current input */ } }, [draft, original.threadId]);
 async function refresh() { if (loading.current) return; loading.current = true; const seq = ++requestSequence.current; try { const next = await api.state(original.threadId); if (next.watch.threadId !== original.threadId || next.watch.cwd !== original.cwd) throw new Error("REPLY_TARGET_CHANGED_REFRESH"); if (mounted.current && seq === requestSequence.current) { setState(next);const snap=next.watch.snapshot;if(next.publicMessages?.length){setPublicMessages(old=>{const merged=new Map(old.map(m=>[JSON.stringify([m.turnId,m.id]),m]));for(const m of next.publicMessages!){if(m.role!=="assistant"||!m.id||!m.turnId)continue;const k=JSON.stringify([m.turnId,m.id]);merged.set(k,{...m,seenAt:merged.get(k)?.seenAt??Date.now()});}return mergeHistory(old,next.publicMessages!).map(m=>({...m,seenAt:merged.get(JSON.stringify([m.turnId,m.id]))?.seenAt??Date.now()})).slice(-40);});}if(snap.turnId&&snap.itemId&&snap.text){setPublicMessages(old=>{const prior=old.find(m=>m.turnId===snap.turnId&&m.id===snap.itemId);const message:ObservedPublicMessage={id:snap.itemId!,turnId:snap.turnId!,role:"assistant",text:snap.text,seenAt:prior?.seenAt??Date.now()};return [...old.filter(m=>m.turnId!==message.turnId||m.id!==message.id),message].slice(-20);});}setError(null); } } catch (e) { if (mounted.current && seq === requestSequence.current) setError(chatError(e)); } finally { loading.current = false; } }
 useEffect(() => { mounted.current = true; void refresh(); const timer = setInterval(() => void refresh(), 5000); return () => { mounted.current = false; requestSequence.current++; clearInterval(timer); }; }, [api, original.threadId]);
 async function act(work: () => Promise<void>) { if (actionRunning.current) return; actionRunning.current = true; setBusy(true); setError(null); try { await work(); } catch (e) { if (mounted.current) setError(chatError(e)); } finally { actionRunning.current = false; if (mounted.current) setBusy(false); } }
 function command<T>(action: string, extra: object = {}) { return api.command<T>({ action, threadId: original.threadId, ...extra }); }
 function clearReceipt(){setUnconfirmed(null);try{sessionStorage.removeItem(`${key(original.threadId)}:pending`);}catch{/* no credentials stored */}}
 async function checkReceipt(){if(!unconfirmed)return;const r=await command<WatchReply|null>("RECEIPT",{id:unconfirmed.id});if(!mounted.current)return;if(r&&["SENT","QUEUED"].includes(r.status)){const sent=unconfirmed;setDraft(d=>d.text===sent.text&&JSON.stringify(d.files.map(f=>f.id))===JSON.stringify(sent.options.attachments)?{...d,text:"",files:[]}:d);clearReceipt();setNotice(r.status==="SENT"?"已确认回复发到原对话。":"已确认回复排队。");}else if(r&&r.status==="ACKNOWLEDGED"){clearReceipt();setNotice("这条回复已结束送达检查，送达状态仍未确定，可以输入新的要求。");}else if(r&&["FAILED","CANCELLED"].includes(r.status)){clearReceipt();setNotice("已确认这次没有发送。草稿保留，可检查后重新发送。");}else setNotice("送达尚未确认。不会自动重发；请查看原对话或取消尚未发送的尝试。");await refresh();}
 async function acknowledgeUnknown(r:WatchReply){await command("ACKNOWLEDGE_UNKNOWN",{id:r.id,confirmed:true});if(unconfirmed?.id===r.id)clearReceipt();setDraft(d=>d.text===r.text?{...d,text:"",files:[]}:d);setNotice("已结束这条回复的送达检查，原文仍保留。可以输入新的要求。");await refresh();input.current?.focus();}
 async function send(mode: "SEND" | "QUEUE" | "STEER") {
  if (!state || !draft.text.trim() || blocked) return;
  const options: ReplyOptions = { model: mode === "STEER" ? null : draft.model || null, effort: mode === "STEER" ? null : draft.effort || null, attachments: draft.files.map(f => f.id) };
  const request = { id: crypto.randomUUID(), generation: state.watch.generation, sourceSequence: "sequence" in original ? original.sequence : null, expectedTurnId: mode === "STEER" ? state.ownedTurnId : state.watch.snapshot.turnId, mode, text: draft.text, options };
  // Retain the exact id after an ambiguous HTTP outcome; never mint a retry.
  const receiptKey = `${key(original.threadId)}:pending`;
  try { sessionStorage.setItem(receiptKey, JSON.stringify(request)); } catch { /* current request remains in memory */ }
  setUnconfirmed(request);
  let reply: WatchReply;
  try { reply = await command<WatchReply>("SEND", request); } catch (e) { if (mounted.current) { setError(chatError(e)); setNotice("回复是否送达尚未确认。请检查发送记录；草稿已保留，不会自动重发。"); await refresh(); } return; }
  if (!mounted.current) return;
  if (reply.status === "SENT" || reply.status === "QUEUED") { setDraft(d=>d.text===request.text&&JSON.stringify(d.files.map(f=>f.id))===JSON.stringify(request.options.attachments)?{...d,text:"",files:[]}:d); clearReceipt(); setNotice(reply.status === "QUEUED" ? "已排队，当前任务完成后发送。" : "回复已发送到这个原对话。"); }
  else setNotice("回复送达尚未确认。请检查原对话，避免重复发送。");
  await refresh();
 }
 async function upload(selected: FileList | null) {
  if (!selected?.length) return; const list = Array.from(selected);
  if (draft.files.length + list.length > 4 || list.some(f => f.size > 8 * 1024 * 1024) || list.reduce((n, f) => n + f.size, draft.files.reduce((n, f) => n + f.size, 0)) > 16 * 1024 * 1024) throw new Error("REPLY_ATTACHMENT_LIMIT");
  for (const f of list) { const data = await new Promise<string>((resolve, reject) => { const r = new FileReader(); r.onerror = () => reject(new Error("无法读取所选文件")); r.onload = () => resolve(String(r.result).split(",")[1]); r.readAsDataURL(f); }); const ref = await command<FileRef>("UPLOAD", { name: f.name, data }); if (mounted.current) setDraft(d => ({ ...d, files: [...d.files, ref] })); }
 }
 const latest = state?.watch.snapshot;
 const sourceMessage={id:original.snapshot.itemId??'notification',turnId:original.snapshot.turnId??'notification',role:'assistant' as const,text:original.snapshot.text||'暂时没有可读取的公开消息。',seenAt:('observedAt' in original?original.observedAt:original.checkedAt)??Date.now()};
 const currentMessage=latest?.turnId&&latest?.itemId&&latest.text?{id:latest.itemId,turnId:latest.turnId,role:'assistant' as const,text:latest.text}:undefined;
 const timeline=chatTimeline(history,publicMessages,state?.replies??[],sourceMessage,historyLoaded,currentMessage,"sequence" in original&&latest?.turnId!==sourceMessage.turnId);
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
 useEffect(()=>{if(!initialScroll.current&&!followLatest.current)return;const scroll=reader.current;if(scroll){scroll.scrollTop=scroll.scrollHeight;initialScroll.current=false;}},[historyLoaded,history,publicMessages,state?.replies]);
 return <section ref={shell} className="v4-watch-chat" aria-label="通知原对话与回复" onFocus={()=>syncViewport.current()} onBlur={()=>queueMicrotask(()=>syncViewport.current())}>
  <header className="v4-chat-header"><button type="button" className="v4-chat-back" aria-label={`‹ ${backLabel}`} onClick={onBack}><ChevronLeft size={24} aria-hidden="true"/><span className="v4-chat-desktop-label">{backLabel}</span></button><div className="v4-chat-identity"><h2 ref={heading} tabIndex={-1}>{original.label}</h2><p className="v4-meta">{original.cwd.split(/[\\/]/).filter(Boolean).at(-1)} · {state?.host ?? "这台电脑"}</p></div><button type="button" className="v4-chat-more" aria-label="对话信息" onClick={()=>setInfoOpen(true)}><Ellipsis size={24} aria-hidden="true"/></button></header>
  <div className="v4-chat-live" role="status" aria-live="polite" data-active={active}>{active&&<LoaderCircle size={14} aria-hidden="true"/>}<span>{pendingRequests.length?"等待确认":error?"连接中断":active?(state?.activity==="THINKING"?"正在思考":"正在执行"):(states[latest?.state??original.snapshot.state]??"状态待确认")}</span>{pendingRequests.length>0&&<button type="button" onClick={()=>setPermissionsOpen(true)}>查看</button>}</div>
  <div className="v4-chat-reader" ref={reader} onScroll={()=>{const scroll=reader.current;if(scroll)followLatest.current=scroll.scrollHeight-scroll.scrollTop-scroll.clientHeight<80;}}>
   {error && <p role="alert" className="v4-chat-alert">{error}<button type="button" disabled={busy} onClick={() => void act(refresh)}>重新连接</button></p>}

   {state?.goal && <details className="v4-secondary-details"><summary>Goal · {state.goal.status}</summary><p>{state.goal.objective}</p></details>}
   <div className="v4-chat-history">{historyCursor&&<button type="button" disabled={busy||historyReading.current} onClick={()=>void readHistory(historyCursor)}>加载更早消息</button>}{!historyLoaded&&<button type="button" disabled={busy||historyReading.current} onClick={()=>void readHistory()}>读取对话消息</button>}</div>
   {timeline.filter(m=>m.id!=="notification"||!historyLoaded||!history.length).map(m=>{const r=m.receipt;return <article key={`${m.turnId}:${m.id}`} className="v4-chat-message" data-role={m.role} aria-label={m.source?'完整通知内容':undefined}>
    <p className="v4-meta">{m.role==='user'?`你${r?' · '+replyStates[r.status]:''}`:m.source&&'sequence' in original?'Codex · 通知原文':latest?.itemId===m.id?'Codex · 最新公开回复':'Codex'}</p>
    <MarkdownMessage text={m.text} media={m.source?media:messageMedia({kind:'WATCH',threadId:original.threadId,turnId:m.turnId,itemId:m.id})}/>
    {r?.status==='QUEUED'&&<button type="button" disabled={busy} onClick={()=>void act(async()=>{await command('CANCEL',{id:r.id});await refresh();})}>取消排队</button>}
    {r?.status==='UNKNOWN'&&<><p role="status">回复可能已发出。请先检查原对话，不会自动重发。</p><button type="button" disabled={busy} onClick={()=>void act(()=>acknowledgeUnknown(r))}>我已检查原对话，继续输入新回复</button></>}
    {r?.status==='FAILED'&&<p role="status">这条回复没有完成发送。请检查当前对话后重新输入。</p>}
    {m.source&&<div className="v4-chat-message-tools"><button type="button" aria-label="复制原文" onClick={()=>void act(async()=>{await navigator.clipboard.writeText(m.text);setNotice('原文已复制。');})}><Copy size={24} aria-hidden="true"/></button>{'sequence' in original&&<time className="v4-meta">{new Date(sourceMessage.seenAt).toLocaleString()}</time>}</div>}
   </article>;})}
   {state?.requests.map(r => <RequestCard key={`${r.requestId}:${r.revision}`} request={r} disabled={busy} respond={extra => act(async () => { await command("RESPOND", { requestId: r.requestId, input: extra }); await refresh(); })} />)}
   {notice && <p role="status" className="v4-chat-alert">{notice}<button type="button" onClick={() => setNotice(null)}>知道了</button></p>}
   {unconfirmed&&<div className="v4-chat-alert" role="status"><p>上一次回复仍需确认，草稿已保留。</p><div className="v4-chat-actions"><button type="button" disabled={busy} onClick={()=>void act(checkReceipt)}>检查发送记录</button><button type="button" disabled={busy} onClick={()=>void act(async()=>{const v=await command<{cancelled:boolean}>("ABANDON",{id:unconfirmed.id});if(v.cancelled){clearReceipt();setNotice("已取消尚未发送的尝试。草稿保留。");}else setNotice("这条回复已进入发送，请查看原对话，不会重复发送。");await refresh();})}>取消尚未发送的尝试</button></div></div>}
  </div>
  <div className="v4-chat-dock"><form className="v4-chat-composer" onSubmit={e => { e.preventDefault(); void act(() => send(nextMode)); }}>
   <label htmlFor="watch-chat-reply">回复这个 Codex 对话</label><textarea id="watch-chat-reply" ref={input} placeholder={`在 ${state?.host??"这台电脑"} 上工作`} value={draft.text} maxLength={100000} rows={2} onChange={e => setDraft(d => ({ ...d, text: e.target.value }))} onKeyDown={e => { if ((e.ctrlKey || e.metaKey) && e.key === "Enter" && !e.nativeEvent.isComposing) { e.preventDefault(); if (!busy) void act(() => send(nextMode)); } }} />
   {draft.files.length > 0 && <ul className="v4-chat-files">{draft.files.map(f => <li key={f.id}>{f.name} · {Math.ceil(f.size / 1024)} KB <button type="button" aria-label={`移除 ${f.name}`} disabled={busy} onClick={() => setDraft(d => ({ ...d, files: d.files.filter(v => v.id !== f.id) }))}>移除</button></li>)}</ul>}
   <div className="v4-chat-composer-tools"><input ref={fileInput} type="file" multiple hidden onChange={e => { const fs = e.currentTarget.files; void act(() => upload(fs)); e.currentTarget.value = ""; }} /><input ref={photoInput} type="file" accept="image/*" multiple hidden onChange={e=>{const fs=e.currentTarget.files;void act(()=>upload(fs));e.currentTarget.value="";}}/><button type="button" className="v4-chat-icon" aria-label="＋ 附件" disabled={busy || draft.files.length >= 4} onClick={() => {if(window.matchMedia?.("(max-width:760px),(hover:none) and (pointer:coarse) and (max-width:1100px)").matches)setAttachmentOpen(true);else fileInput.current?.click();}}><Plus size={24} aria-hidden="true"/><span className="v4-chat-desktop-label">附件</span></button><button type="button" className="v4-chat-icon v4-chat-permissions" aria-label={`权限与执行请求${pendingRequests.length?`，${pendingRequests.length}条待处理`:""}`} onClick={()=>setPermissionsOpen(true)}><ShieldCheck size={24} aria-hidden="true"/>{pendingRequests.length>0&&<span className="v4-chat-request-count" aria-hidden="true">{pendingRequests.length}</span>}</button><span className="v4-chat-tool-space"/><button type="button" className="v4-chat-icon" aria-label="对话选项" aria-expanded={optionsOpen} disabled={busy} onClick={() => { setOptionsOpen(v => !v); if (!models) void act(async () => { const v = await command<{ models: ChatModel[] }>("OPTIONS"); if (mounted.current) setModels(v.models); }); }}><SlidersHorizontal size={24} aria-hidden="true"/><span className="v4-chat-desktop-label">对话选项</span></button>{state?.ownedTurnId&&!draft.text.trim()?<button type="button" className="v4-chat-send" aria-label="停止本次执行" disabled={busy} onClick={()=>void act(async()=>{await command("STOP",{turnId:state.ownedTurnId});setNotice("已请求停止，等待 Codex 确认。");await refresh();})}><Square size={24} aria-hidden="true"/></button>:<button type="submit" className="v3-primary v4-chat-send" aria-label={nextMode==="STEER"?"调整当前任务":queueNext?"排队，完成后发送":"发送回复"} disabled={busy || !state || !draft.text.trim() || blocked}><ArrowUp size={24} aria-hidden="true"/><span className="v4-chat-desktop-label">{busy?"正在处理…":nextMode==="STEER"?"调整当前任务":queueNext?"排队，完成后发送":"发送回复"}</span></button>}</div>
   {queueNext&&draft.text.trim()&&<p className="v4-meta v4-chat-mode">{nextMode==="STEER"?"调整当前任务":"当前任务完成后发送"} · 在对话选项中选择</p>}
   {pending && <p className="v4-meta">{replyStates[pending.status]} · 请先处理这条回复。</p>}
  </form><p className="v4-meta v4-chat-dictation">手机键盘可直接听写</p></div>
  <ChatSheet title="添加附件" open={attachmentOpen} onOpenChange={setAttachmentOpen}><div className="v4-chat-sheet-list"><button type="button" onClick={()=>{photoInput.current?.click();setAttachmentOpen(false);}}>照片或截图</button><button type="button" onClick={()=>{fileInput.current?.click();setAttachmentOpen(false);}}>选择文件</button></div><p className="v4-meta">最多 4 个附件，每个 8 MB，总计 16 MB。上传后可移除，发送时才交给这个对话。</p></ChatSheet>
  <ChatSheet title="对话选项" open={optionsOpen} onOpenChange={setOptionsOpen}><fieldset className="v4-chat-options"><legend>本次回复设置</legend><label>模型<select aria-label="模型" value={draft.model} onChange={e=>setDraft(d=>({...d,model:e.target.value,effort:""}))}><option value="">沿用原对话</option>{models?.map(m=><option key={m.id} value={m.id}>{m.displayName||m.model||m.id}</option>)}</select></label><label>思考强度<select aria-label="思考强度" value={draft.effort} disabled={!draft.model} onChange={e=>setDraft(d=>({...d,effort:e.target.value}))}><option value="">沿用原对话</option>{models?.find(m=>m.id===draft.model)?.supportedReasoningEfforts.map(e=><option key={e.reasoningEffort} value={e.reasoningEffort}>{e.reasoningEffort}</option>)}</select></label>{state?.ownedTurnId&&<label>运行中跟进<select aria-label="运行中跟进方式" value={followupMode} onChange={e=>setFollowupMode(e.target.value as "QUEUE"|"STEER")}><option value="QUEUE">完成后发送</option><option value="STEER">调整当前任务</option></select></label>}<p className="v4-meta">权限沿用本机设置；具体请求到达时确认。调整当前任务时沿用当前模型。</p></fieldset></ChatSheet>
  <ChatSheet title="权限与执行请求" open={permissionsOpen} onOpenChange={setPermissionsOpen}><p>权限沿用这台电脑上的 Codex 设置。</p>{state?.requests.length?state.requests.map(r=><RequestCard key={`${r.requestId}:${r.revision}`} request={r} disabled={busy} respond={extra=>act(async()=>{await command("RESPOND",{requestId:r.requestId,input:extra});await refresh();})}/>):<p className="v4-meta">目前没有这个 Router 接收到的待确认请求。原 Codex 正在运行的任务，请在原对话处理请求。</p>}</ChatSheet>
  <ChatSheet title="对话信息" open={infoOpen} onOpenChange={setInfoOpen}><p>{original.label}</p><p>{original.cwd}</p><code>{original.threadId}</code><p className="v4-meta">回复会发到这个原对话。</p></ChatSheet>
 </section>;
}
