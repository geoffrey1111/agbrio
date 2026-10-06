import {t as uiText,useLanguage,getLanguage} from "../../i18n";
import {useBackLayer} from "./navigationHistory";
import { WatchChat } from "./WatchChat";
import type { CodexWatch } from "./CodexNotifications";
import type { WatchChatApi } from "./watchChatApi";
import {messageMedia} from "../codex/messageMedia";
import type { ExistingCodexThreadCatalog } from "../codex/types";
import { BridgeBindingDialog } from "./BridgeBindingDialog";
import * as Dialog from "@radix-ui/react-dialog";
import { useContext, useEffect, useLayoutEffect, useRef, useState } from "react";
import {NativeSurfaceContext} from "./NativeSurfaceContext";
import {useReaderCopy} from "./NativeReaderActions";
import { MarkdownMessage } from "../codex/MarkdownMessage";
import { RelayReviewSurface } from "./RelayReviewSurface";
import { PanelDivider } from "./PanelDivider";
import { Fragment } from "react";
import {motion,useReducedMotion} from "motion/react";
import { readRoleReviews, saveRoleReviews, reviewKey, type RoleReview } from "./roleReviewDrafts";
import { Columns2, History, RefreshCw, Settings2, SquarePen, SlidersHorizontal, ArrowRightLeft, MessageSquareReply,Link2 } from "lucide-react";

export type BridgeRole = "DECISION" | "EXECUTION";
export type RelayTextBlock = { id: string; kind: "INSTRUCTION" | "CODE" | "PROSE"; recommended: boolean; text: string };
export type RoleInput = { provider: "CODEX" | "CHATGPT"; externalId: string; label: string; cwd?: string | null };
type Endpoint = { id: string; provider: "CODEX" | "CHATGPT"; externalId: string; label: string };
type Side = { role: BridgeRole; endpoint: Endpoint; cwd?: string | null };
export type RoleBindings = { workstreamId: string; bindingRevision: number; decision?: Side | null; execution?: Side | null; explicitRoles: boolean };
type Reply = { id: string; endpointId: string; text: string; assistantIdentity?: string | null; observedAt?: number };
export type RoleHandoff = { id: string; workstreamId: string; approvedText: string; originalText: string; payloadHash: string; status: string; sourceEndpoint?: Endpoint; destinationEndpoint: Endpoint; errorMessage?: string | null; attachments: { id: string; filename: string; size?: number | null; sha256?: string | null }[] };
export type RoleActivity={role:BridgeRole;endpointId:string;state:string;checkedAt:number;turnId?:string|null;resultObservationId?:string|null};
export type RoleState = { bindings: RoleBindings; replies: Reply[]; handoffs: RoleHandoff[];handoffSources?:{id:string;role:BridgeRole;bindingRevision:number}[];activities?:RoleActivity[];snapshotAt?:number;readOutcome?:{role:BridgeRole;endpointId:string;state:string;retainedReply:boolean} };
export type RoleBridgeApi = {
  state: (workstream: string) => Promise<RoleState>;
  sync?: (workstream:string)=>Promise<RoleState>;
  bind: (workstream: string, revision: number, decision: RoleInput, execution: RoleInput) => Promise<RoleBindings>;
  read: (workstream: string, role: BridgeRole) => Promise<RoleState>;
  prepare: (workstream: string, role: BridgeRole, observation: string, text: string, attachmentIds?: string[]) => Promise<RoleHandoff>;
  attachments: (workstream: string, role: BridgeRole, observation: string) => Promise<{ id: string; filename: string; sha256?: string | null; size?: number | null }[]>;
  blocks?: (workstream: string, role: BridgeRole, observation: string) => Promise<RelayTextBlock[]>;
  edit: (handoff: string, expectedHash: string, text: string) => Promise<RoleHandoff>;
  approve: (handoff: string, expectedHash: string) => Promise<RoleHandoff>;
  send: (handoff: string) => Promise<RoleState>;
  threads: (workstream: string) => Promise<ExistingCodexThreadCatalog>;
  connect: () => Promise<unknown>;
  openChat?: (thread:string)=>Promise<CodexWatch>;
  chat?:WatchChatApi;
};
const label = (role: BridgeRole) => role === "DECISION" ? uiText("控制端") : uiText("执行端");
const initial = (side?: Side | null): RoleInput => ({ provider: side?.endpoint.provider ?? "CODEX", externalId: side?.endpoint.provider === "CHATGPT" ? `https://chatgpt.com/c/${side.endpoint.externalId}` : side?.endpoint.externalId ?? "", label: side?.endpoint.label ?? "" });
function errorText(error: unknown) {
  const text = String(error);
  if (text.includes("owned by another application") || text.includes("THREAD_OWNED")) return uiText("对话正在 Codex Desktop 中使用。请在电脑端 Router 的设置 → 连接中准备共享连接，再自行退出 Desktop，点“打开共享 Codex”。已批准内容保留，尚未发送。");
  if (text.includes("BRIDGE_SAME_NATIVE_TARGET")) return uiText("两端必须选择不同的 Codex 对话。");
  if (text.includes("BRIDGE_BINDING_CHANGED")) return uiText("绑定已发生变化，请重新打开审阅。旧批准不能用于新目标。");
  if (text.includes("BRIDGE_NATIVE_TARGET_ALREADY_BOUND")) return uiText("这条对话已绑定其他工作流，请先核对原绑定。");
  if (text.includes("BRIDGE_TARGET_ALREADY_RUNNING")) return uiText("目标对话正在执行，请等待当前回合完成。");
  if (text.includes("BRIDGE_TARGET_GOAL_ACTIVE")) return uiText("目标对话有活动或受阻的 Goal，请先核对该对话的目标状态；Router 不会替你恢复或改变目标。");
  if (text.includes("BRIDGE_ATTACHMENT_CHANGED_AFTER_REVIEW")) return uiText("文件在确认后发生了变化，请重新读取源回复并确认附件。");
  if (text.includes("BRIDGE_REVIEW_CHANGED_OR_NOT_READY") || text.includes("BRIDGE_DRAFT_CHANGED")) return uiText("审阅内容已被另一端更新，请刷新后重新批准。");
  if (text.includes("BRIDGE_WRITER_UNRESOLVED")) return uiText("已有发送或执行尚未确认结束，请核对原记录；不能重复发送。");
  if (text.includes("NO_NEW_TERMINAL_REPLY") || text.includes("LATEST_TURN_ACTIVE")) return uiText("当前没有可读取的新完整回复，或对话仍在执行。");
  if (text.includes("LATEST_TURN_INTERRUPTED")) return uiText("最新执行状态需核对，目前没有新的完整回复。");
  if (text.includes("OBSERVATION_MATERIALIZATION_PENDING")) return uiText("完整回复正在整理，请稍后检查。");
  return text;
}
export function RoleBridgePanel({ workstreamId, workstreamName, api, onModeChange, onBindingsChanged, onOpenConnection, bindingRequest = 0, externalBindingEntry = false }: {
  workstreamId: string; workstreamName?: string; api: RoleBridgeApi; onModeChange?: (enabled: boolean) => void; onBindingsChanged?: () => void; onOpenConnection?: () => void; bindingRequest?: number; externalBindingEntry?: boolean;
}) {
 useLanguage();
  const seenBindingRequest = useRef(bindingRequest);
  const nativeSurface=useContext(NativeSurfaceContext);
  const reducedMotion=useReducedMotion();
  const [state, setState] = useState<RoleState | null>(null);
  const [editing, setEditing] = useState(false);
  const [bindingStep, setBindingStep] = useState<0 | 1 | 2>(0);
  const [decision, setDecision] = useState<RoleInput>(initial());
  const [execution, setExecution] = useState<RoleInput>(initial());
  const [catalog, setCatalog] = useState<Awaited<ReturnType<RoleBridgeApi["threads"]>> | null>(null);
  const [busy, setBusy] = useState(false);
  const panel = useRef<HTMLElement|null>(null);
  const readingPositions = useRef<Partial<Record<BridgeRole,number>>>({});
  const [reviewOpen, setReviewOpen] = useState(false);
  const [activeRole, setActiveRole] = useState<BridgeRole>("DECISION");
  const pending = useRef(false);
  const generation = useRef(0);
  const pollBusy=useRef(false),operationRevision=useRef(0),stateRef=useRef<RoleState|null>(null),protectedView=useRef(false),receivedAt=useRef(0);
  const[clock,setClock]=useState(Date.now());stateRef.current=state;
  const[chat,setChat]=useState<CodexWatch|null>(null);
  useBackLayer(reviewOpen,()=>setReviewOpen(false));
  useBackLayer(editing,()=>setEditing(false));
  useBackLayer(Boolean(chat),()=>setChat(null));
  const [error, setError] = useState<string | null>(null);
  const connectionAction = error?.includes(uiText("对话正在 Codex Desktop 中使用")) && onOpenConnection ? <button type="button" onClick={() => { setReviewOpen(false); onOpenConnection(); }}>{uiText("连接设置")}</button> : null;
  const [review, setCurrentReview] = useState<RoleReview | null>(null);
  const [drafts,setDrafts]=useState(()=>({workstream:workstreamId,values:readRoleReviews(workstreamId)}));
  const [draftError,setDraftError]=useState(false);
  const [ratio,setRatio]=useState(50);
  const [compare,setCompare]=useState(false),[records,setRecords]=useState(false);
  const [checked,setChecked]=useState<Partial<Record<BridgeRole,{at:number;sourceId?:string}>>>({});
  function setReview(value:RoleReview|null){if(value?.bindingRevision!=null&&value.bindingRevision!==state?.bindings.bindingRevision)value={...value,choosing:false};setCurrentReview(value);if(!value)return;setDrafts(previous=>{const values={...(previous.workstream===workstreamId?previous.values:readRoleReviews(workstreamId)),[reviewKey(value)]:value};try{saveRoleReviews(workstreamId,values);setDraftError(false);}catch{setDraftError(true);}return{workstream:workstreamId,values};});}
  function endReview(){if(review){setDrafts(previous=>{const values={...previous.values};delete values[reviewKey(review)];try{saveRoleReviews(workstreamId,values);}catch{setDraftError(true);}return{workstream:workstreamId,values};});}setCurrentReview(null);setReviewOpen(false);}
  // A closed draft retains its exact source, but must not freeze live readers.
  protectedView.current=Boolean(reviewOpen||editing);
  useEffect(() => {
    const current = ++generation.current;
    pending.current=false;pollBusy.current=false;setBusy(false);
    try{readingPositions.current=JSON.parse(localStorage.getItem(`aiwr.role-scroll.${workstreamId}`)??"{}");}catch{readingPositions.current={};} setReviewOpen(false); setState(null); setEditing(false); setCurrentReview(null); setDrafts({workstream:workstreamId,values:readRoleReviews(workstreamId)});setChecked({});setChat(null);setError(null); onModeChange?.(false);
    void api.state(workstreamId).then(value => {
      if (generation.current !== current) return;
      setState(value); onModeChange?.(Boolean(value.bindings.explicitRoles));
      let saved:string|null=null;try{saved=localStorage.getItem(`aiwr.role-selected.${workstreamId}`);}catch{/* optional reading preference */}
      const newest=value.replies.slice().sort((a,b)=>(b.observedAt??0)-(a.observedAt??0))[0];
      setActiveRole(nativeSurface&&(saved==="DECISION"||saved==="EXECUTION")?saved:nativeSurface&&newest?newest.endpointId===value.bindings.execution?.endpoint.id?"EXECUTION":"DECISION":value.replies.some(reply=>reply.endpointId===value.bindings.execution?.endpoint.id)?"EXECUTION":"DECISION");
    }).catch(() => { if(nativeSurface&&generation.current===current)setError(uiText("此 Bridge 暂时无法读取，请重试。")); });
    return () => { generation.current++; };
  }, [workstreamId, api]);
  async function act(operation: (valid: () => boolean) => Promise<void>) {
    if (pending.current) return;
    pending.current = true; ++operationRevision.current;setBusy(true); setError(null);
    const current = generation.current;
    try { await operation(() => current === generation.current); } catch (cause) { if (current === generation.current) setError(errorText(cause)); }
    finally { if (current === generation.current) { pending.current = false; setBusy(false); } }
  }
  useEffect(()=>{
    if(!api.sync||!state?.bindings.explicitRoles)return;
    const current=generation.current;let alive=true;const sync=api.sync;
    const check=async()=>{
      if(!alive||document.visibilityState==="hidden"||panel.current?.closest("[hidden]")||pending.current||pollBusy.current)return;
      pollBusy.current=true;const revision=operationRevision.current;
      try{const next=await sync(workstreamId);if(!alive||current!==generation.current||revision!==operationRevision.current)return;
        const old=stateRef.current;if(!old||next.bindings.workstreamId!==workstreamId)return;
        if(protectedView.current&&old.bindings.bindingRevision!==next.bindings.bindingRevision){setState({...old,activities:undefined,snapshotAt:0});receivedAt.current=0;return;}
        receivedAt.current=Date.now();
        if(protectedView.current){setState({...old,activities:next.activities,snapshotAt:next.snapshotAt});}
        else setState(next);
      }catch{if(alive&&current===generation.current&&revision===operationRevision.current){setState(old=>old?{...old,activities:[old.bindings.decision,old.bindings.execution].filter(Boolean).map(side=>({role:side!.role,endpointId:side!.endpoint.id,state:"UNCONFIRMED",checkedAt:0})),snapshotAt:0}:old);}}
      finally{pollBusy.current=false;}
    };
    void check();const timer=setInterval(()=>{setClock(Date.now());void check();},2000);const wake=()=>{if(document.visibilityState!=="hidden"){setClock(Date.now());void check();}};document.addEventListener("visibilitychange",wake);window.addEventListener("online",wake);
    return()=>{alive=false;clearInterval(timer);document.removeEventListener("visibilitychange",wake);window.removeEventListener("online",wake);};
  },[api,workstreamId,state?.bindings.explicitRoles]);
  function readingScroller(): HTMLElement | null {
    const local=panel.current?.querySelector<HTMLElement>(`#role-reader-${activeRole} .v4-role-original`);
    if(local&&nativeSurface)return local;
    let node=panel.current?.parentElement;
    while(node){if(/auto|scroll/.test(getComputedStyle(node).overflowY)&&node.scrollHeight>node.clientHeight)return node;node=node.parentElement;}
    return null;
  }
  function selectRole(role:BridgeRole){if(role===activeRole)return;const scroller=readingScroller();readingPositions.current[activeRole]=scroller?.scrollTop??window.scrollY;setActiveRole(role);try{localStorage.setItem(`aiwr.role-selected.${workstreamId}`,role);}catch{/* optional reading preference */}}
  useLayoutEffect(()=>{if(panel.current?.closest("[hidden]"))return;const top=readingPositions.current[activeRole]??0;const scroller=readingScroller();if(scroller)scroller.scrollTop=top;else window.scrollTo({top,behavior:"instant"});},[activeRole,state?.bindings.explicitRoles]);
  useEffect(()=>{if(!state?.bindings.explicitRoles)return;const scroller=readingScroller();if(!scroller)return;const track=()=>{if(panel.current?.closest("[hidden]"))return;readingPositions.current[activeRole]=scroller.scrollTop;try{localStorage.setItem(`aiwr.role-scroll.${workstreamId}`,JSON.stringify(readingPositions.current));}catch{}};scroller.addEventListener("scroll",track);return()=>scroller.removeEventListener("scroll",track);},[activeRole,workstreamId,state?.bindings.explicitRoles]);
  const bindings = state?.bindings;
  const copiedEndpoint=(activeRole==="DECISION"?bindings?.decision:bindings?.execution)?.endpoint.id;
  const copiedReply=state?.replies.filter(item=>item.endpointId===copiedEndpoint).sort((a,b)=>(b.observedAt??0)-(a.observedAt??0))[0];
  useReaderCopy(copiedReply?.text??null);
  const oldReview=review?.bindingRevision!=null&&review.bindingRevision!==bindings?.bindingRevision;
  function loadCatalog() {
    void act(async valid => { await api.connect(); const next = await api.threads(workstreamId); if(valid()) setCatalog(next); });
  }
  function open(step: 0 | 1 | 2 = 0) {
    setDecision(initial(bindings?.decision)); setExecution(initial(bindings?.execution)); setBindingStep(step); setEditing(true); setError(null);
    if (!catalog) loadCatalog();
  }
  useEffect(() => {
    if (bindingRequest === seenBindingRequest.current || review) return;
    if (!bindings) {
      void act(async valid => { const next = await api.state(workstreamId); if (valid()) {setState(next);onModeChange?.(Boolean(next.bindings.explicitRoles));} });
      return;
    }
    seenBindingRequest.current = bindingRequest;
    open(bindings.explicitRoles ? 2 : 0);
  }, [bindingRequest, bindings]);
  function latest(side: Side) { return state?.replies.filter(reply => reply.endpointId === side.endpoint.id).sort((a,b) => (b.observedAt ?? 0) - (a.observedAt ?? 0)).at(0); }
  function reviewPayload(value:RoleReview){
    if(!value.choosing || !value.blocks?.length)return value.text;
    const chosen=value.blocks.filter(block=>value.blockIds?.includes(block.id));
    return chosen.length===value.blocks.length?value.reply.text:chosen.map(block=>block.text).join("\n\n");
  }
  function sendReview(){
    if(!review||oldReview||busy)return;
    const currentReview=review;const text=reviewPayload(currentReview)+(currentReview.attachmentManifest??"");
    if(!text.trim() || (currentReview.approved && currentReview.approved.status!=="APPROVED"))return;
    void act(async valid=>{
      let prepared=currentReview.prepared;
      let approved=currentReview.approved;
      if(!approved){
        prepared=prepared??await api.prepare(workstreamId,currentReview.role,currentReview.reply.id,text,[]);
        if(!valid())return;
        if(prepared.workstreamId!==workstreamId || (currentReview.destinationId && prepared.destinationEndpoint.id!==currentReview.destinationId))throw Error("BRIDGE_BINDING_CHANGED");
        if(prepared.approvedText!==text){prepared=await api.edit(prepared.id,prepared.payloadHash,text);if(!valid())return;}
        approved=await api.approve(prepared.id,prepared.payloadHash);if(!valid())return;
      }
      if(approved.status!=="APPROVED" || approved.workstreamId!==workstreamId || (currentReview.destinationId&&approved.destinationEndpoint.id!==currentReview.destinationId))throw Error("BRIDGE_BINDING_CHANGED");
      const retained={...currentReview,text,attachmentManifest:undefined,choosing:false,prepared,approved};
      setReview({...retained,approved:{...approved,status:"SENDING"}});
      try{
        const next=await api.send(approved.id);if(!valid())return;setState(next);
        const receipt=next.handoffs.find(item=>item.id===approved!.id);
        setReview({...retained,approved:receipt??{...approved,status:"UNKNOWN"}});
      }catch(cause){
        // Only a proven pre-write rejection may leave APPROVED retryable.
        // A lost response remains SENDING/UNKNOWN, and can only be reconciled.
        try{const next=await api.state(workstreamId);if(valid()){setState(next);const receipt=next.handoffs.find(item=>item.id===approved!.id);if(receipt)setReview({...retained,approved:receipt});}}catch{/* Preserve uncertain delivery. */}
        throw cause;
      }
    });
  }
  function selectAttachments(ids:string[]){
    if(!review||review.approved||oldReview)return;
    const snapshot={...review,selected:ids,prepared:undefined,attachmentManifest:undefined,choosing:false,text:reviewPayload(review)};
    setReview(snapshot);
    if(!ids.length)return;
    void act(async valid=>{
      const prepared=await api.prepare(workstreamId,snapshot.role,snapshot.reply.id,snapshot.text,ids);
      if(!valid())return;
      if(!prepared.approvedText.startsWith(snapshot.text))throw Error("BRIDGE_REVIEW_CHANGED_OR_NOT_READY");
      setReview({...snapshot,prepared,attachmentManifest:prepared.approvedText.slice(snapshot.text.length)});
    });
  }
  return <section ref={panel} className="v3-role-bridge" aria-label={uiText("决策与执行提供方兼容")}>
    {nativeSurface&&!state&&!error&&<p className="r2-role-loading" role="status">{uiText("正在读取…")}</p>}
    {(!externalBindingEntry||nativeSurface) && !bindings?.explicitRoles && !editing && state && <div className="r2-unbound"><Link2 size={32}/><button type="button" aria-label={uiText("绑定控制端与执行端")} onClick={() => open()}>{uiText("绑定两端")}</button></div>}
    {bindings?.explicitRoles && <><header className="v4-role-heading"><h2>{workstreamName || uiText("当前工作")}</h2>{(!nativeSurface||bindings?.explicitRoles)&&<div className="v5-reader-tools"><button type="button" aria-label={uiText("双端对照")} aria-pressed={compare} onClick={()=>setCompare(!compare)}><Columns2 size={18}/></button><button type="button" aria-label={uiText("草稿与记录")} aria-pressed={records} onClick={()=>setRecords(!records)}><History size={18}/>{Object.keys(drafts.values).length||null}</button><button type="button" aria-label={uiText("对话设置")} title={uiText("对话设置")} disabled={busy || reviewOpen} onClick={() => open(2)}><Settings2 size={18}/></button></div>}</header>
      {draftError&&<p role="alert">{uiText("草稿未能保存，请保留此窗口。")}</p>}

      <div className="v4-role-tabs" role="tablist" aria-label={uiText("阅读哪一端的回复")}>{(["DECISION", "EXECUTION"] as const).map(role => {const side=role==="DECISION"?bindings.decision:bindings.execution;return <button key={role} id={`role-tab-${role}`} type="button" role="tab" aria-selected={activeRole===role} aria-controls={`role-reader-${role}`} tabIndex={activeRole===role?0:-1} onClick={()=>selectRole(role)} onKeyDown={event=>{if(["ArrowLeft","ArrowRight","Home","End"].includes(event.key)){event.preventDefault();const target=event.key==="Home"?"DECISION":event.key==="End"?"EXECUTION":role==="DECISION"?"EXECUTION":"DECISION";selectRole(target);(event.currentTarget.parentElement?.querySelector(`[aria-controls="role-reader-${target}"]`) as HTMLButtonElement|null)?.focus();}}} aria-label={`${label(role)} · ${side?.endpoint.provider === "CODEX" ? "Codex" : side?.endpoint.provider || uiText("未绑定")}`}>{activeRole===role&&<motion.span className="r2-role-selection" layoutId={`role-selection-${workstreamId}`} transition={reducedMotion?{duration:0}:{type:"spring",stiffness:560,damping:42}}/>}{role==="DECISION"?<SquarePen size={15}/>:<SlidersHorizontal size={15}/>} {role==="DECISION"?uiText("控制"):uiText("执行")}</button>;})}</div>
      {state?.handoffs.some(h=>["UNKNOWN","SENDING"].includes(h.status))&&<p className="v4-delivery-attention" role="status">{uiText("有交接的送达状态仍待确认。请先核对记录，避免重复发送。")}</p>}
      <div className={`v3-role-bridge-sides ${compare?"v5-compare":""}`} style={compare?{gridTemplateColumns:`minmax(0,${ratio}fr) 8px minmax(0,${100-ratio}fr)`}:undefined}>{(compare?["DECISION","EXECUTION"] as BridgeRole[]:[activeRole]).map((role,index) => {
        const side = role === "DECISION" ? bindings.decision : bindings.execution;
        const reply = side ? latest(side) : undefined;
        const outcome=state?.readOutcome?.role===role&&state.readOutcome.endpointId===side?.endpoint.id?state.readOutcome:undefined;
        const activity=state?.activities?.find(a=>a.role===role&&a.endpointId===side?.endpoint.id);
        const fresh=activity&&receivedAt.current>0&&(clock-receivedAt.current+Math.max(0,(state?.snapshotAt??0)-activity.checkedAt))<7000;
        const phase=api.sync?(fresh?activity?.state:"UNCONFIRMED"):undefined;
        const currentResult=Boolean(reply&&phase==="COMPLETE"&&activity?.resultObservationId===reply.id);
        const historical=Boolean(api.sync&&!currentResult)||Boolean(outcome&&outcome.state!=="NO_NEW_TERMINAL_REPLY");
        const notice=phase==="RUNNING"?uiText("执行中"):phase==="COMPLETE"?(currentResult?uiText("已完成"):uiText("新结果正在同步")):phase==="RESULT_PENDING"?uiText("新结果正在同步"):phase==="INTERRUPTED"?uiText("最新一轮已中断"):phase==="ACTION_REQUIRED"?uiText("待处理"):phase==="PAUSED"?uiText("已暂停"):phase==="FAILED"?uiText("最新执行未完成，请核对原对话"):phase==="EMPTY"?uiText("当前没有执行结果"):phase==="UNCONFIRMED"?uiText("状态待确认"):outcome?.state==="LATEST_TURN_INTERRUPTED"?uiText("{0}最新一轮已中断。{1}", label(role), outcome.retainedReply?uiText("当前保留的是先前的完整回复。"):uiText("目前没有新的完整回复。")):outcome?.state==="LATEST_TURN_ACTIVE"?uiText("{0}仍在执行。{1}", label(role), outcome.retainedReply?uiText("当前显示的是先前的完整回复。"):uiText("完成后可检查回复。")):outcome?.state==="OBSERVATION_MATERIALIZATION_PENDING"?uiText("完整回复正在整理，请稍后检查。"):outcome?.state==="EXTERNAL_STATUS_UNCONFIRMED"?uiText("状态待确认"):outcome?uiText("当前没有新的完整回复。"):null;
        const retainedDraft=drafts.values[reviewKey({role,reply:reply??{id:"",endpointId:"",text:""},text:reply?.text??"",options:[],selected:[],bindingRevision:bindings.bindingRevision})];
        const sent=retainedDraft?.approved?.status==="SENT";
        const relayActionLabel=retainedDraft?(nativeSurface&&sent?uiText("查看发送记录"):uiText("继续审阅交接")):uiText("转给{0}", label(role === "DECISION" ? "EXECUTION" : "DECISION"));
        return <Fragment key={role}><article id={`role-reader-${role}`} role="tabpanel" aria-labelledby={`role-tab-${role}`}><header className="v4-role-reader-heading"><strong>{side?.endpoint.label || uiText("尚未绑定")}</strong><button type="button" disabled={busy || !side} onClick={() => void act(async valid => { const next = await api.read(workstreamId, role); if(valid()){setState(next);const endpoint=(role==="DECISION"?next.bindings.decision:next.bindings.execution)?.endpoint.id;const source=next.replies.filter(r=>r.endpointId===endpoint).sort((a,b)=>(b.observedAt??0)-(a.observedAt??0))[0];setChecked(old=>({...old,[role]:{at:Date.now(),sourceId:next.readOutcome?undefined:source?.id}}));} })} aria-label={uiText("检查{0}回复", label(role))} title={uiText("刷新")}><RefreshCw size={18}/></button></header>
          {notice&&!nativeSurface&&<p className="v5-role-status" role="status">{uiText(notice)}</p>}
          {nativeSurface?<div className="v5-reply-time"><span role="status">{currentResult?uiText("最新"):notice??uiText("已保存")}{reply&&!currentResult&&["RUNNING","INTERRUPTED","RESULT_PENDING"].includes(phase??"")?uiText(" · 上次结果"):""}</span>{reply?.observedAt&&<time dateTime={new Date(reply.observedAt).toISOString()}>{uiText("收到")} {new Date(reply.observedAt).toLocaleString(getLanguage(),{month:"numeric",day:"numeric",hour:"2-digit",minute:"2-digit"})}</time>}</div>:<div className="v5-reply-time">{reply&&<span>{currentResult||checked[role]?.sourceId===reply.id?uiText("最新已读取"):uiText("已保存")}</span>}{reply?.observedAt&&<time dateTime={new Date(reply.observedAt).toISOString()}>{uiText("收到")} {new Date(reply.observedAt).toLocaleString(getLanguage(),{month:"numeric",day:"numeric",hour:"2-digit",minute:"2-digit"})}</time>}{checked[role]&&<span>{uiText("核对")} {new Date(checked[role]!.at).toLocaleTimeString(getLanguage(),{hour:"2-digit",minute:"2-digit"})}</span>}{fresh&&<span>{uiText("同步")} {new Date(activity!.checkedAt).toLocaleTimeString(getLanguage(),{hour:"2-digit",minute:"2-digit"})}</span>}</div>}

          {reply ? <><div className="v4-role-original"><MarkdownMessage text={reply.text} media={messageMedia({kind:"ROLE",workstreamId,role,observationId:reply.id})}/></div><footer className="v4-role-reader-actions"><button type="button" className="v3-primary" aria-label={relayActionLabel} disabled={busy || reviewOpen} onClick={() => void act(async valid => { const target = role === "DECISION" ? bindings.execution : bindings.decision;
            const key=reviewKey({role,reply,text:reply.text,options:[],selected:[],bindingRevision:bindings.bindingRevision});
            const existing=drafts.workstream===workstreamId?drafts.values[key]:undefined;
            if(existing){
              const actual=existing.approved&&state?.handoffs.find(h=>h.id===existing.approved!.id);
              // An untouched, zero-selection draft can refresh its structural
              // suggestions. Selected/edited/prepared/approved payloads stay exact.
              if(!existing.approved&&!existing.prepared&&existing.choosing&&!existing.blockIds?.length&&existing.text===existing.reply.text&&api.blocks){
                const blocks=await api.blocks(workstreamId,role,existing.reply.id);if(!valid())return;
                setReview({...existing,blocks,blockIds:[],choosing:blocks.length>0});
              }else setReview(actual?{...existing,approved:actual}:existing);
              setReviewOpen(true);return;
            }
            const [options, blocks] = await Promise.all([api.attachments(workstreamId, role, reply.id), api.blocks?.(workstreamId, role, reply.id) ?? Promise.resolve([])]); if(valid()){setReview({ role, reply, bindingRevision: bindings.bindingRevision, destinationId: target?.endpoint.id, text: reply.text, blocks, blockIds: [], choosing: blocks.length > 0, options, selected: [] });setReviewOpen(true);} })}><ArrowRightLeft size={18}/>{nativeSurface&&sent?uiText("记录"):uiText("转发")}</button>{side?.endpoint.provider==="CODEX"&&api.openChat&&api.chat&&<button type="button" disabled={busy} onClick={()=>void act(async valid=>{const next=await api.openChat!(side.endpoint.externalId);if(valid())setChat(next);})} aria-label={uiText("监听并回复")} title={uiText("回复原对话")}><MessageSquareReply size={20}/></button>}</footer></> : <div className="v4-reader-empty"><h3>{uiText("这里会显示")}{label(role)}{uiText("的完整回复")}</h3><p>{uiText("检查对话后再阅读，不会启动任务或发送消息。")}</p></div>}
          <details className="v4-secondary-details"><summary>{uiText("对话信息")}</summary>{side?.cwd && <p>{uiText("项目目录：")}{side.cwd}</p>}<p>{side?.endpoint.externalId}</p></details>
        </article>{compare&&index===0&&<PanelDivider value={ratio} min={25} max={75} step={5} factor={(panel.current?.clientWidth??1000)/100} label={uiText("调整两端宽度")} onChange={setRatio}/>}</Fragment>;
      })}</div>
      {!compare&&<div id={`role-reader-${activeRole==="DECISION"?"EXECUTION":"DECISION"}`} role="tabpanel" hidden />}
      {records&&<section className="v5-review-records" aria-label={uiText("草稿与记录")}>{Object.entries(drafts.values).map(([key,draft])=><button type="button" key={key} onClick={()=>{setReview(draft);setReviewOpen(true);}}>{nativeSurface?({CANCELLED:uiText("已结束"),SENT:uiText("已发送"),SENDING:uiText("发送中"),UNKNOWN:uiText("待核对"),FAILED:uiText("发送失败"),APPROVED:uiText("待发送")} as Record<string,string>)[draft.approved?.status??draft.prepared?.status??""]??uiText("草稿"):uiText("草稿")} · {label(draft.role)} → {label(draft.role==="DECISION"?"EXECUTION":"DECISION")}</button>)}{state?.handoffs.length ? <details className="v4-secondary-details" open><summary>{uiText("交接记录 ·")} {state.handoffs.length}</summary><button type="button" disabled={busy} onClick={() => void act(async valid => { const next = await api.state(workstreamId); if(valid()) setState(next); })}>{uiText("刷新交接状态")}</button>{state.handoffs.map(h => <article key={h.id}><p>{h.destinationEndpoint.label} · {({CANCELLED:uiText("已结束"),READY:uiText("草稿"),APPROVED:uiText("已批准"),SENDING:uiText("发送中"),SENT:uiText("已发送"),FAILED:uiText("发送失败"),UNKNOWN:uiText("送达待确认")} as Record<string,string>)[h.status]??h.status}</p><details><summary>{uiText("查看交接内容")}</summary><pre>{h.approvedText}</pre></details>{h.errorMessage && <p role="alert">{h.errorMessage}</p>}{["READY","APPROVED"].includes(h.status) && h.sourceEndpoint && <button type="button" disabled={busy || reviewOpen} onClick={() => {
        const provenance=state.handoffSources?.find(s=>s.id===h.id);const role = provenance?.role ?? (h.sourceEndpoint!.id === bindings.decision?.endpoint.id ? "DECISION" : "EXECUTION");
        setReviewOpen(true);setReview({ role, bindingRevision:provenance?.bindingRevision, destinationId:h.destinationEndpoint.id, reply: { id: "retained-approved-source", endpointId: h.sourceEndpoint!.id, text: h.originalText }, text: h.approvedText, options: [], selected: [], prepared: h, approved: h.status === "APPROVED" ? h : undefined });
      }}>{uiText("继续")}{h.status === "APPROVED" ? uiText("已批准交接") : uiText("审阅")}</button>}</article>)}</details> : null}</section>}
    </>}
    {chat&&api.chat&&<Dialog.Root open onOpenChange={open=>{if(!open)setChat(null);}}><Dialog.Portal><Dialog.Overlay className="v3-role-dialog-overlay"/><Dialog.Content className="v5-role-chat" aria-describedby={undefined}><Dialog.Title className="v5-sr-only">{uiText("原对话回复")}</Dialog.Title><WatchChat original={chat} api={api.chat} onBack={()=>setChat(null)} backLabel="Bridge"/></Dialog.Content></Dialog.Portal></Dialog.Root>}
    {editing && <BridgeBindingDialog workstreamName={workstreamName} decision={decision} execution={execution} setDecision={setDecision} setExecution={setExecution} step={bindingStep} setStep={setBindingStep} catalog={catalog} busy={busy} error={uiText(error)} canSave={Boolean(bindings)} refresh={loadCatalog} close={() => setEditing(false)} save={() => void act(async valid => {
        await api.bind(workstreamId, bindings!.bindingRevision, decision, execution);
        const next = await api.state(workstreamId); if(!valid()) return; setState(next); onModeChange?.(next.bindings.explicitRoles); setReview(null); setReviewOpen(false); setEditing(false); onBindingsChanged?.();
    })} />}
    {review && reviewOpen && <Dialog.Root open onOpenChange={open=>{if(!open&&!busy)setReviewOpen(false);}}><Dialog.Portal><Dialog.Overlay className="v3-role-dialog-overlay r2-relay-overlay"/><Dialog.Content className="r2-relay" aria-describedby={undefined}>
      <RelayReviewSurface review={review} busy={busy} oldReview={oldReview} sourceLabel={(review.role==="DECISION"?bindings?.decision:bindings?.execution)?.endpoint.label??label(review.role)} targetLabel={review.prepared?.destinationEndpoint.label??(review.role==="DECISION"?bindings?.execution:bindings?.decision)?.endpoint.label??uiText("未绑定")} media={messageMedia({kind:"ROLE",workstreamId,role:review.role,observationId:review.reply.id})} error={uiText(error)} connectionAction={connectionAction} change={setReview} attachmentChange={selectAttachments} submit={sendReview} close={()=>setReviewOpen(false)} discard={endReview} checkStatus={()=>void act(async valid=>{const next=await api.state(workstreamId);if(valid()){setState(next);const receipt=next.handoffs.find(item=>item.id===review.approved?.id);if(receipt)setReview({...review,approved:receipt});}})}/>
    </Dialog.Content></Dialog.Portal></Dialog.Root>}
    {error && !editing && !reviewOpen && <><p role="alert">{uiText(error)}</p>{connectionAction}</>}
    {nativeSurface&&error&&!state&&!editing&&<button type="button" disabled={busy} onClick={()=>void act(async valid=>{const next=await api.state(workstreamId);if(valid()){setState(next);onModeChange?.(Boolean(next.bindings.explicitRoles));}})}>{uiText("重试")}</button>}
  </section>;
}
