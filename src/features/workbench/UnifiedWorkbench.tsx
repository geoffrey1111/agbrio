/** Legacy detail reads are retained only for historical projection/recovery harnesses. */
export const LEGACY_WORKBENCH_DETAILS=false;
import {SettingsOverview} from "./SettingsOverview";
import {NativeWorkbench} from "./NativeWorkbench";
import { useEffect, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent as ReactKeyboardEvent, type MouseEvent as ReactMouseEvent, type ReactNode } from "react";
import { BridgeDirectory, NewBridgeForm } from "./BridgeDirectory";
import { PanelDivider } from "./PanelDivider";
import { ArrowLeft, PanelLeftClose, PanelLeftOpen, Settings, MoreHorizontal } from "lucide-react";
import { MarkdownMessage } from "../codex/MarkdownMessage";
import type {
  ExactReply,
  GoalPresentation,
  HandoffPresentation,
  LifecyclePresentation,
  RuntimePresentation,
  WorkbenchAttentionItem,
  WorkbenchDraft,
  WorkbenchItem,
  WorkbenchLifecycle,
} from "./models";

export type WorkbenchSurface = "NOTIFICATIONS" | "SETTINGS" | "BRIDGES" | "NEW_BRIDGE" | "INBOX" | "WORKSPACE" | "DISCUSSION" | "PROJECT_HOME" | "PROJECT" | "NEW_WORK" | "GOAL" | "RUNTIME" | "PROVIDER_RUN_STATUS" | "CHATGPT_DIAGNOSTICS" | "HANDOFF_SELECTION" | "HANDOFF_REVIEW" | "HANDOFF_STATUS" | "EXIT_IMPACT" | "INSTALL" | "RECYCLE" | "PURGE_CONFIRM" | "TEST_CLEANUP" | "CODEX_HISTORY" | "CHATGPT_RESULTS" | "CODEX_RESULTS" | "CODEX_ATTACHMENTS" | "CODEX_FEEDBACK" | "CODEX_REQUEST";

export interface UnifiedWorkbenchProps {
  bridgeActivityApi?:import("./bridgeActivity").BridgeActivityApi;
  onManageBindings?:()=>void;
  onRenameBridge?:(id:string,name:string)=>Promise<void>;
  notificationSettings?:ReactNode;
  notificationCount?:number;
  notificationDetail?:boolean;
  devicePanel?:ReactNode;
  assistantPanel?:ReactNode;
  quotaPanel?:ReactNode;
  updatePanel?:ReactNode;
  onOpenWebAccess?:()=>void;
  onCreateBridge?: (name:string)=>Promise<void>;
  items: WorkbenchItem[];
  selectedWorkstreamId?: string | null;
  reply?: ExactReply | null;
  draft: WorkbenchDraft;
  handoff?: HandoffPresentation | null;
  goal?: GoalPresentation | null;
  lifecycle?: LifecyclePresentation | null;
  runtime?: RuntimePresentation | null;
  onSelectWorkstream: (workstreamId: string) => void;
  /** Opens one exact Router-owned attention record, never a title-matched workstream guess. */
  onSelectAttention?: (workstreamId: string, attention: WorkbenchAttentionItem) => void;
  /** D12 changes the exact target without leaving its list-first selection surface. */
  onSelectRecycleWorkstream?: (workstreamId: string) => void;
  onDraftChange: (value: string) => void;
  /** Resolves only after the host has accepted this one discussion submission. */
  onSendDiscussion?: (value: string) => void | Promise<void>;
  /** Owner-led alternative while Router's ChatGPT writer is paused.  This
   * never writes a provider message or reports delivery. */
  onOpenManualDiscussionDestination?: () => void | string | Promise<void | string>;
  manualDiscussionDestinationActionLabel?: string;
  onPinChange?: (workstreamId: string, pinned: boolean) => void;
  onLifecycleChange?: (workstreamId: string, lifecycle: WorkbenchLifecycle) => void | Promise<void>;
  onPurgeTrashedWorkstream?: (workstreamId: string) => void;
  onCreateVerifiedBackup?: () => void;
  onReplyRead?: (replyId: string) => void;
  onReplyHandled?: (replyId: string) => void;
  /** Repeats only the exact, read-only current-history operation. */
  onRetryCurrentChatGptHistory?: () => void;
  /** Runs one explicit, read-only carrier check for the exact active ChatGPT Endpoint. */
  onCheckNewChatGptReplies?: () => void;
  chatGptRefreshStatus?: string | null;
  chatGptRefreshBusy?: boolean;
  /** Visible result/progress of an explicit provider check, including after a menu closes. */
  workspaceNotice?: string | null;
  /** Opens the exact persisted ChatGPT endpoint in the owner's default browser.
   * The Router never reads or controls that page. */
  onOpenBoundChatGptInDefaultBrowser?: () => void;
  /** Opens the already-approved reverse Handoff destination only after the
   * host has rechecked that its exact destination remains the ACTIVE binding. */
  onOpenHandoffDestination?: (handoffId: string) => void;
  /** The mobile projection first resolves a safe link; desktop opens its
   * default browser directly after the same exact-binding guard. */
  handoffDestinationActionLabel?: string;
  /** Legacy carrier setup is unavailable while the owner-directed pause is active. */
  onOpenHostChatGptBrowserSetup?: () => void;
  onAddSelectionToHandoff?: (replyId: string, text: string) => void | Promise<void>;
  onPrepareHandoff?: (replyId: string) => void | WorkbenchSurface | Promise<void | WorkbenchSurface>;
  onHandoffMessageChange?: (handoffId: string, value: string) => void;
  /** Persists one explicit selection from the exact reviewed ChatGPT message. */
  onSelectHandoffAttachments?: (handoffId: string, filenames: string[]) => void | Promise<void>;
  /** Resolves only after this exact approval attempt settles. */
  onApproveHandoff?: (handoffId: string) => void | Promise<void>;
  /** Resolves only after this exact approved Handoff send attempt settles. */
  onSendHandoff?: (handoffId: string) => void | Promise<void>;
  /** Read-only reconciliation for one already-dispatched reverse Handoff. */
  onRefreshHandoffStatus?: (handoffId: string) => void | Promise<void>;
  onGoalAction?: (threadId: string, action: "PAUSE" | "RESUME" | "DELETE" | "STOP_TURN") => void;
  /** D27 is deliberately hide-only: quitting the Router is never a presentation action. */
  onHideToTray?: () => void;
  /** The host is the one navigation authority for every primary V3 surface. */
  surface?: WorkbenchSurface;
  onSurfaceChange?: (surface: WorkbenchSurface) => void;
  projectPanel?: ReactNode;
  bridgePanel?: ReactNode;
  globalActions?: ReactNode;
  runtimeAddon?:ReactNode;
  criticalNotice?: ReactNode;
  roleCompatible?: boolean;
  projectHomePanel?: ReactNode;
  newWorkPanel?: ReactNode;
  /** Read-only exact thread history; distinct from Router-owned ProviderRun results. */
  codexHistoryPanel?: ReactNode;
  hasCodexEndpoint?: boolean;
  /** Router-owned historical ChatGPT ProviderRun results; never current-page history. */
  chatgptResultsPanel?: ReactNode;
  codexResultsPanel?: ReactNode;
  /** M11 is a result-scoped attachment choice, not an ID-first form. */
  codexAttachmentsPanel?: ReactNode;
  onOpenCodexAttachments?: () => void;
  onOpenCodexResults?: () => void;
  /** Reopens only the exact ChatGPT ReplyObservation that supplied this review. */
  onOpenChatGptOrigin?: () => void;
  onReopenReverseHandoff?: () => void;
  /** D23 is a direct, exact-thread feedback draft; it is never a Handoff. */
  codexFeedbackPanel?: ReactNode;
  codexResultCount?: number;
  /** M17 is an exact official request, distinct from a Handoff or Goal action. */
  codexRequestPanel?: ReactNode;
  /** Exact, status-only ProviderRun detail for a failed/cancelled/unknown Inbox item. */
  providerRunStatusPanel?: ReactNode;
  codexRequestCount?: number;
  /** Desktop D02 opens the saved discussion editor deliberately (D15); mobile keeps its approved dock. */
  inlineComposer?: boolean;
  /** Optional task-local controls shown only in the compact workstream menu. */
  mobileWorkspaceMenu?: ReactNode;
}

function timestamp(value?: number | null) {
  return value ? new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(value) : "日期不可用";
}

function keepFocusWithinDialog(event: ReactKeyboardEvent<HTMLElement>) {
  if (event.key !== "Tab") return;
  const focusables = Array.from(event.currentTarget.querySelectorAll<HTMLElement>("button:not(:disabled), input:not(:disabled), textarea:not(:disabled), select:not(:disabled), [tabindex]:not([tabindex='-1'])"));
  const first = focusables[0];
  const last = focusables[focusables.length - 1];
  if (!first || !last) return;
  if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus(); }
  else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
}

function useCompactWorkbenchViewport() {
  const query = "(max-width: 680px)";
  const [compact, setCompact] = useState(() => typeof window !== "undefined" && typeof window.matchMedia === "function" && window.matchMedia(query).matches);
  useEffect(() => {
    if (typeof window === "undefined" || typeof window.matchMedia !== "function") return;
    const media = window.matchMedia(query);
    const update = () => setCompact(media.matches);
    update();
    media.addEventListener("change", update);
    return () => media.removeEventListener("change", update);
  }, []);
  return compact;
}

function WorkbenchRail({ roleCompatible,items, selectedWorkstreamId, runtime, surface, globalActions, onSelectWorkstream, onPinChange, onSurfaceChange }: Pick<UnifiedWorkbenchProps, "roleCompatible" | "items" | "selectedWorkstreamId" | "runtime" | "onSelectWorkstream" | "onPinChange" | "onSurfaceChange" | "surface" | "globalActions">) {
  const [query, setQuery] = useState("");
  const [actionItemId, setActionItemId] = useState<string | null>(null);
  const activeItems = useMemo(() => items.filter((item) => item.lifecycle === "ACTIVE").sort((left, right) => Number(Boolean(right.pinned)) - Number(Boolean(left.pinned)) || (right.updatedAt ?? 0) - (left.updatedAt ?? 0)), [items]);
  const visible = useMemo(() => activeItems.filter((item) => `${item.name} ${item.projectName ?? ""} ${item.sourceLabel ?? ""}`.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase())), [activeItems, query]);
  const attentionCount = items.reduce((count, item) => count + (item.attentionCount ?? 0), 0);
  const hostCheck = runtime?.checks.find((check) => check.id === "host");
  const runtimeTitle = hostCheck?.state === "READY" ? "电脑正在运行" : hostCheck ? "电脑状态待确认" : "运行状态未读取";
  const runtimeDetail = attentionCount ? `${attentionCount} 项状态需要核对` : hostCheck?.detail ?? "打开查看运行环境";

  return <nav className="v3-workbench-rail" aria-label="工作区">
    <header>
      <p className="v3-brand-mark"><span className="v3-logo">R</span><span>AI Work Router</span></p>



    </header>
    <section className="v3-rail-pinned-list" aria-label="置顶工作">
      <h2>Bridge</h2>
      {activeItems.length > 3 && <label className="v3-rail-overflow-search">搜索工作区<input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索" /></label>}
      <div>
        {visible.map((item) => <article key={item.id} className={item.id === selectedWorkstreamId ? "selected" : ""}>
          <button type="button" className="v3-workstream-select" onClick={() => onSelectWorkstream(item.id)} onContextMenu={(event) => { if (onPinChange) { event.preventDefault(); setActionItemId(item.id); } }} onKeyDown={(event) => { if (onPinChange && ((event.shiftKey && event.key === "F10") || event.key === "ContextMenu")) { event.preventDefault(); setActionItemId(item.id); } }} aria-label={item.sourceLabel ? `${item.name} · ${item.sourceLabel}` : item.name} aria-current={item.id === selectedWorkstreamId ? "page" : undefined}>
            <strong>{item.name}</strong>
            {item.sourceLabel && <small>{item.sourceLabel}</small>}
            {item.statusLabel && <small>{item.statusLabel}</small>}
          </button>
          {onPinChange && actionItemId === item.id && <div className="v3-rail-item-menu" role="menu" aria-label={`${item.name} 操作`}><button type="button" role="menuitem" onClick={() => { onPinChange(item.id, !item.pinned); setActionItemId(null); }}>{item.pinned ? "取消置顶" : "置顶"}</button></div>}
        </article>)}
      </div>
      {!visible.length && <p className="v3-empty">还没有 Bridge。</p>}
    </section>
    <footer className="v3-rail-footer"><details className="v5-rail-advanced" open={!roleCompatible}><summary>更多入口</summary><div className="v3-rail-navigation"><button type="button" aria-current={surface === "INBOX" ? "page" : undefined} onClick={() => onSurfaceChange?.("INBOX")}>收件箱{attentionCount ? <span aria-label={`${attentionCount} 项需要处理`}>{attentionCount}</span> : null}</button><button type="button" aria-current={surface === "WORKSPACE" || surface === "DISCUSSION" || surface === "GOAL" ? "page" : undefined} onClick={() => onSurfaceChange?.("WORKSPACE")}>工作区</button><button type="button" aria-current={surface === "PROJECT_HOME" || surface === "PROJECT" || surface === "NEW_WORK" ? "page" : undefined} onClick={() => onSurfaceChange?.("PROJECT_HOME")}>项目</button><button type="button" aria-current={surface === "RUNTIME" ? "page" : undefined} onClick={() => onSurfaceChange?.("RUNTIME")}>运行环境</button></div></details><button type="button" className="v3-rail-archive-entry" aria-current={surface === "RECYCLE" ? "page" : undefined} onClick={() => onSurfaceChange?.("RECYCLE")}>归档与回收站</button><button type="button" className="v3-rail-runtime-entry" aria-current={surface === "RUNTIME" ? "page" : undefined} onClick={() => onSurfaceChange?.("RUNTIME")}><strong>●　{runtimeTitle}</strong><small>{runtimeDetail}</small></button></footer>
  </nav>;
}

function ReplyReader({ reply, onReplyRead, onReplyHandled, onAddSelectionToHandoff, onBeginHandoffSelection, onRetryCurrentChatGptHistory, onCheckNewChatGptReplies, chatGptRefreshStatus, chatGptRefreshBusy, onOpenBoundChatGptInDefaultBrowser, onOpenHostChatGptBrowserSetup, codexResultCount, onSurfaceChange, inboxPresentation = false }: Pick<UnifiedWorkbenchProps, "reply" | "onReplyRead" | "onReplyHandled" | "onAddSelectionToHandoff" | "onRetryCurrentChatGptHistory" | "onCheckNewChatGptReplies" | "chatGptRefreshStatus" | "chatGptRefreshBusy" | "onOpenBoundChatGptInDefaultBrowser" | "onOpenHostChatGptBrowserSetup" | "codexResultCount" | "onSurfaceChange"> & { onBeginHandoffSelection?: (text: string) => void; inboxPresentation?: boolean }) {
  const [selection, setSelection] = useState("");
  const readerBodyRef = useRef<HTMLDivElement>(null);
  const readerFrameRef = useRef<HTMLElement>(null);
  useLayoutEffect(() => {
    const readerBody = readerBodyRef.current;
    const readerFrame = readerFrameRef.current;
    if (!reply || !readerBody || !readerFrame || typeof window === "undefined") return;
    // Reading position is presentation state only. It is scoped to this exact
    // Router observation and deliberately never becomes a SQLite/external fact.
    const key = `ai-work-router:reply-scroll:${encodeURIComponent(reply.id)}`;
    const main = readerFrame.closest<HTMLElement>(".v3-workbench-main");
    const activeReader = () => main && main.scrollHeight > main.clientHeight + 1 ? main : readerFrame.scrollHeight > readerFrame.clientHeight + 1 ? readerFrame : readerBody;
    const persist = () => {
      try { window.sessionStorage.setItem(key, JSON.stringify({ reader: activeReader().scrollTop, window: window.scrollY })); } catch { /* Storage can be unavailable in a host WebView. */ }
    };
    const restore = () => {
      try {
        const raw = window.sessionStorage.getItem(key);
        const saved = raw ? JSON.parse(raw) as { reader?: unknown; window?: unknown } : null;
        const readerTop = Number(saved?.reader);
        const windowTop = Number(saved?.window);
        if (Number.isFinite(readerTop) && readerTop >= 0) activeReader().scrollTop = readerTop;
        if (Number.isFinite(windowTop) && windowTop >= 0 && window.scrollY !== windowTop) window.scrollTo(0, windowTop);
      } catch { /* Keep the reader usable when session storage is unavailable. */ }
    };
    restore();
    const restoreFrame = window.requestAnimationFrame(restore);
    main?.addEventListener("scroll", persist, { passive: true });
    readerBody.addEventListener("scroll", persist, { passive: true });
    readerFrame.addEventListener("scroll", persist, { passive: true });
    window.addEventListener("scroll", persist, { passive: true });
    return () => {
      window.cancelAnimationFrame(restoreFrame);
      persist();
      main?.removeEventListener("scroll", persist);
      readerBody.removeEventListener("scroll", persist);
      readerFrame.removeEventListener("scroll", persist);
      window.removeEventListener("scroll", persist);
    };
  }, [reply?.id]);
  if (!reply) return <section className="v3-reader v3-reader-empty" aria-label="完整回复"><h2>选择一个工作区</h2><p>工作区和对话选择保持独立；打开不会获取 writer，也不会改变运行状态。</p>{codexResultCount ? <button type="button" onClick={() => onSurfaceChange?.("CODEX_RESULTS")}>Codex 结果与附件 {codexResultCount}</button> : null}</section>;
  const bodyOwnsTitle = /^\s*#\s+/.test(reply.text);
  const readState = reply.readState === "UNREAD" ? "未读" : reply.readState === "HANDLED" ? "已处理" : "已读";
  const observedTime = reply.observedAt ? new Intl.DateTimeFormat("zh-CN", { hour: "2-digit", minute: "2-digit", hour12: false }).format(reply.observedAt) : "时间待确认";
  const providerLabel = reply.provider === "CHATGPT" ? "ChatGPT" : "Codex";
  const notificationStatus = reply.notificationRenderedAt
    ? "PWA 已执行通知展示（系统横幅无回执）"
    : reply.notificationState === "SENT"
      ? "Router 已提交通知；尚未收到 PWA 展示回执（不能据此确认手机横幅）"
      : reply.notificationState === "NO_SUBSCRIPTION"
        ? "本设备未订阅"
        : reply.notificationState === "FAILED"
          ? "发送未成功，回复仍保留"
          : reply.notificationState ? "等待发送" : null;
  // This only prevents Markdown from swallowing a semantic closing marker into
  // the final ordered-list item. The persisted reply and selected text stay
  // untouched; it gives the approved Reader presentation a distinct close line.
  const renderedReplyText = reply.text.replace(/\n(<\/CODEX_HANDOFF>)(?=\n|$)/g, "\n\n$1");
  const captureSelection = (event: ReactMouseEvent<HTMLElement>) => {
    const range = window.getSelection();
    const text = range?.toString().trim() ?? "";
    setSelection(event.currentTarget.contains(range?.anchorNode ?? null) ? text : "");
  };
  return <section ref={readerFrameRef} id={`reply-observation-${reply.id}`} className="v3-reader" aria-label="完整回复" tabIndex={-1} onMouseUp={captureSelection}>
    <header className="v3-reader-header"><div><p title={reply.sourceLabel ?? undefined}>{inboxPresentation ? `${providerLabel} · ${readState} · ${observedTime}` : `${providerLabel} · ${reply.sourceLabel ?? "精确来源"}`}</p>{bodyOwnsTitle && inboxPresentation ? <small className="v3-inbox-reader-work-title">{reply.title ?? "当前工作"}</small> : !bodyOwnsTitle && <><h2>{reply.title ?? "完整回复"}</h2><small>{timestamp(reply.observedAt)} · {reply.completeness === "PARTIAL" ? "部分可读" : reply.completeness === "UNAVAILABLE" ? "当前不可读" : "完整可读"}</small></>}</div><div className="v3-reply-header-actions">{reply.provider === "CHATGPT" && onCheckNewChatGptReplies ? <span><button type="button" disabled={chatGptRefreshBusy} onClick={onCheckNewChatGptReplies}>{chatGptRefreshBusy ? chatGptRefreshStatus?.includes("安全验证") ? "已暂停读取" : "正在检查…" : "检查新回复"}</button>{chatGptRefreshStatus ? <small role="status">{chatGptRefreshStatus}</small> : null}</span> : null}{(codexResultCount || (!reply.readOnly && (reply.readState === "UNREAD" || reply.readState !== "HANDLED"))) && <details className="v3-reply-actions"><summary>回复状态</summary><div>{bodyOwnsTitle && <small>{timestamp(reply.observedAt)} · {reply.completeness === "PARTIAL" ? "部分可读" : reply.completeness === "UNAVAILABLE" ? "当前不可读" : "完整可读"}</small>}{codexResultCount ? <button type="button" onClick={() => onSurfaceChange?.("CODEX_RESULTS")}>Codex 结果与附件 {codexResultCount}</button> : null}{!reply.readOnly && reply.readState === "UNREAD" && onReplyRead && <button type="button" onClick={() => onReplyRead(reply.id)}>标为已读</button>}{!reply.readOnly && reply.readState !== "HANDLED" && onReplyHandled && <button type="button" onClick={() => onReplyHandled(reply.id)}>标为已处理</button>}</div></details>}</div></header>
    <div ref={readerBodyRef} className="v3-reader-body"><p className="v3-mobile-reply-meta">{providerLabel} · {readState} · {observedTime}</p>{notificationStatus && <p className="v3-mobile-notification-status">通知状态：{notificationStatus}</p>}<MarkdownMessage text={renderedReplyText} />{reply.readOnly && reply.completeness === "UNAVAILABLE" && <aside className="v3-reader-history-recovery" role="status">{reply.historyReadFailure === "MANUAL_READ_REQUIRED" && onOpenBoundChatGptInDefaultBrowser ? <button type="button" onClick={onOpenBoundChatGptInDefaultBrowser}>在默认浏览器查看当前对话</button> : null}{reply.historyReadFailure === "AUTH_REQUIRED" && onOpenHostChatGptBrowserSetup ? <button type="button" onClick={onOpenHostChatGptBrowserSetup}>打开专用浏览器登录</button> : null}{reply.historyReadFailure !== "ACCOUNT_SECURITY_REQUIRED" && reply.historyReadFailure !== "PROFILE_IN_USE" && reply.historyReadFailure !== "MANUAL_READ_REQUIRED" && onRetryCurrentChatGptHistory ? <button type="button" onClick={onRetryCurrentChatGptHistory}>重新读取当前对话</button> : null}<small>{reply.historyReadFailure === "MANUAL_READ_REQUIRED" ? "自动读取已暂停。此入口只让你在默认浏览器人工查看；Router 不会读取、控制或导入该页面。" : reply.historyReadFailure === "ACCOUNT_SECURITY_REQUIRED" ? "请先在保留的浏览器中完成人工验证；Router 不会自动重试。" : reply.historyReadFailure === "PROFILE_IN_USE" ? "当前页面仍在使用这个 profile；Router 不会自动重试或重开页面。" : "重新读取不会发送消息。"}</small>{reply.historyReadFailure && <details><summary>技术详情</summary><small>读取状态：{reply.historyReadFailure}</small></details>}</aside>}<p className="v3-mobile-reply-end">— 完整回复结束 —</p></div>
    {!reply.readOnly && reply.provider === "CHATGPT" && onAddSelectionToHandoff && <div className="v3-selection-toolbar" aria-label="选文交接工具"><div><strong>准备交给 Codex？</strong><span>{selection ? `已取得你选中的 ${selection.length} 个字符；下一页仍会让你核对和编辑。` : "“上面”“这段”不会被自动猜测：请先选中原文，或打开范围编辑器明确填写。"}</span></div><button type="button" onClick={() => onBeginHandoffSelection?.(selection)}>{selection ? "核对这段范围，进入审阅" : "选择/编辑交给 Codex 的范围"}</button></div>}
  </section>;
}

function HandoffSelectionFocus({ item, reply, selection, onAdd, onBack }: { item?: WorkbenchItem; reply?: ExactReply | null; selection: string; onAdd?: (replyId: string, text: string) => void | Promise<void>; onBack: () => void }) {
  const [selectedText, setSelectedText] = useState(selection);
  const [saving, setSaving] = useState(false);
  useLayoutEffect(() => {
    // This focus view replaces a scrollable reader.  Keeping the reader's
    // document position leaves the range box above the viewport on desktop.
    if (!window.navigator.userAgent.toLocaleLowerCase().includes("jsdom")) window.scrollTo(0, 0);
    document.documentElement.scrollTop = 0;
    document.body.scrollTop = 0;
  }, []);
  const addSelection = () => {
    const text = selectedText.trim();
    if (!reply || !text || !onAdd || saving) return;
    setSaving(true);
    void Promise.resolve(onAdd(reply.id, text)).then(() => undefined).catch(() => setSaving(false));
  };
  const target = reply?.provider === "CODEX" ? "ChatGPT" : "Codex";
  return <section className="v3-handoff-selection-focus" aria-label="交接指令范围"><header><button type="button" onClick={onBack}>‹ 返回</button><h1>{item?.name ?? "当前工作"}</h1></header><section className="v3-handoff-selection-scroll"><span>步骤 1/3 · 确认范围</span><h2>确认这次交给 {target} 的文字</h2><p>原回复不会被改变。Router 不会从“上面”“这段”等自然语言猜测范围；只有下面明确显示的文字会进入下一步审阅。</p><label>本次交给 {target} 的指令范围<textarea aria-label="交接指令范围" value={selectedText} onChange={(event) => setSelectedText(event.target.value)} placeholder="在原回复中选中文字，或在这里粘贴/输入要交接的完整指令…" /></label><button type="button" onClick={() => setSelectedText(reply?.text ?? "")} disabled={!reply || saving}>以完整回复作为编辑起点</button><details><summary>查看原始完整回复（只读）</summary><blockquote>{reply?.text ?? "当前原回复不可读。"}</blockquote></details><small>下一步：审阅并编辑 → 批准此版本（不会发送） → 单独发送。</small></section><footer><button type="button" className="v3-primary" onClick={addSelection} disabled={!reply || !selectedText.trim() || !onAdd || saving}>{saving ? "正在准备范围…" : "确认范围，进入审阅"}</button><button type="button" onClick={onBack} disabled={saving}>取消，返回原回复</button></footer></section>;
}

function ReplyActionContext({ reply, handoff }: Pick<UnifiedWorkbenchProps, "reply" | "handoff">) {
  if (!reply || reply.readOnly) return null;
  const prepared = handoff?.status === "READY" || handoff?.status === "APPROVED";
  return <aside className="v3-reply-context" aria-label="当前交接上下文">
    <h2>{prepared ? "这条回复已进入交接审阅" : "这条回复可用于交接"}</h2>
    <p>{prepared ? "候选内容仍可编辑；批准与发送保持为两个独立操作。" : "选择正文后加入草稿，或仅在 Router 已识别候选时准备交接。"}</p>
    <small>不会自动转发；批准后才会发送。</small>
  </aside>;
}

function WorkspaceFocus({ item, reply, handoff, onOpenConnection, onNewWork, hasCodexEndpoint, codexResultCount, codexRequestCount, onSurfaceChange, mobileWorkspaceMenu, children }: { item?: WorkbenchItem; reply?: ExactReply | null; handoff?: HandoffPresentation | null; onOpenConnection: () => void; onNewWork: () => void; hasCodexEndpoint?: boolean; codexResultCount?: number; codexRequestCount?: number; onSurfaceChange?: (surface: WorkbenchSurface) => void; mobileWorkspaceMenu?: ReactNode; children: ReactNode }) {
  const compact = useCompactWorkbenchViewport();
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false);
  const closeMobileMenu = () => setMobileMenuOpen(false);
  return <section className="v3-workspace-focus" aria-label="当前工作区阅读">
    {compact ? <header className="v3-mobile-workspace-heading">
      <button type="button" onClick={() => onSurfaceChange?.("INBOX")}>‹ 收件箱</button>
      <h1>{item?.name ?? "工作区"}</h1>
      <button type="button" aria-label="更多工作区操作" aria-expanded={mobileMenuOpen} onClick={() => setMobileMenuOpen((current) => !current)}>⋯</button>
      {mobileMenuOpen && <><button type="button" className="v3-mobile-workspace-menu-backdrop" aria-label="关闭工作区操作菜单" onClick={closeMobileMenu} />
        <nav className="v3-mobile-workspace-menu" aria-label="工作区更多操作" onClick={closeMobileMenu} onKeyDown={(event) => { if (event.key === "Escape") { event.preventDefault(); closeMobileMenu(); } }}><button type="button" className="v3-mobile-workspace-menu-close" onClick={closeMobileMenu}>关闭</button><button type="button" onClick={() => onOpenConnection()}>查看两端绑定与详情</button>{hasCodexEndpoint ? <button type="button" onClick={() => onSurfaceChange?.("CODEX_HISTORY")}>Codex 对话</button> : null}{codexResultCount ? <button type="button" onClick={() => onSurfaceChange?.("CODEX_RESULTS")}>Codex 结果</button> : null}{codexRequestCount ? <button type="button" onClick={() => onSurfaceChange?.("CODEX_REQUEST")}>Codex 授权请求{codexRequestCount > 1 ? ` · ${codexRequestCount}` : ""}</button> : null}{mobileWorkspaceMenu}<button type="button" onClick={() => onSurfaceChange?.("RUNTIME")}>运行环境</button><button type="button" onClick={onNewWork}>新建工作</button></nav></>}
    </header> : <><header className="v3-workspace-heading">
      <div><h1>{item?.name ?? "工作区"}</h1><p>{item?.projectName ? `${item.projectName} 项目　/　当前工作` : "当前工作区的精确回复与交接"}</p></div>
      <div className="v3-workspace-heading-actions">{codexRequestCount ? <button type="button" className="v3-workspace-request-entry" onClick={() => onSurfaceChange?.("CODEX_REQUEST")}>Codex 授权请求{codexRequestCount > 1 ? ` · ${codexRequestCount}` : ""}</button> : null}<button type="button" className="v3-primary" onClick={onNewWork}>＋ 新建工作</button></div>
    </header>
    <div className="v3-workspace-provider-row"><nav className="v3-workspace-provider-tabs" aria-label="当前阅读来源"><span aria-current="page">{reply?.provider === "CODEX" ? "Codex 最新回复" : "ChatGPT 最新回复"}</span>{hasCodexEndpoint ? <button type="button" onClick={() => onSurfaceChange?.("CODEX_HISTORY")}>Codex 对话记录</button> : <span className="unavailable">Codex 未绑定</span>}{codexResultCount ? <button type="button" onClick={() => onSurfaceChange?.("CODEX_RESULTS")}>Codex 结果 {codexResultCount}</button> : null}</nav><button type="button" className="v3-workspace-connection" onClick={onOpenConnection}>查看两端绑定与详情</button></div></>}
    {children}
  </section>;
}

function attentionAction(kind: string) {
  switch (kind) {
    case "CHATGPT_REPLY_OBSERVED": return "查看 ChatGPT 新回复";
    case "CODEX_REPLY_OBSERVED": return "查看 Codex 新回复";
    case "CHATGPT_RESULT_READY": return "审阅 ChatGPT 结果";
    case "CODEX_RESULT_READY": return "审阅 Codex 结果";
    case "CODEX_STRUCTURED_REQUEST": return "处理 Codex 请求";
    case "HANDOFF_FAILED": return "检查交付失败";
    case "DELIVERY_UNCERTAIN": return "确认交付状态";
    case "PROVIDER_RUN_FAILED": return "查看失败执行状态";
    case "PROVIDER_RUN_CANCELLED": return "查看已取消执行状态";
    case "UNKNOWN_RUN": return "核对未知执行状态";
    case "MISSING_CHATGPT_BINDING": return "补充 ChatGPT 绑定";
    case "MISSING_CODEX_BINDING": return "补充 Codex 绑定";
    default: return "查看此 Router 事项";
  }
}

function attentionSource(kind: string) {
  switch (kind) {
    case "CHATGPT_REPLY_OBSERVED": return "ChatGPT 精确外部回复";
    case "CODEX_REPLY_OBSERVED": return "Codex 精确外部回复";
    case "CHATGPT_RESULT_READY": return "ChatGPT 保留执行结果";
    case "CODEX_RESULT_READY": return "Codex 保留执行结果";
    case "CODEX_STRUCTURED_REQUEST": return "Codex 精确结构化请求";
    case "HANDOFF_FAILED": return "失败的精确交接";
    case "DELIVERY_UNCERTAIN": return "待确认的精确交接";
    case "PROVIDER_RUN_FAILED": return "失败的精确 Provider 执行";
    case "PROVIDER_RUN_CANCELLED": return "已取消的精确 Provider 执行";
    case "UNKNOWN_RUN": return "状态未知的精确 Provider 执行";
    case "MISSING_CHATGPT_BINDING": return "当前工作区的 ChatGPT 绑定";
    case "MISSING_CODEX_BINDING": return "当前工作区的 Codex 绑定";
    default: return "Router 精确注意记录";
  }
}

function InboxFocus({ items, selectedWorkstreamId, onSelectWorkstream, onSelectAttention, onNewWork, onOpenWorkspace, onOpenDiscussion, onOpenRuntime, reply, onReplyRead, onReplyHandled, children }: Pick<UnifiedWorkbenchProps, "items" | "selectedWorkstreamId" | "onSelectWorkstream" | "onSelectAttention" | "reply" | "onReplyRead" | "onReplyHandled"> & { onNewWork: () => void; onOpenWorkspace: () => void; onOpenDiscussion: () => void; onOpenRuntime: () => void; children: ReactNode }) {
  const [selectedAttentionKey, setSelectedAttentionKey] = useState<string | null>(null);
  const attentionItems = items.filter((item) => item.lifecycle === "ACTIVE").flatMap((item) => {
    // An Inbox card is an actionable exact Router record. A legacy aggregate
    // count has no safe target, so it must never masquerade as one.
    return (item.attentionItems ?? []).filter((attention) => Boolean(attention.sourceId?.trim())).map((attention) => ({ item, attention }));
  }).sort((left, right) => left.attention.priority - right.attention.priority || (right.attention.activityAt ?? right.item.updatedAt ?? 0) - (left.attention.activityAt ?? left.item.updatedAt ?? 0));
  const unavailableAttentionCount = items.filter((item) => item.lifecycle === "ACTIVE").reduce((count, item) => {
    const exactCount = (item.attentionItems ?? []).filter((attention) => Boolean(attention.sourceId?.trim())).length;
    return count + Math.max(item.attentionCount ?? 0, (item.attentionItems ?? []).length) - exactCount;
  }, 0);
  const attentionKey = (workstreamId: string, attention: WorkbenchAttentionItem, index: number) => `${workstreamId}:${attention.sourceId ?? attention.kind}:${index}`;
  const replyIsCodex = reply?.provider === "CODEX";
  return <section className="v3-inbox-focus" aria-label="收件箱">
    <header><div><h1>收件箱</h1><p>{attentionItems.length ? `${attentionItems.length} 条具体事项等待你处理；每一条都保留自己的来源和下一步。` : unavailableAttentionCount ? "Router 报告有待处理状态，但尚未提供可安全打开的精确来源。" : "当前没有需要处理的工作；不会凭空创建收件箱项目。"}</p></div><button type="button" className="v3-primary" onClick={onNewWork}>＋ 新建工作</button></header>
    <div className="v3-inbox-grid"><section className="v3-attention-list" aria-label="待处理列表">{attentionItems.map(({ item, attention }, index) => { const key = attentionKey(item.id, attention, index); return <button key={key} type="button" className={selectedAttentionKey === key ? "selected" : ""} aria-current={selectedAttentionKey === key ? "true" : undefined} onClick={() => { setSelectedAttentionKey(key); if (onSelectAttention) onSelectAttention(item.id, attention); else onSelectWorkstream(item.id); }}><small className="v3-attention-next-action">下一步：{attentionAction(attention.kind)}</small><strong>{item.name}</strong>{item.sourceLabel && <small>{item.sourceLabel}</small>}<small className="v3-attention-source">精确来源：{attentionSource(attention.kind)} · {attention.sourceId}</small><small>{attention.message ?? "Router 尚未提供这条待办的说明。"}</small><small>{timestamp(attention.activityAt ?? item.updatedAt)} · {item.projectName ?? "未分组工作"}</small></button>; })}{unavailableAttentionCount ? <aside className="v3-reader-history-recovery" role="alert">Router 还有 {unavailableAttentionCount} 条状态未附精确来源。为避免打开错误的对话或结果，Router 没有把它们显示为可操作事项。<button type="button" onClick={onOpenRuntime}>查看运行环境并重新读取</button></aside> : null}{!attentionItems.length && !unavailableAttentionCount && <p>没有可显示的注意事项。</p>}</section><div className="v3-inbox-reader">{children}<footer className="v3-inbox-footer"><button type="button" className="v3-inbox-other-replies" onClick={onOpenWorkspace}>查看本工作区其他回复</button><div><details><summary>更多操作</summary><button type="button" disabled={!reply || reply.readOnly} onClick={() => reply && !reply.readOnly && onReplyHandled?.(reply.id)}>标为已处理</button></details><button type="button" disabled={!reply || reply.readOnly || reply.readState !== "UNREAD"} onClick={() => reply && !reply.readOnly && onReplyRead?.(reply.id)}>{reply?.readState === "UNREAD" && !reply.readOnly ? "标为已读" : "已读"}</button><button type="button" className="v3-primary" disabled={!reply} onClick={replyIsCodex ? onOpenWorkspace : onOpenDiscussion}>{replyIsCodex ? "前往 Codex 审阅" : "回复 ChatGPT"}</button></div></footer></div></div>
  </section>;
}

function useSingleDiscussionSubmission(value: string, onSendDiscussion?: (value: string) => void | Promise<void>) {
  const [sending, setSending] = useState(false);
  const sendingRef = useRef(false);
  const send = () => {
    if (!onSendDiscussion || !value.trim() || sendingRef.current) return;
    sendingRef.current = true;
    setSending(true);
    void Promise.resolve(onSendDiscussion(value)).catch(() => undefined).finally(() => {
      sendingRef.current = false;
      setSending(false);
    });
  };
  return { sending, send };
}

function ManualDiscussionDispatch({ value, onOpenManualDiscussionDestination, manualDiscussionDestinationActionLabel }: Pick<UnifiedWorkbenchProps, "onOpenManualDiscussionDestination" | "manualDiscussionDestinationActionLabel"> & { value: string }) {
  const [copyState, setCopyState] = useState<"IDLE" | "COPIED" | "UNAVAILABLE">("IDLE");
  const [destinationHref, setDestinationHref] = useState<string | null>(null);
  const [destinationError, setDestinationError] = useState(false);
  useEffect(() => {
    setCopyState("IDLE");
    setDestinationHref(null);
    setDestinationError(false);
  }, [value]);
  if (!onOpenManualDiscussionDestination) return null;
  const copy = () => {
    if (!value.trim() || !navigator.clipboard?.writeText) {
      setCopyState("UNAVAILABLE");
      return;
    }
    void navigator.clipboard.writeText(value).then(() => setCopyState("COPIED")).catch(() => setCopyState("UNAVAILABLE"));
  };
  const resolveDestination = () => {
    setDestinationError(false);
    void Promise.resolve(onOpenManualDiscussionDestination()).then((destination) => {
      if (typeof destination === "string") setDestinationHref(destination);
    }).catch(() => setDestinationError(true));
  };
  return <section className="v3-manual-discussion-dispatch" aria-label="手动发送讨论">
    <p>Router 的 ChatGPT 自动写入已暂停。草稿不会自动发送或标为送达：请复制后，在当前精确绑定的 ChatGPT 对话中自行粘贴并发送。</p>
    <div><button type="button" disabled={!value.trim()} onClick={copy}>{copyState === "COPIED" ? "已复制草稿" : "复制草稿"}</button><button type="button" disabled={!value.trim()} onClick={resolveDestination}>{manualDiscussionDestinationActionLabel ?? "在默认浏览器查看当前精确 ChatGPT 对话"}</button>{destinationHref ? <a className="v3-primary" href={destinationHref} target="_blank" rel="noreferrer">打开已核对的 ChatGPT 对话</a> : null}</div>
    {copyState === "UNAVAILABLE" ? <small role="status">浏览器未授权剪贴板；可选中草稿手动复制。</small> : null}
    {destinationError ? <small role="alert">当前精确 ChatGPT 对话无法确认；Router 没有打开或替换任何对话。</small> : null}
  </section>;
}

function DraftComposer({ draft, onDraftChange, onSendDiscussion, onOpenManualDiscussionDestination, manualDiscussionDestinationActionLabel }: Pick<UnifiedWorkbenchProps, "draft" | "onDraftChange" | "onSendDiscussion" | "onOpenManualDiscussionDestination" | "manualDiscussionDestinationActionLabel">) {
  const { sending, send } = useSingleDiscussionSubmission(draft.value, onSendDiscussion);
  const state = draft.saveState === "SAVING" ? "正在自动保存" : draft.saveState === "FAILED" ? "草稿保存失败" : draft.saveState === "SAVED" ? "草稿已自动保存" : "草稿尚未保存";
  return <section className="v3-draft-composer" aria-label="当前对话草稿"><label>继续当前精确对话<textarea value={draft.value} onChange={(event) => onDraftChange(event.target.value)} placeholder="输入讨论、修改建议或补充交接内容…" /></label><div className="v3-draft-footer"><small role={draft.saveState === "FAILED" ? "alert" : "status"}>{state}{draft.savedAt ? ` · ${timestamp(draft.savedAt)}` : ""}</small>{onSendDiscussion && <button type="button" className="v3-primary" disabled={sending || !draft.value.trim()} onClick={send}>{sending ? "正在发送讨论…" : "发送讨论（非 Handoff）"}</button>}</div>{!onSendDiscussion && <ManualDiscussionDispatch value={draft.value} onOpenManualDiscussionDestination={onOpenManualDiscussionDestination} manualDiscussionDestinationActionLabel={manualDiscussionDestinationActionLabel} />}</section>;
}

function DiscussionFocus({ item, draft, onDraftChange, onSendDiscussion, onOpenManualDiscussionDestination, manualDiscussionDestinationActionLabel, onBack, onNewWork }: Pick<UnifiedWorkbenchProps, "draft" | "onDraftChange" | "onSendDiscussion" | "onOpenManualDiscussionDestination" | "manualDiscussionDestinationActionLabel"> & { item?: WorkbenchItem; onBack: () => void; onNewWork: () => void }) {
  const { sending, send } = useSingleDiscussionSubmission(draft.value, onSendDiscussion);
  const state = draft.saveState === "SAVING" ? "正在自动保存" : draft.saveState === "FAILED" ? "草稿保存失败" : draft.saveState === "SAVED" ? "草稿已自动保存" : "草稿尚未保存";
  const savedAt = draft.savedAt ? ` · ${timestamp(draft.savedAt)}` : "";
  return <section className="v3-discussion-focus" aria-label="回复 ChatGPT">
    <header><div><h1>回复 ChatGPT</h1><p>{item?.name ?? "当前工作"}　/　只发给当前 ChatGPT 对话</p></div><button type="button" className="v3-primary" onClick={onNewWork}>＋ 新建工作</button></header>
    <section className="v3-discussion-editor" aria-label="当前对话草稿"><label>你的修改意见<textarea value={draft.value} onChange={(event) => onDraftChange(event.target.value)} placeholder="输入讨论、修改建议或补充交接内容…" /></label></section>
    <button type="button" className="v3-discussion-reader-link" onClick={onBack}>查看正在回应的完整回复</button>
    <p className={`v3-discussion-note${draft.saveState === "FAILED" ? " is-error" : ""}`}><small className="v3-visually-hidden" role={draft.saveState === "FAILED" ? "alert" : "status"}>{state}{savedAt}</small>{draft.saveState === "FAILED" ? "草稿保存失败；草稿仍保留。" : "草稿离开后保留。"}发送是否送达，与新回复是否生成分开显示。</p>{!onSendDiscussion && <ManualDiscussionDispatch value={draft.value} onOpenManualDiscussionDestination={onOpenManualDiscussionDestination} manualDiscussionDestinationActionLabel={manualDiscussionDestinationActionLabel} />}
    <footer><button type="button" onClick={onBack}>取消，保留草稿</button>{onSendDiscussion && <button type="button" className="v3-primary" disabled={sending || !draft.value.trim()} onClick={send}>{sending ? "正在发送讨论…" : "发送给 ChatGPT"}</button>}</footer>
  </section>;
}

function HandoffReview({ handoff, workstreamName, onHandoffMessageChange, onSelectHandoffAttachments, onApproveHandoff, onSendHandoff, onOpenCodexAttachments, onOpenCodexResults, onOpenChatGptOrigin, onReopenReverseHandoff, onOpenHandoffDestination, handoffDestinationActionLabel }: Pick<UnifiedWorkbenchProps, "handoff" | "onHandoffMessageChange" | "onSelectHandoffAttachments" | "onApproveHandoff" | "onSendHandoff" | "onOpenCodexAttachments" | "onOpenCodexResults" | "onOpenChatGptOrigin" | "onReopenReverseHandoff" | "onOpenHandoffDestination" | "handoffDestinationActionLabel"> & { workstreamName?: string }) {
  const [approving, setApproving] = useState(false);
  const approvingRef = useRef(false);
  const [sending, setSending] = useState(false);
  const sendingRef = useRef(false);
  const [attachmentSelection, setAttachmentSelection] = useState<string[]>([]);
  const [attachmentSaving, setAttachmentSaving] = useState(false);
  const [manualCopyStatus, setManualCopyStatus] = useState<"IDLE" | "COPIED" | "UNAVAILABLE">("IDLE");
  const compact = useCompactWorkbenchViewport();
  useEffect(() => setAttachmentSelection(handoff?.selectedAttachmentLabels ?? []), [handoff?.id, handoff?.revision, handoff?.selectedAttachmentLabels]);
  useEffect(() => {
    approvingRef.current = false;
    setApproving(false);
  }, [handoff?.id, handoff?.revision, handoff?.status]);
  useEffect(() => setManualCopyStatus("IDLE"), [handoff?.id, handoff?.revision]);
  if (!handoff) return null;
  const target = handoff.direction === "CHATGPT_TO_CODEX" ? "Codex" : "ChatGPT";
  const isReady = handoff.status === "READY";
  const isApproved = handoff.status === "APPROVED";
  const reverse = handoff.direction === "CODEX_TO_CHATGPT";
  const reverseReady = reverse && isReady;
  const reverseApproved = reverse && isApproved;
  // This capability is known before approval.  Showing it at the review step
  // avoids promising an in-Router "send" that the paused carrier cannot do.
  const manualDispatchAfterApproval = reverse && handoff.requiresManualDispatch === true;
  const requiresManualDispatch = reverseApproved && manualDispatchAfterApproval;
  const origin = handoff.origin ? `来源：${handoff.origin.provider === "CHATGPT" ? "ChatGPT" : "Codex"}${handoff.origin.observedAt ? ` · ${new Intl.DateTimeFormat("zh-CN", { hour: "2-digit", minute: "2-digit", hour12: false }).format(handoff.origin.observedAt)}` : " · 时间待确认"}${handoff.origin.manualSelectionCount ? `　含 ${handoff.origin.manualSelectionCount} 段手动补充` : ""}` : "来源信息等待 Router 确认";
  const send = () => {
    if (!onSendHandoff || !handoff.canSend || sendingRef.current) return;
    sendingRef.current = true;
    setSending(true);
    void Promise.resolve(onSendHandoff(handoff.id)).catch(() => undefined).finally(() => {
      sendingRef.current = false;
      setSending(false);
    });
  };
  const approve = () => {
    if (!onApproveHandoff || !handoff.canApprove || !handoff.message.trim() || approvingRef.current) return;
    approvingRef.current = true;
    setApproving(true);
    void Promise.resolve(onApproveHandoff(handoff.id)).catch(() => undefined).finally(() => {
      approvingRef.current = false;
      setApproving(false);
    });
  };
  const copyApprovedForManualDispatch = () => {
    if (!navigator.clipboard?.writeText) {
      setManualCopyStatus("UNAVAILABLE");
      return;
    }
    void navigator.clipboard.writeText(handoff.message).then(() => setManualCopyStatus("COPIED")).catch(() => setManualCopyStatus("UNAVAILABLE"));
  };
  return <section className={`v3-handoff-review${isApproved ? " is-approved" : ""}${reverseReady ? " is-reverse" : ""}${reverseApproved ? " is-reverse-approved" : ""}`} aria-label="人工交接审阅"><span className="v3-mobile-handoff-count">{isApproved ? requiresManualDispatch ? "步骤 3/3 · 已批准，等待你手动发送" : "步骤 3/3 · 已批准，等待单独发送" : "步骤 2/3 · 审阅并批准"}</span><header><p>{reverseReady ? "外部 Codex 回复的回传审阅" : "人工交接"}</p><h2>{isReady ? reverseReady ? manualDispatchAfterApproval ? "审阅后批准，再复制到同工作区的 ChatGPT 手动发送" : "审阅后批准，再单独发送给同工作区的 ChatGPT" : `审阅并批准后，单独发送给本工作区的 ${target}` : isApproved ? reverseApproved ? requiresManualDispatch ? "内容已批准，等待你在 ChatGPT 手动发送" : "结论和附件已锁定，尚未发送" : "内容已锁定，尚未发送" : handoff.status === "SENT" ? "已发送，执行状态独立" : "交接状态待确认"}</h2></header>
    {!reverseReady && !reverseApproved && <><p className="v3-mobile-handoff-origin">{isApproved ? <>目标：{workstreamName ?? "当前工作"} · {target}<br />批准后的内容不会被新回复覆盖。</> : origin}</p><p className="v3-contract">候选内容仅帮助编辑；批准和发送始终是两个独立动作。</p>{isReady && handoff.candidates?.length ? <div className="v3-candidate-list" aria-label="交接候选"><h3>可选候选</h3>{handoff.candidates.map((candidate) => <button key={candidate.id} type="button" onClick={() => onHandoffMessageChange?.(handoff.id, candidate.text)}><strong>{candidate.label}</strong><span>{candidate.sourceLabel ?? "来源正文"}</span><small>精确 Router 来源记录：<code>{candidate.id}</code></small></button>)}</div> : null}</>}
    {isReady && <p className="v3-handoff-flow">{manualDispatchAfterApproval ? "批准不会发送；批准后复制已批准内容，在精确 ChatGPT 对话中自行粘贴发送。" : "你现在在审阅阶段：修改内容后点击“批准此版本”；批准不会发送，下一步仍需单独确认发送。"}</p>}
    {reverseReady && <p className="v3-mobile-handoff-origin">来源：选定的精确 Codex 外部回复。下面是可编辑的转交草稿；此页尚未向 ChatGPT 发送任何内容。</p>}
    {(reverseReady || reverseApproved || (!reverse && !isReady)) && handoff.candidates?.[0] && <p className="v3-detail">精确 Router 来源记录：<code>{handoff.candidates[0].id}</code></p>}
    {reverseReady && <button type="button" className="v3-reverse-handoff-attachments" onClick={onOpenCodexAttachments}>已选附件 {handoff.selectedAttachmentLabels?.length ?? 0} 项<span>{handoff.selectedAttachmentLabels?.length ? handoff.selectedAttachmentLabels.join(" · ") : "本次只回传结论"}</span></button>}
    {reverseApproved ? <section className="v3-reverse-approved-payload" aria-label="已批准回传内容"><p>{handoff.message}</p>{handoff.selectedAttachmentLabels?.length ? <small>附件：{handoff.selectedAttachmentLabels.join("、")}</small> : null}</section> : <label className={reverseReady ? "v3-reverse-handoff-message" : undefined}>{reverseReady ? "将发送给 ChatGPT 的内容（可编辑，尚未发送）" : "将发送的内容"}<textarea aria-label="Handoff message" value={handoff.message} disabled={!isReady} onChange={(event) => onHandoffMessageChange?.(handoff.id, event.target.value)} /></label>}
    {!reverse && isReady && handoff.attachmentOptions?.length ? <section className="v3-chatgpt-handoff-attachment-selection" aria-label="ChatGPT 附件选择"><h3>本条 ChatGPT 回复的可选附件</h3><p>不会自动附带。确认后会捕获所选文件并绑定到本次审阅版本。</p>{handoff.attachmentOptions.map((filename) => <label key={filename}><input type="checkbox" checked={attachmentSelection.includes(filename)} disabled={attachmentSaving} onChange={() => setAttachmentSelection((current) => current.includes(filename) ? current.filter((item) => item !== filename) : [...current, filename])} />{filename}</label>)}<button type="button" disabled={attachmentSaving || !onSelectHandoffAttachments} onClick={() => { if (!onSelectHandoffAttachments) return; setAttachmentSaving(true); void Promise.resolve(onSelectHandoffAttachments(handoff.id, attachmentSelection)).catch(() => undefined).finally(() => setAttachmentSaving(false)); }}>{attachmentSaving ? "正在确认附件…" : "确认附件选择"}</button></section> : null}
    {!reverse && !isReady && handoff.selectedAttachmentLabels?.length ? <p className="v3-detail">已选附件：{handoff.selectedAttachmentLabels.join("、")}</p> : null}
    {handoff.destination ? <details className="v3-detail" aria-label="本次接收端精确绑定"><summary>本次接收端：{handoff.destination.provider === "CHATGPT" ? "ChatGPT" : "Codex"} 精确绑定</summary><p>{handoff.destination.label || (handoff.destination.provider === "CHATGPT" ? "ChatGPT 对话" : "Codex 对话")}</p><code>{handoff.destination.externalId}</code><small>此标识用于核对本次审阅的接收端；Router 不会按名称或最近对话改写目标。</small></details> : <p className="v3-warning">接收端精确绑定未在当前审阅中提供；不能据名称推断目标。</p>}
    {reverseReady && <button type="button" className="v3-reverse-handoff-original" onClick={onOpenCodexResults}>{handoff.origin?.sourceKind === "PROVIDER_RUN" ? "查看 Codex 完整原结果" : "查看 Codex 完整原回复"}</button>}
    {!reverse && handoff.origin?.sourceKind === "REPLY_OBSERVATION" && <button type="button" className="v3-handoff-source-link" onClick={onOpenChatGptOrigin}>查看 ChatGPT 完整原回复</button>}
    {reverseApproved && (requiresManualDispatch ? <section className="v3-reverse-approved-note" role="status"><strong>ChatGPT 自动发送已按你的安全设置暂停。</strong><p>Router 尚未把这段内容写入 ChatGPT，也不会把它标记为已送达。</p><p>下一步：复制下方已批准内容，切换到下列精确 ChatGPT 对话，核对 ID 一致后再粘贴并发送。</p>{handoff.destination?.provider === "CHATGPT" ? <><p><strong>手动发送目标：</strong>{handoff.destination.label || "ChatGPT 对话"}<br /><code>{handoff.destination.externalId}</code></p>{!compact && (handoff.destination.canonicalUrl ? <a href={handoff.destination.canonicalUrl} target="_blank" rel="noreferrer">打开已核对的 ChatGPT 对话</a> : onOpenHandoffDestination && <button type="button" onClick={() => onOpenHandoffDestination(handoff.id)}>{handoffDestinationActionLabel ?? "在默认浏览器查看这个精确 ChatGPT 对话"}</button>)}</> : null}{handoff.selectedAttachmentLabels?.length ? <p>已选附件未由 Router 上传；如需一并发送，请在目标 ChatGPT 对话中重新选择：{handoff.selectedAttachmentLabels.join("、")}。</p> : null}</section> : <p className="v3-reverse-approved-note">仅发送你刚刚批准的版本。</p>)}
    {handoff.unavailableReason && <p className="v3-warning">{handoff.unavailableReason}</p>}
    {handoff.deliveryDetail && <p className="v3-detail">交付：{handoff.deliveryDetail}</p>}
    {handoff.runStatus && <p className="v3-detail">执行：{handoff.runStatus}</p>}
    <footer>{isReady && <button type="button" className="v3-primary" disabled={approving || (!reverse && (attachmentSaving || attachmentSelection.some(name => !handoff.selectedAttachmentLabels?.includes(name)) || (handoff.selectedAttachmentLabels ?? []).some(name => !attachmentSelection.includes(name)))) || !handoff.canApprove || !handoff.message.trim()} onClick={approve}>{approving ? "正在批准此版本…" : "批准此版本（不会发送）"}</button>}{isApproved && <>{requiresManualDispatch ? <button type="button" className="v3-primary" onClick={copyApprovedForManualDispatch}>{manualCopyStatus === "COPIED" ? "已复制：去当前 ChatGPT 对话粘贴并发送" : "复制已批准内容，手动发送到 ChatGPT"}</button> : <button type="button" className="v3-primary" disabled={sending || !handoff.canSend} onClick={send}>{sending ? `正在发送给 ${target}…` : `单独发送给 ${target}`}</button>}{requiresManualDispatch && compact && handoff.destination?.provider === "CHATGPT" && (handoff.destination.canonicalUrl ? <a className="v3-primary" href={handoff.destination.canonicalUrl} target="_blank" rel="noreferrer">打开已核对的 ChatGPT 对话</a> : onOpenHandoffDestination && <button type="button" onClick={() => onOpenHandoffDestination(handoff.id)}>{handoffDestinationActionLabel ?? "核对并显示精确 ChatGPT 链接"}</button>)}{requiresManualDispatch && manualCopyStatus === "UNAVAILABLE" && <p className="v3-warning">浏览器未授权剪贴板；上方完整内容可长按或选中复制。</p>}{reverseApproved && onReopenReverseHandoff && <button type="button" className="v3-reverse-approved-reopen" onClick={onReopenReverseHandoff}>返回编辑（需重新批准）</button>}</>}</footer>
  </section>;
}

function HandoffReviewFocus({ handoff, workstreamName, onBack, onOpenStatus, onReopenReverseHandoff, children }: { handoff?: HandoffPresentation | null; workstreamName?: string; onBack: () => void; onOpenStatus: () => void; onReopenReverseHandoff?: () => void; children: ReactNode }) {
  const target = handoff?.direction === "CODEX_TO_CHATGPT" ? "ChatGPT" : "Codex";
  const manualDispatchAfterApproval = handoff?.direction === "CODEX_TO_CHATGPT" && handoff.requiresManualDispatch === true;
  const title = handoff?.status === "APPROVED" ? manualDispatchAfterApproval ? "内容已批准，等待你手动发送" : "内容已锁定，等待发送" : handoff?.status === "READY" ? manualDispatchAfterApproval ? "审阅后批准，再复制到 ChatGPT 手动发送" : `审阅并批准后，单独发送给 ${target}` : "交接状态待确认";
  const compact = useCompactWorkbenchViewport();
  const returnLabel = handoff?.status === "APPROVED" ? handoff.direction === "CHATGPT_TO_CODEX" ? "返回原回复，另建审阅版本" : "返回编辑（需重新批准）" : compact && handoff?.status === "READY" ? "保存草稿，稍后处理" : "返回阅读";
  const hasDeliveryStatus = handoff?.status === "SENT" || handoff?.status === "SENDING" || handoff?.status === "FAILED";
  const deliveryStatusLabel = handoff?.status === "SENT" ? "查看交付与执行" : "查看送达状态";
  const reverseReady = handoff?.direction === "CODEX_TO_CHATGPT" && handoff.status === "READY";
  const reverseApproved = handoff?.direction === "CODEX_TO_CHATGPT" && handoff.status === "APPROVED";
  return <section className={`v3-handoff-review-focus${reverseReady ? " is-reverse" : ""}${reverseApproved ? " is-reverse-approved" : ""}`} aria-label="交接审阅页面"><header><button type="button" onClick={onBack}>‹ 返回</button><h1>{compact ? workstreamName ?? "当前工作" : title}</h1></header>{children}{!reverseReady && !reverseApproved && <footer className="v3-handoff-review-return">{hasDeliveryStatus ? <button type="button" className="v3-primary" onClick={onOpenStatus}>{deliveryStatusLabel}</button> : <button type="button" onClick={onBack}>{returnLabel}</button>}</footer>}</section>;
}

function GoalControls({ goal, onGoalAction }: Pick<UnifiedWorkbenchProps, "goal" | "onGoalAction">) {
  const [confirmation, setConfirmation] = useState<"PAUSE" | "RESUME" | "DELETE" | "STOP_TURN" | null>(null);
  const [showMobileUnavailable, setShowMobileUnavailable] = useState(false);
  const trigger = useRef<HTMLButtonElement | null>(null);
  const cancel = useRef<HTMLButtonElement | null>(null);
  useEffect(() => { if (confirmation) cancel.current?.focus(); else trigger.current?.focus(); }, [confirmation]);
  if (!goal) return null;
  const statusLabel: Record<GoalPresentation["status"], string> = { NONE: "当前没有 Goal", ACTIVE: "目标进行中", PAUSED: "目标已暂停", BLOCKED: "目标受阻", LIMITED: "目标受限", COMPLETE: "目标已完成", UNKNOWN: "目标状态待确认" };
  const actionLabel = confirmation === "PAUSE" ? "暂停目标" : confirmation === "RESUME" ? "继续目标" : confirmation === "DELETE" ? "删除目标" : "停止本轮执行";
  const actions: Array<["PAUSE" | "RESUME" | "DELETE" | "STOP_TURN", string]> = goal.status === "PAUSED" ? [["RESUME", "继续目标"], ["STOP_TURN", "停止本轮执行"], ["DELETE", "删除目标…"]] : [["PAUSE", "暂停目标"], ["STOP_TURN", "停止本轮执行"], ["DELETE", "删除目标…"]];
  const unavailableActions = actions.filter(([action]) => !goal.controllableActions.includes(action)).map(([, label]) => label.replace("…", ""));
  const unavailableDetail = unavailableActions.length ? `当前无法控制：${unavailableActions.join("、")}` : "当前所有适用控制均已获得官方控制权";
  const unavailableNote = unavailableActions.length ? (goal.unavailableReason ?? "Router 尚未取得对应官方控制权；状态会在下一次正式读取后更新。") : "Router 已确认当前对话可控制。";
  const unavailableReasonId = "goal-control-unavailable-reason";
  return <section className="v3-goal-controls" aria-label="Goal 控制"><header><h2>{goal.text ?? "当前没有 Goal"}</h2><span>目标状态与本轮执行分别显示；控制提交后等待官方确认。</span></header><div className="v3-goal-status-grid"><div><strong>{statusLabel[goal.status]}</strong><p>{goal.usageLabel ? `Codex 报告 ${goal.usageLabel}` : "用量或时间尚未报告"}</p><small>读取：{timestamp(goal.readAt)}</small></div><div><strong>{goal.activeTurnId ? "本轮正在执行" : "本轮未见可控执行"}</strong><p>{goal.activeTurnId ? "本轮正在执行 · 独立状态" : "本轮未见可控执行 · 独立状态"}</p><small>{goal.activeTurnId ? "Router 已观察到精确 Turn" : "没有可确认的 Turn 控制权"}</small></div></div>{goal.unavailableReason && <p className="v3-warning">{goal.unavailableReason}</p>}<p id={unavailableReasonId} className="v3-visually-hidden">{unavailableNote}</p><div className="v3-control-row">{actions.map(([action, label]) => <button key={action} type="button" disabled={!goal.controllableActions.includes(action)} aria-describedby={!goal.controllableActions.includes(action) ? unavailableReasonId : undefined} onClick={(event) => { trigger.current = event.currentTarget; setConfirmation(action); }}>{label}</button>)}<button type="button" className="v3-goal-mobile-unavailable-trigger" aria-expanded={showMobileUnavailable} onClick={() => setShowMobileUnavailable((current) => !current)}>控制不可用状态</button></div>{showMobileUnavailable && <aside className="v3-goal-mobile-unavailable-details" role="status" aria-label="控制不可用状态详情"><strong>{unavailableDetail}</strong><p>{unavailableNote}</p></aside>}<p className="v3-goal-worker-note">Workers 由这个主对话汇总，不在 Router 里生成额外绑定。</p>{confirmation && <div className="v3-confirmation" role="alertdialog" aria-modal="true" aria-label={`${actionLabel} 确认`} onKeyDown={(event) => { if (event.key === "Escape") { event.preventDefault(); setConfirmation(null); } else keepFocusWithinDialog(event); }}><p><strong>确认{actionLabel}？</strong>{confirmation === "DELETE" ? " 仅清除 Goal，不删除聊天、工作区、项目或文件。" : confirmation === "STOP_TURN" ? " 只中断当前准确本轮，不会回滚已完成的修改。" : " 将使用当前 owner 的官方控制语义。"}</p><button ref={cancel} type="button" onClick={() => setConfirmation(null)}>取消</button><button type="button" className="v3-danger" onClick={() => { onGoalAction?.(goal.threadId, confirmation); setConfirmation(null); }}>确认{actionLabel}</button></div>}</section>;
}

function GoalFocus({ children, onBack, goal }: { children: ReactNode; onBack: () => void; goal?: GoalPresentation | null }) {
  const [showUnavailable, setShowUnavailable] = useState(false);
  const applicableActions = goal?.status === "PAUSED" ? ["RESUME", "STOP_TURN", "DELETE"] : goal ? ["PAUSE", "STOP_TURN", "DELETE"] : [];
  const unavailableActions = applicableActions.filter((action) => !goal?.controllableActions.includes(action as "PAUSE" | "RESUME" | "DELETE" | "STOP_TURN"));
  const actionLabels: Record<string, string> = { PAUSE: "暂停目标", RESUME: "继续目标", DELETE: "删除目标", STOP_TURN: "停止本轮执行" };
  const controlDetail = unavailableActions.length ? `当前无法控制：${unavailableActions.map((action) => actionLabels[action]).join("、")}` : "当前所有适用控制均已获得官方控制权";
  const controlNote = unavailableActions.length ? (goal?.unavailableReason ?? "Router 尚未取得对应官方控制权；状态会在下一次正式读取后更新。") : "Router 已确认当前对话可控制。";
  return <section className="v3-utility-focus"><header className="v3-utility-heading"><button className="v3-mobile-goal-back" type="button" onClick={onBack}>‹ 返回</button><h1 className="v3-goal-desktop-title">当前目标与执行</h1><h1 className="v3-goal-mobile-title">当前 Goal</h1><p>目标状态与本轮执行分别显示；点击控制后等待 provider 确认，不先改成成功。</p></header>{children}<p className="v3-goal-mobile-control-note">{controlNote}</p><footer className="v3-goal-footer"><button type="button" onClick={onBack}>返回 Codex</button><button type="button" className="v3-goal-unavailable-trigger" aria-expanded={showUnavailable} onClick={() => setShowUnavailable((current) => !current)}>控制不可用状态</button></footer>{showUnavailable && <aside className="v3-goal-unavailable-details" role="status" aria-label="控制不可用状态详情"><strong>{controlDetail}</strong><p>{controlNote}</p></aside>}</section>;
}

function RuntimeFocus({ children, onNewWork }: { children: ReactNode; onNewWork: () => void }) {
  return <section className="v3-utility-focus"><header className="v3-utility-heading"><div><h1>运行环境</h1><p>所有启动、关闭与连接检查，都在这里。</p></div><button type="button" className="v3-primary" onClick={onNewWork}>＋ 新建工作</button></header>{children}</section>;
}

function ProviderRunStatusFocus({ children, onBack }: { children: ReactNode; onBack: () => void }) {
  return <section className="v3-utility-focus" aria-label="精确 Provider 执行状态"><header className="v3-utility-heading"><div><h1>这一次执行的状态</h1><p>只显示收件箱指定的精确 Router 执行；不会以其他结果替代，也不会自动重试或发送。</p></div><button type="button" onClick={onBack}>‹ 返回收件箱</button></header>{children}</section>;
}

function InstallFocus({ onNewWork, onBack }: { onNewWork: () => void; onBack: () => void }) {
  return <section className="v3-install-focus" aria-label="安装与更新"><header><div><h1>安装与更新</h1><p>当前安装状态尚未接入官方读取；不会从历史安装记录推断实时状态。</p></div><button type="button" className="v3-primary" onClick={onNewWork}>＋ 新建工作</button></header><section className="v3-install-card"><span>Windows · 本地优先</span><h2>把工作环境一次准备好</h2><p>安装 Router 和必要组件，之后都从一个窗口管理。</p><ol><li><span className="v3-install-step-number">1</span><strong>安装必要组件</strong><small>使用受支持的安装和分发方式</small></li><li><span className="v3-install-step-number">2</span><strong>保留现有资料</strong><small>不覆盖登录资料、Push 密钥与已保存工作</small></li><li><span className="v3-install-step-number">3</span><strong>完成首次连接</strong><small>需要的账号登录与手机授权由你确认</small></li></ol></section><label className="v3-install-option"><input type="checkbox" disabled />随 Windows 登录启动（可稍后在设置修改）</label><footer><button type="button" onClick={onBack}>已有安装？查看当前运行环境</button><span aria-label="开始安装当前不可用">开始安装当前不可用</span></footer></section>;
}

function RecycleFocus({ items, selectedWorkstreamId, lifecycle, onSelectRecycleWorkstream, onLifecycleChange, onNewWork, onBack, onOpenCleanup, onOpenPurge }: Pick<UnifiedWorkbenchProps, "items" | "selectedWorkstreamId" | "lifecycle" | "onSelectRecycleWorkstream" | "onLifecycleChange"> & { onNewWork: () => void; onBack: () => void; onOpenCleanup: () => void; onOpenPurge: () => void }) {
  const [view, setView] = useState<"TRASHED" | "ARCHIVED">("TRASHED");
  const visible = items.filter((item) => item.lifecycle === view);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  useEffect(() => { setSelectedId((current) => visible.some((item) => item.id === current) ? current : visible[0]?.id ?? null); }, [view, items]);
  const selected = visible.find((item) => item.id === selectedId) ?? null;
  return <section className="v3-recycle-focus" aria-label="归档与回收站">
    <header><div><h1>归档与回收站</h1><p>移走不等于永久删除；原始 ChatGPT / Codex 对话不受影响。</p></div><button type="button" className="v3-primary" onClick={onNewWork}>＋ 新建工作</button></header>
    <nav aria-label="归档与回收视图"><button type="button" className={view === "TRASHED" ? "selected" : ""} onClick={() => setView("TRASHED")}>回收站　{items.filter((item) => item.lifecycle === "TRASHED").length}</button><button type="button" className={view === "ARCHIVED" ? "selected" : ""} onClick={() => setView("ARCHIVED")}>已归档</button></nav>
    <section className="v3-recycle-list" aria-label={view === "TRASHED" ? "回收站条目" : "已归档条目"}>{visible.map((item) => <article key={item.id} className={item.id === selectedId ? "selected" : ""} onClick={() => { setSelectedId(item.id); onSelectRecycleWorkstream?.(item.id); }}><div><h2>{item.id === selectedId ? "✓　" : ""}{item.name}</h2><p>{timestamp(item.updatedAt)} {view === "TRASHED" ? "移入" : "归档"} · {item.statusLabel ?? "状态待确认"}</p></div><button type="button" onClick={(event) => { event.stopPropagation(); onLifecycleChange?.(item.id, "ACTIVE"); }}>恢复</button></article>)}{!visible.length && <p>此视图没有可恢复的工作区。</p>}</section>
    <p className="v3-recycle-note">不会自动清空。永久清除需要单独确认。</p>
    <button type="button" className="v3-recycle-cleanup" onClick={onOpenCleanup}>整理测试工作区</button>
    <footer><button type="button" className="v3-recycle-return" onClick={onBack}>返回工作区</button><div className="v3-recycle-confirm"><span>{selected && selected.id === selectedWorkstreamId && lifecycle?.canPurge ? "确认页会再次说明永久清除的范围。" : lifecycle?.destructiveReason ?? "选择当前精确工作区后，才会检查是否允许永久清除。"}</span><button type="button" disabled={view !== "TRASHED" || !selected || selected.id !== selectedWorkstreamId || !lifecycle?.canPurge} onClick={onOpenPurge}>永久清除选中项</button></div></footer>
  </section>;
}

function PurgeConfirmFocus({ item, lifecycle, onPurgeTrashedWorkstream, onBack }: Pick<UnifiedWorkbenchProps, "lifecycle" | "onPurgeTrashedWorkstream"> & { item?: WorkbenchItem; onBack: () => void }) {
  const canPurge = Boolean(item && item.lifecycle === "TRASHED" && lifecycle?.canPurge && onPurgeTrashedWorkstream);
  const detail = lifecycle?.destructiveReason ?? "有活跃执行、未知执行状态或未确认交付时，Core 会拒绝永久清除。";
  return <section className="v3-purge-confirm-focus" aria-label="永久清除前的最后确认">
    <header><h1>永久清除前的最后确认</h1><button type="button" onClick={onBack}>‹ 返回工作区</button></header>
    <span className="v3-purge-local-badge">仅清除 Router 本地记录</span>
    <section className="v3-purge-confirm-card"><h2>永久清除“{item?.name ?? "当前工作区"}”？</h2><p>将移除：该工作区的本地保留内容与草稿。</p><p>不影响：ChatGPT / Codex 的原始对话、外部项目和磁盘文件。</p><p>{detail}</p><small>恢复依赖之前的备份，不再从回收站恢复。</small></section>
    <footer><button type="button" onClick={onBack}>取消，保留在回收站</button><button type="button" className="v3-danger" disabled={!canPurge} onClick={() => { if (item) onPurgeTrashedWorkstream?.(item.id); }}>确认永久清除</button></footer>
    {!canPurge && <p className="v3-purge-unavailable" role="status">当前不能永久清除：{detail}</p>}
  </section>;
}

function TestCleanupFocus({ onNewWork, onBack, onCreateVerifiedBackup }: Pick<UnifiedWorkbenchProps, "onCreateVerifiedBackup"> & { onNewWork: () => void; onBack: () => void }) {
  return <section className="v3-test-cleanup-focus" aria-label="整理测试工作区"><header><div><h1>整理测试工作区</h1><p>先核对来源，再移动；不根据名字相同就批量删除。</p></div><button type="button" className="v3-primary" onClick={onNewWork}>＋ 新建工作</button></header><section className="v3-test-cleanup-card"><span>当前没有已核验的测试清单</span><h2>尚未选择可安全移走的记录</h2><ul><li>需要明确验收来源与精确工作区 ID</li><li>需要确认没有活跃执行或送达未确认的交接</li><li>需要准备本地数据库一致性备份</li></ul><aside><h3>不会移动任何记录</h3><p>Router 尚未取得经批准的真实测试记录清单；不会以名称、日期或状态猜测批量目标。</p></aside></section><p className="v3-test-cleanup-note">移入回收站后退出默认列表与提醒范围，可随时恢复；不会取消外部任务。</p><footer><button type="button" onClick={onBack}>取消，不做任何修改</button>{onCreateVerifiedBackup ? <button type="button" className="v3-primary" onClick={onCreateVerifiedBackup}>创建一致性备份</button> : <span aria-label="一致性备份当前不可用">一致性备份当前不可用</span>}<p role="status">核验清单缺失，不能备份并移入回收站。</p></footer></section>;
}

const NORMAL_CHROME_CONNECTOR_DIRECTORY = "%LOCALAPPDATA%\\AIWorkRouter\\connector-bundles\\AI Work Router Connector 20260929-4";

function ChatGptDiagnostics({ runtime, onBack }: Pick<UnifiedWorkbenchProps, "runtime"> & { onBack: () => void }) {
  const browser = runtime?.checks.find((check) => check.id === "browser-runtime");
  const provider = runtime?.checks.find((check) => check.id === "chatgpt");
  const needsAttention = [browser, provider].filter((check) => check && check.state !== "READY");
  const status = needsAttention.length ? `当前问题：${needsAttention.map((check) => check!.label).join(" / ")}` : "当前连接：精确页面观察已就绪";
  const browserDetail = browser?.detail ?? "尚未读取";
  const providerDetail = provider?.detail ?? "尚未读取";
  const [pathCopied, setPathCopied] = useState(false);
  const connectorNeedsLoading = browser?.state !== "READY";
  const copyConnectorDirectory = async () => {
    try {
      await navigator.clipboard?.writeText(NORMAL_CHROME_CONNECTOR_DIRECTORY);
      setPathCopied(true);
    } catch {
      setPathCopied(false);
    }
  };
  useEffect(() => { const main = document.querySelector<HTMLElement>(".v3-workbench-main"); if (main && typeof main.scrollTo === "function") main.scrollTo({ top: 0 }); }, []);
  return <section className="v3-chatgpt-diagnostics" aria-label="ChatGPT 连接诊断">
    <header><h1>ChatGPT 查看与回复</h1><button type="button" onClick={onBack}>‹ 返回工作区</button></header>
    <span className="v3-diagnostic-badge">{status}</span>
    <section className="v3-diagnostic-card">
      <h2>使用你日常 Chrome 的当前精确对话</h2>
      <ol className="v3-diagnostic-facts">
        <li>Connector 状态：{browserDetail}</li>
        <li>新回复观察：{providerDetail}</li>
        <li>Connector 只观察已打开且 conversation ID 精确匹配的页面；不会新开、导航、刷新、读取 Cookie/存储。</li>
        <li>对 ChatGPT 的一次文字发送只能来自精确绑定对话的 Review → Edit → Approve → Send；打开链接或复制文字不等于送达。</li>
      </ol>
      {connectorNeedsLoading && <section className="v3-connector-setup" aria-labelledby="connector-setup-title">
        <div><span>下一步</span><h3 id="connector-setup-title">在电脑 Chrome 核对当前 Connector</h3></div>
        <ol>
          <li>在 Chrome 地址栏输入 <code>chrome://extensions</code>。</li>
          <li>如果已看到 <strong>AI Work Router — ChatGPT Connector</strong>：确认它已启用，然后点击一次“重新加载”。</li>
          <li>只有找不到它时，才打开右上角的“开发者模式”并点“加载已解压的扩展程序”。</li>
          <li>加载时选择下面的<strong>文件夹本身</strong>，不要选择 <code>manifest.json</code>；再回到这里点击“检查 Connector 状态”。</li>
        </ol>
        <p>这些步骤不会刷新、打开或控制 ChatGPT 对话。</p>
        <div className="v3-connector-path"><code>{NORMAL_CHROME_CONNECTOR_DIRECTORY}</code><button type="button" onClick={() => void copyConnectorDirectory()}>{pathCopied ? "已复制路径" : "复制加载路径"}</button></div>
      </section>}
    </section>
    <p>若 ChatGPT 显示真人验证，请先在同一 Chrome 标签由你完成验证，再回到 Router 检查 Connector 状态；Router 不会刷新、重开或绕过验证。</p>
    <footer><button type="button" onClick={runtime?.onOpenDiagnostics}>重新检查 Router 运行状态</button>{browser?.secondaryAction ? <button type="button" onClick={browser.secondaryAction}>{browser.secondaryActionLabel ?? "在默认浏览器查看当前对话"}</button> : null}{browser?.action ? <button type="button" className="v3-primary" onClick={browser.action}>{browser.actionLabel ?? "检查 Connector 状态"}</button> : <span aria-label="Connector 当前已连接或不可用">Connector 当前已连接或不可用</span>}</footer>
  </section>;
}

function HandoffStatusFocus({ handoff, workstreamName, onBack, onOpenInbox, onRefreshHandoffStatus, onOpenHandoffDestination, handoffDestinationActionLabel }: Pick<UnifiedWorkbenchProps, "handoff" | "onRefreshHandoffStatus" | "onOpenHandoffDestination" | "handoffDestinationActionLabel"> & { workstreamName?: string; onBack: () => void; onOpenInbox: () => void }) {
  const [showPayload, setShowPayload] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [copiedForManualDispatch, setCopiedForManualDispatch] = useState(false);
  const target = handoff?.direction === "CODEX_TO_CHATGPT" ? "ChatGPT" : "Codex";
  const sent = handoff?.status === "SENT";
  const compact = useCompactWorkbenchViewport();
  const mobileSending = compact && handoff?.status === "SENDING";
  const prewriteCodes = new Set(["CHATGPT_AUTH_OR_COMPOSER_REQUIRED", "CHATGPT_ACCOUNT_SECURITY_REQUIRED", "CHATGPT_CONVERSATION_ID_INVALID", "CHATGPT_CONVERSATION_NOT_OPEN", "CHATGPT_EXACT_IDENTITY_MISMATCH", "CHATGPT_GENERATING", "CHATGPT_HUMAN_DRAFT_PRESENT", "CHATGPT_ROUTER_PROFILE_IN_USE", "CHATGPT_ROUTER_PROFILE_REQUIRED"]);
  const manualDispatchPending = handoff?.status === "APPROVED" && handoff.direction === "CODEX_TO_CHATGPT" && handoff.requiresManualDispatch === true;
  const manualDispatchAvailable = manualDispatchPending || (handoff?.status === "FAILED" && handoff.direction === "CODEX_TO_CHATGPT" && prewriteCodes.has(handoff.deliveryCode ?? ""));
  const mobileManualDispatch = compact && manualDispatchAvailable;
  const destinationActionLabel = compact ? "核对并显示精确 ChatGPT 链接" : handoffDestinationActionLabel ?? "在默认浏览器查看这个精确 ChatGPT 对话";
  const deliveryTitle = sent ? `已送达 ${target}` : handoff?.status === "SENDING" ? "正在确认送达" : manualDispatchPending ? "等待你手动发送" : manualDispatchAvailable ? "未写入 ChatGPT" : handoff?.status === "FAILED" ? "未确认送达" : "交付状态待确认";
  const executionTitle = sent ? (handoff?.runStatus?.includes("执行") ? "正在执行" : "执行状态待确认") : "尚未可确认";
  const deliveryCopy = sent ? "批准后的指令完整发送。" : manualDispatchPending ? "Router 已保留批准内容，但按安全设置没有写入 ChatGPT。" : handoff?.deliveryDetail ?? "Router 尚未获得可显示为已送达的精确接受证据。";
  const refresh = () => {
    if (!handoff || !onRefreshHandoffStatus || refreshing) return;
    setRefreshing(true);
    void Promise.resolve(onRefreshHandoffStatus(handoff.id)).catch(() => undefined).finally(() => setRefreshing(false));
  };
  const copyForManualDispatch = () => {
    if (!handoff || !navigator.clipboard?.writeText) return;
    void navigator.clipboard.writeText(handoff.message).then(() => setCopiedForManualDispatch(true)).catch(() => undefined);
  };
  useEffect(() => { const main = document.querySelector<HTMLElement>(".v3-workbench-main"); if (main && typeof main.scrollTo === "function") main.scrollTo({ top: 0 }); }, []);
  return <section className={`v3-handoff-status-focus${compact && sent ? " is-mobile-sent" : ""}${mobileSending ? " is-mobile-sending" : ""}${mobileManualDispatch ? " is-mobile-manual-dispatch" : ""}`} aria-label="交付与执行状态">
    <header><h1>{compact && (sent || mobileSending) ? workstreamName ?? "当前工作" : sent ? `交接已送达，${target} 正在执行` : manualDispatchPending ? "内容已批准，等待你手动发送" : manualDispatchAvailable ? "尚未发送到 ChatGPT" : handoff?.status === "SENDING" ? "正在确认交接送达" : "交付状态待确认"}</h1><button type="button" onClick={compact && (sent || mobileSending) ? onOpenInbox : onBack}>{compact && (sent || mobileSending) ? "‹ 收件箱" : "‹ 返回工作区"}</button></header>
    <span className={sent ? "v3-handoff-status-badge sent" : "v3-handoff-status-badge"}>{sent ? "交接已送达" : manualDispatchPending ? "等待你手动发送" : manualDispatchAvailable ? "未写入 ChatGPT" : handoff?.status === "FAILED" ? "发送未确认" : "正在确认"}</span>
    {sent && <h2 className="v3-mobile-sent-title">{target} 已收到指令</h2>}
    {mobileSending && <><h2 className="v3-mobile-sending-title">请求已提交，<br />暂时不能确认对端是否收到</h2><p className="v3-mobile-sending-explanation">不要重新发送。已批准的内容已保留，刷新只查询这次交接的状态。</p></>}
    {manualDispatchAvailable && <p className="v3-mobile-sending-explanation">{manualDispatchPending ? "ChatGPT 自动发送已按安全设置暂停。Router 没有写入 ChatGPT，也不会把它标记为已送达。" : "Router 在写入输入框前已停止；没有内容发送到 ChatGPT。"}你可以复制这份已批准内容，再由你本人粘贴并发送到当前绑定对话。</p>}
    {handoff?.unavailableReason && <p className="v3-warning" role="alert">{handoff.unavailableReason}</p>}
    {manualDispatchAvailable && handoff?.destination?.provider === "CHATGPT" && <section className="v3-handoff-status-payload" aria-label="手动发送的精确目标"><h2>手动发送目标</h2><p>{handoff.destination.label || "ChatGPT 对话"}</p><code>{handoff.destination.externalId}</code><small>切换到 ChatGPT 后，先核对这个 ID 一致；Router 不会按名称或最近对话替换目标。</small></section>}
    {handoff?.handoffId && <details className="v3-detail" aria-label="精确交付记录"><summary>本页对应的精确 Router 交付记录</summary><code>{handoff.handoffId}</code><small>这是收件箱所选记录；不会以另一条、较新的交付记录替代。</small></details>}
    <div className="v3-handoff-status-cards"><section>{!mobileSending && <p>这次交接</p>}<h2>{deliveryTitle}</h2>{mobileSending ? <><h2>对端执行：状态尚不可确认</h2><span>连接中断不等于发送失败。</span></> : <span>{deliveryCopy}</span>}</section>{!mobileSending && <section><p>{target} 的执行</p><h2>{executionTitle}</h2><span>{sent ? handoff?.runStatus ?? "交接送达与任务完成是两件事。" : "交付未确认前，不把接收端显示成正在执行。"}</span></section>}</div>
    <p className="v3-handoff-status-note">交接送达和任务完成是两件事。这里不会把“已发送”显示成“已完成”。</p>
    {showPayload && <section className="v3-handoff-status-payload" aria-label={sent ? "已发送内容" : "已批准内容"}><h2>{sent ? "已发送内容" : "已批准的完整内容"}</h2><p>{handoff?.message || "没有可显示的已批准内容。"}</p></section>}
    <footer>{mobileSending ? <><button type="button" className="v3-primary" disabled={refreshing || !onRefreshHandoffStatus} onClick={refresh}>{refreshing ? "正在刷新送达状态…" : "刷新送达状态"}</button><button type="button" onClick={onOpenInbox}>返回收件箱，稍后查看</button></> : <><button type="button" aria-expanded={showPayload} onClick={() => setShowPayload((current) => !current)}>{sent ? "查看已发送的完整内容" : "查看已批准的完整内容"}</button>{manualDispatchAvailable && <button type="button" className="v3-primary" onClick={copyForManualDispatch}>{copiedForManualDispatch ? "已复制：请到绑定 ChatGPT 对话手动发送" : "复制已批准内容，手动发送到 ChatGPT"}</button>}{manualDispatchAvailable && handoff?.destination?.canonicalUrl ? <a href={handoff.destination.canonicalUrl} target="_blank" rel="noreferrer">打开已核对的 ChatGPT 对话</a> : manualDispatchAvailable && onOpenHandoffDestination && <button type="button" onClick={() => handoff && onOpenHandoffDestination(handoff.id)}>{destinationActionLabel}</button>}<button type="button" onClick={onOpenInbox}>返回收件箱</button></>}</footer>
  </section>;
}

function ExitImpactFocus({ onBack, onOpenGoal, onHideToTray }: Pick<UnifiedWorkbenchProps, "onHideToTray"> & { onBack: () => void; onOpenGoal: () => void }) {
  useEffect(() => { const main = document.querySelector<HTMLElement>(".v3-workbench-main"); if (main && typeof main.scrollTo === "function") main.scrollTo({ top: 0 }); }, []);
  return <section className="v3-exit-impact" aria-label="退出全部前确认影响"><header><h1>退出全部前确认影响</h1><button type="button" onClick={onBack}>‹ 返回工作区</button></header><span className="v3-exit-impact-badge">关闭窗口与退出程序不同</span><section className="v3-exit-impact-card"><h2>退出后，手机将无法收到新回复提醒</h2><p><strong>关闭窗口：</strong>继续在托盘运行。</p><p><strong>退出程序：</strong>停止本机 Router 服务与它拥有的观察连接。</p><small>如 Router-owned 工作正在执行，会按其执行状态完成收束；<br />不会终止其他应用、Codex 对话或外部任务。</small></section><footer>{onHideToTray ? <button type="button" className="v3-primary" onClick={onHideToTray}>留在托盘运行</button> : <button type="button" className="v3-primary" disabled>留在托盘运行</button>}<button type="button" onClick={onOpenGoal}>查看当前执行，再退出</button></footer>{!onHideToTray && <p className="v3-exit-impact-unavailable" role="status">当前设备无法隐藏桌面窗口到托盘；不会把它替换为退出程序。</p>}</section>;
}

function LifecycleAndRuntime({ selectedWorkstreamId, lifecycle, runtime, onLifecycleChange, onPurgeTrashedWorkstream, onCreateVerifiedBackup, onOpenInstall }: Pick<UnifiedWorkbenchProps, "selectedWorkstreamId" | "lifecycle" | "runtime" | "onLifecycleChange" | "onPurgeTrashedWorkstream" | "onCreateVerifiedBackup"> & { onOpenInstall: () => void }) {
  const [purgeConfirmation, setPurgeConfirmation] = useState("");
  const purgeTrigger = useRef<HTMLButtonElement | null>(null);
  const purgeCancel = useRef<HTMLButtonElement | null>(null);
  useEffect(() => { if (purgeConfirmation) purgeCancel.current?.focus(); else purgeTrigger.current?.focus(); }, [purgeConfirmation]);
  const purgeReady = purgeConfirmation === "PURGE_LOCAL_WORKSTREAM";
  const attention = runtime?.checks.find((check) => check.state === "WARNING" || check.state === "UNAVAILABLE");
  const primaryAction = attention?.action;
  const primaryActionLabel = attention?.actionLabel ?? "检查状态";
  return <section className="v3-runtime-stack" aria-label="工作区状态与运行环境">{attention && <section className="v3-runtime-alert" aria-label="当前需要处理的运行状态"><div><h2>{attention.label}需要处理</h2><p>{attention.detail}</p></div>{primaryAction ? <button type="button" className="v3-primary" onClick={primaryAction}>{primaryActionLabel}</button> : <span className="v3-runtime-action-blocked" aria-label={`${attention.label} 操作当前不可用`}>操作当前不可用</span>}</section>}{runtime && <ul className="v3-runtime-components" aria-label="运行组件">{runtime.checks.map((check) => <li key={check.id}><strong>{check.label}</strong><div><b data-state={check.state}>{check.state === "READY" ? "正在运行" : check.state === "WARNING" ? "需要处理" : check.state === "UNAVAILABLE" ? "当前不可用" : "正在检查"}</b><span>{check.detail}</span></div>{check.action ? <button type="button" className="v3-runtime-refresh" aria-label={`${check.actionLabel ?? "检查状态"} ${check.label}`} onClick={check.action}>{check.actionLabel ?? "检查状态"}</button> : <span className="v3-runtime-action-blocked" aria-label={`${check.label} 操作当前不可用`}>操作当前不可用</span>}</li>)}</ul>}<footer className="v3-runtime-footer"><details className="v3-lifecycle-disclosure" open={purgeConfirmation ? true : undefined}><summary>关闭全部前的影响确认</summary><div><h2>本地生命周期</h2><p>归档、回收和永久清除是不同操作；外部对话和项目目录不会由这里删除。</p>{lifecycle?.destructiveReason && <p className="v3-warning">{lifecycle.destructiveReason}</p>}<div className="v3-control-row">{onCreateVerifiedBackup && <button type="button" onClick={onCreateVerifiedBackup}>创建一致性备份</button>}{selectedWorkstreamId && lifecycle?.canArchive && <button type="button" onClick={() => onLifecycleChange?.(selectedWorkstreamId, "ARCHIVED")}>归档当前工作区</button>}{selectedWorkstreamId && lifecycle?.canTrash && <button type="button" onClick={() => onLifecycleChange?.(selectedWorkstreamId, "TRASHED")}>移至回收站</button>}{selectedWorkstreamId && lifecycle?.canPurge && <button ref={purgeTrigger} type="button" className="v3-danger" onClick={() => setPurgeConfirmation(" ")}>永久清除本地工作区</button>}</div>{lifecycle?.backupNotice && <p className="v3-detail">{lifecycle.backupNotice}</p>}{purgeConfirmation && <div className="v3-confirmation" role="alertdialog" aria-modal="true" aria-label="永久清除本地工作区确认" onKeyDown={(event) => { if (event.key === "Escape") { event.preventDefault(); setPurgeConfirmation(""); } else keepFocusWithinDialog(event); }}><p><strong>永久清除只作用于本地 Router 记录。</strong>外部对话、项目目录和文件不会被删除；若有运行、未确认交付或未读回复，Core 会拒绝。</p><label>输入 <code>PURGE_LOCAL_WORKSTREAM</code><input value={purgeConfirmation} onChange={(event) => setPurgeConfirmation(event.target.value)} /></label><button ref={purgeCancel} type="button" onClick={() => setPurgeConfirmation("")}>取消</button><button type="button" className="v3-danger" disabled={!purgeReady} onClick={() => { if (selectedWorkstreamId) onPurgeTrashedWorkstream?.(selectedWorkstreamId); setPurgeConfirmation(""); }}>永久清除本地记录</button></div>}</div></details><button type="button" className="v3-runtime-installer-blocked" onClick={onOpenInstall}>查看安装与更新</button></footer></section>;
}

/** D10 only projects runtime evidence. Data-lifecycle actions remain under D12/D20. */
function RuntimeStatus({ runtime, onOpenInstall, onOpenExitImpact, onOpenChatGptDiagnostics }: Pick<UnifiedWorkbenchProps, "runtime"> & { onOpenInstall: () => void; onOpenExitImpact: () => void; onOpenChatGptDiagnostics: () => void }) {
  const attention = runtime?.checks.find((check) => check.state === "WARNING" || check.state === "UNAVAILABLE");
  const primaryAction = attention?.action;
  const primaryActionLabel = attention?.actionLabel ?? "检查状态";
  return <section className="v3-runtime-stack" aria-label="工作区状态与运行环境">
    {attention && <section className="v3-runtime-alert" aria-label="当前需要处理的运行状态"><div><h2>{attention.label}需要处理</h2><p>{attention.detail}</p></div>{primaryAction ? <button type="button" className="v3-primary" onClick={primaryAction}>{primaryActionLabel}</button> : <span className="v3-runtime-action-blocked" aria-label={`${attention.label} 操作当前不可用`}>操作当前不可用</span>}</section>}
    {runtime?.notice ? <p className="v3-detail" role="status">{runtime.notice}</p> : null}
    {runtime && <ul className="v3-runtime-components" aria-label="运行组件">{runtime.checks.map((check) => <li key={check.id}><strong>{check.label}</strong><div><b data-state={check.state}>{check.state === "READY" ? "正在运行" : check.state === "WARNING" ? "需要处理" : check.state === "UNAVAILABLE" ? "当前不可用" : "正在检查"}</b><span>{check.detail}</span></div>{check.secondaryAction ? <button type="button" className="v3-runtime-refresh" aria-label={`${check.secondaryActionLabel ?? "在默认浏览器人工查看"} ${check.label}`} onClick={check.secondaryAction}>{check.secondaryActionLabel ?? "在默认浏览器人工查看"}</button> : null}{check.action && check.id !== attention?.id ? <button type="button" className="v3-runtime-refresh" aria-label={`${check.actionLabel ?? "检查状态"} ${check.label}`} onClick={check.action}>{check.actionLabel ?? "检查状态"}</button> : check.action ? <span className="v3-runtime-primary-note" aria-label={`${check.label} 的当前操作位于页面顶部`}>请使用页面顶部按钮</span> : check.id === "chatgpt" ? <button type="button" className="v3-runtime-refresh" aria-label={`查看详情 ${check.label}`} onClick={onOpenChatGptDiagnostics}>查看详情</button> : <span className="v3-runtime-action-blocked" aria-label={`${check.label} 操作当前不可用`}>操作当前不可用</span>}</li>)}</ul>}
    <footer className="v3-runtime-footer"><button type="button" className="v3-runtime-exit-impact-link" onClick={onOpenExitImpact}>关闭全部前的影响确认</button><button type="button" className="v3-runtime-installer-blocked" onClick={onOpenInstall}>查看安装与更新</button></footer>
  </section>;
}

/** START 127:2 is the first-run entry, not a second dashboard. Every card
 * navigates into an existing Router surface and never manufactures a selected
 * workstream, result, project, or provider state. */
function StartFocus({ onSurfaceChange }: { onSurfaceChange: (surface: WorkbenchSurface) => void }) {
  const entries: Array<{ number: string; title: string; detail: string; surface: WorkbenchSurface }> = [
    { number: "01", title: "手机从通知读回复", detail: "直达这一条 → 回复意见 → 保留旧回复", surface: "INBOX" },
    { number: "02", title: "电脑阅读与补充交接", detail: "选入代码块外文字 → 审阅 → 批准 → 发送", surface: "NEW_WORK" },
    { number: "03", title: "Codex 结果与一次性授权", detail: "完整结论、附件、打回修改；授权独立处理", surface: "NEW_WORK" },
    { number: "04", title: "外部项目与对话连接", detail: "保存项目链接，精确连接，允许稍后另一端", surface: "PROJECT_HOME" },
    { number: "05", title: "多工作区与回收站", detail: "搜索与归档、测试清理、恢复与永久清除", surface: "RECYCLE" },
    { number: "06", title: "安装与统一运行环境", detail: "一处管理组件；区分进程、登录、连接与可读", surface: "RUNTIME" },
  ];
  return <section className="v3-start-focus" aria-label="V3.1 工作台入口">
    <header><p>AI WORK ROUTER&nbsp; / &nbsp;V3.1</p><h1>少找入口，多把事情做完。</h1><span>电脑与手机，一套工作台。先读完整回复，再决定是否交接。</span></header>
    <p className="v3-start-note">V3.1 可点击修订原型 · 项目/对话显式选择 + Goal 操作 · 示例不写入真实系统</p>
    <div className="v3-start-grid">{entries.map((entry) => <button type="button" key={entry.number} className="v3-start-entry" onClick={() => onSurfaceChange(entry.surface)}><span>{entry.number}</span><strong>{entry.title}</strong><small>{entry.detail}</small><em>开始体验 →</em></button>)}</div>
    <button type="button" className="v3-start-coverage" onClick={() => onSurfaceChange("PROJECT_HOME")}>设计覆盖、状态边界与施工验收 →</button>
  </section>;
}

/** Shared responsive V3 presentation. All effects and persistence remain in its host. */
export function UnifiedWorkbench(props: UnifiedWorkbenchProps) { return <NativeWorkbench {...props}/>; }

/** Historical composition retained for protocol fixtures; never the product entry. */
export function LegacyWorkbench(props: UnifiedWorkbenchProps) {
  // Hosts always pass the controlled value. The local fallback keeps isolated
  // component rendering usable without creating a second production authority.
  const [uncontrolledSurface, setUncontrolledSurface] = useState<WorkbenchSurface>("WORKSPACE");
  const [railWidth,setRailWidth]=useState(()=>{try{return Number(localStorage.getItem("aiwr.v5.rail-width"))||240;}catch{return 240;}});
  const [focused,setFocused]=useState(false);
  const setWidth=(width:number)=>{setRailWidth(width);try{localStorage.setItem("aiwr.v5.rail-width",String(width));}catch{}};
  useEffect(()=>{const viewport=window.visualViewport;const update=()=>{if((window.innerWidth<=800||window.matchMedia?.("(hover:none) and (pointer:coarse) and (max-width:1100px)")?.matches)&&(!viewport||Math.abs(viewport.scale-1)<0.05))document.documentElement.style.setProperty("--aiwr-shell-height",`${viewport?.height??window.innerHeight}px`);else document.documentElement.style.removeProperty("--aiwr-shell-height");};update();window.addEventListener("resize",update);viewport?.addEventListener("resize",update);return()=>{window.removeEventListener("resize",update);viewport?.removeEventListener("resize",update);document.documentElement.style.removeProperty("--aiwr-shell-height");};},[]);
  const [handoffSelection, setHandoffSelection] = useState("");
  const surface = props.surface ?? uncontrolledSurface;
  const onSurfaceChange = props.onSurfaceChange ?? setUncontrolledSurface;
  const surfaceScroll=useRef<Record<string,number>>({});
  const scrollKey=surface==="WORKSPACE"?`${surface}:${props.selectedWorkstreamId??""}`:surface;
  useLayoutEffect(()=>{const main=document.querySelector<HTMLElement>(".v3-workbench-main");if(!main)return;main.scrollTop=surfaceScroll.current[scrollKey]??0;const track=()=>{surfaceScroll.current[scrollKey]=main.scrollTop;};main.addEventListener("scroll",track,{passive:true});return()=>main.removeEventListener("scroll",track);},[scrollKey]);
  const selectedItem = props.items.find((item) => item.id === props.selectedWorkstreamId);
  const beginHandoffSelection = (selection: string) => { setHandoffSelection(selection); onSurfaceChange("HANDOFF_SELECTION"); };
  const focusPanel = surface === "NOTIFICATIONS" ? <></> : surface === "SETTINGS" ? <SettingsOverview navigate={onSurfaceChange} onDevices={props.onOpenWebAccess}/> : surface === "BRIDGES" ? <BridgeDirectory items={props.items} onSelect={props.onSelectWorkstream} onNew={()=>onSurfaceChange("NEW_BRIDGE")}/> : surface === "NEW_BRIDGE" && props.onCreateBridge ? <NewBridgeForm create={props.onCreateBridge} cancel={()=>onSurfaceChange("BRIDGES")}/> : surface === "PROJECT_HOME" ? props.projectHomePanel ?? null : surface === "PROJECT" ? props.projectPanel ?? null : surface === "NEW_WORK" ? props.newWorkPanel ?? null : surface === "DISCUSSION" ? <DiscussionFocus item={props.items.find((item) => item.id === props.selectedWorkstreamId)} draft={props.draft} onDraftChange={props.onDraftChange} onSendDiscussion={props.onSendDiscussion} onOpenManualDiscussionDestination={props.onOpenManualDiscussionDestination} manualDiscussionDestinationActionLabel={props.manualDiscussionDestinationActionLabel} onBack={() => onSurfaceChange("WORKSPACE")} onNewWork={() => onSurfaceChange("NEW_WORK")} /> : surface === "GOAL" ? <GoalFocus goal={props.goal} onBack={() => onSurfaceChange("WORKSPACE")}><GoalControls {...props} /></GoalFocus> : surface === "RUNTIME" ? <RuntimeFocus onNewWork={() => onSurfaceChange("NEW_WORK")}><RuntimeStatus runtime={props.runtime} onOpenInstall={() => onSurfaceChange("INSTALL")} onOpenExitImpact={() => onSurfaceChange("EXIT_IMPACT")} onOpenChatGptDiagnostics={() => onSurfaceChange("CHATGPT_DIAGNOSTICS")} />{props.runtimeAddon}</RuntimeFocus> : surface === "PROVIDER_RUN_STATUS" ? <ProviderRunStatusFocus onBack={() => onSurfaceChange("INBOX")}>{props.providerRunStatusPanel ?? <p role="alert">收件箱指定的精确执行状态当前不可读；Router 没有显示其他执行。</p>}</ProviderRunStatusFocus> : surface === "CHATGPT_DIAGNOSTICS" ? <ChatGptDiagnostics runtime={props.runtime} onBack={() => onSurfaceChange("WORKSPACE")} /> : surface === "HANDOFF_SELECTION" ? <HandoffSelectionFocus item={selectedItem} reply={props.reply} selection={handoffSelection} onAdd={(replyId, text) => Promise.resolve(props.onAddSelectionToHandoff?.(replyId, text)).then(() => onSurfaceChange("HANDOFF_REVIEW"))} onBack={() => onSurfaceChange("WORKSPACE")} /> : surface === "HANDOFF_REVIEW" ? <HandoffReviewFocus handoff={props.handoff} workstreamName={props.items.find((item) => item.id === props.selectedWorkstreamId)?.name} onBack={() => onSurfaceChange("WORKSPACE")} onOpenStatus={() => onSurfaceChange("HANDOFF_STATUS")}><HandoffReview {...props} workstreamName={props.items.find((item) => item.id === props.selectedWorkstreamId)?.name} /></HandoffReviewFocus> : surface === "HANDOFF_STATUS" ? <HandoffStatusFocus handoff={props.handoff} workstreamName={props.items.find((item) => item.id === props.selectedWorkstreamId)?.name} onBack={() => onSurfaceChange("WORKSPACE")} onOpenInbox={() => onSurfaceChange("INBOX")} onRefreshHandoffStatus={props.onRefreshHandoffStatus} onOpenHandoffDestination={props.onOpenHandoffDestination} /> : surface === "EXIT_IMPACT" ? <ExitImpactFocus onBack={() => onSurfaceChange("WORKSPACE")} onOpenGoal={() => onSurfaceChange("GOAL")} onHideToTray={props.onHideToTray} /> : surface === "INSTALL" ? <InstallFocus onNewWork={() => onSurfaceChange("NEW_WORK")} onBack={() => onSurfaceChange("RUNTIME")} /> : surface === "RECYCLE" ? <RecycleFocus {...props} onBack={() => onSurfaceChange("BRIDGES")} onNewWork={() => onSurfaceChange("WORKSPACE")} onOpenCleanup={() => onSurfaceChange("TEST_CLEANUP")} onOpenPurge={() => onSurfaceChange("PURGE_CONFIRM")} /> : surface === "PURGE_CONFIRM" ? <PurgeConfirmFocus item={props.items.find((item) => item.id === props.selectedWorkstreamId)} lifecycle={props.lifecycle} onPurgeTrashedWorkstream={props.onPurgeTrashedWorkstream} onBack={() => onSurfaceChange("RECYCLE")} /> : surface === "TEST_CLEANUP" ? <TestCleanupFocus onNewWork={() => onSurfaceChange("NEW_WORK")} onBack={() => onSurfaceChange("RECYCLE")} onCreateVerifiedBackup={props.onCreateVerifiedBackup} /> : surface === "CODEX_HISTORY" ? props.codexHistoryPanel ?? null : surface === "CHATGPT_RESULTS" ? props.chatgptResultsPanel ?? null : surface === "CODEX_RESULTS" ? props.codexResultsPanel ?? null : surface === "CODEX_ATTACHMENTS" ? props.codexAttachmentsPanel ?? null : surface === "CODEX_FEEDBACK" ? props.codexFeedbackPanel ?? null : surface === "CODEX_REQUEST" ? props.codexRequestPanel ?? null : null;
  const showStart = surface === "WORKSPACE" && props.items.length === 0 && !props.reply;
  // This action only opens the protected review step.  Approval and delivery
  // are intentionally later, separate owner actions; the entry label must not
  // imply either one already happened.
  const prepareHandoffLabel = props.reply?.provider === "CODEX" ? "审阅这条 Codex 回复" : "审阅这条 ChatGPT 回复";
  const handoffStatusActionLabel = props.handoff?.status === "SENT"
    ? "查看交付与执行"
    : props.handoff?.status === "SENDING"
      ? "查看送达状态"
      : props.handoff?.status === "FAILED"
        ? "查看未发送的交付记录"
        : props.handoff?.status === "APPROVED" && props.handoff.requiresManualDispatch
          ? "查看已批准内容并手动发送"
        : null;
  const hasRecoverableHandoffStatus = handoffStatusActionLabel !== null;
  const hasPendingReview = props.handoff?.status === "READY";
  const hasApprovedReviewWithoutStatus = props.handoff?.status === "APPROVED" && !hasRecoverableHandoffStatus;
  const workspace = <WorkspaceFocus item={selectedItem} reply={props.reply} handoff={props.handoff} onOpenConnection={() => onSurfaceChange("PROJECT")} onNewWork={() => onSurfaceChange("NEW_WORK")} hasCodexEndpoint={props.hasCodexEndpoint} codexResultCount={props.codexResultCount} codexRequestCount={props.codexRequestCount} onSurfaceChange={onSurfaceChange} mobileWorkspaceMenu={props.mobileWorkspaceMenu}>{props.workspaceNotice ? <p className="v3-workspace-check-notice" role="status">{props.workspaceNotice}</p> : null}<div className="v3-workspace-reading-grid"><ReplyReader {...props} onBeginHandoffSelection={beginHandoffSelection} onSurfaceChange={onSurfaceChange} /><ReplyActionContext reply={props.reply} handoff={props.handoff} /></div><footer className="v3-workspace-footer"><button type="button" className="v3-workspace-other-replies" onClick={() => onSurfaceChange("INBOX")}>查看其他回复</button><div>{hasRecoverableHandoffStatus ? <button type="button" className="v3-primary" onClick={() => onSurfaceChange("HANDOFF_STATUS")}>{handoffStatusActionLabel}</button> : props.reply && !props.reply.readOnly && props.onPrepareHandoff ? <><button type="button" onClick={() => onSurfaceChange("DISCUSSION")}>修改</button><button type="button" className="v3-primary" onClick={() => { void Promise.resolve(props.onPrepareHandoff?.(props.reply!.id)).then((target) => onSurfaceChange(target ?? "HANDOFF_REVIEW")).catch(() => undefined); }}>{prepareHandoffLabel}</button></> : props.reply?.provider === "CHATGPT" && !props.reply.readOnly && props.onAddSelectionToHandoff ? <><button type="button" className="v3-primary" onClick={() => onSurfaceChange("DISCUSSION")}>回复 ChatGPT</button><button type="button" onClick={() => beginHandoffSelection("")}>选择范围</button></> : props.reply ? <><button type="button" className="v3-primary" onClick={() => onSurfaceChange("DISCUSSION")}>回复 ChatGPT</button><button type="button" onClick={() => onSurfaceChange("INBOX")}>更多</button></> : <button type="button" onClick={() => onSurfaceChange("DISCUSSION")}>修改</button>}{hasPendingReview && <button type="button" onClick={() => onSurfaceChange("HANDOFF_REVIEW")}>继续审阅待批准内容</button>}{hasApprovedReviewWithoutStatus && <button type="button" className="v3-primary" onClick={() => onSurfaceChange("HANDOFF_REVIEW")}>查看已批准内容并单独发送</button>}</div></footer>{props.inlineComposer !== false && <DraftComposer {...props} />}</WorkspaceFocus>;
  const inbox = <InboxFocus items={props.items} selectedWorkstreamId={props.selectedWorkstreamId} onSelectWorkstream={props.onSelectWorkstream} onSelectAttention={props.onSelectAttention} onNewWork={() => onSurfaceChange("NEW_WORK")} onOpenWorkspace={() => onSurfaceChange("WORKSPACE")} onOpenDiscussion={() => onSurfaceChange("DISCUSSION")} onOpenRuntime={() => onSurfaceChange("RUNTIME")} reply={props.reply} onReplyRead={props.onReplyRead} onReplyHandled={props.onReplyHandled}><ReplyReader {...props} onBeginHandoffSelection={beginHandoffSelection} inboxPresentation onSurfaceChange={onSurfaceChange} /></InboxFocus>;
  const roleNavigation = props.roleCompatible && surface === "WORKSPACE" && <nav className="v5-context-menu" aria-label="工作区导航"><details onKeyDown={event => {if(event.key === "Escape"){event.currentTarget.open=false;(event.currentTarget.querySelector("summary") as HTMLElement)?.focus();}}}><summary aria-label="更多操作" title="更多操作"><MoreHorizontal size={18}/></summary><div onClick={event => {if((event.target as HTMLElement).closest("button"))(event.currentTarget.parentElement as HTMLDetailsElement).open=false;}}>{props.goal && <button type="button" onClick={() => onSurfaceChange("GOAL")}>当前目标与执行</button>}{props.hasCodexEndpoint && <button type="button" onClick={() => onSurfaceChange("CODEX_HISTORY")}>Codex 对话</button>}{Boolean(props.codexResultCount) && <button type="button" onClick={() => onSurfaceChange("CODEX_RESULTS")}>Codex 结果</button>}{Boolean(props.codexRequestCount) && <button type="button" onClick={() => onSurfaceChange("CODEX_REQUEST")}>Codex 授权请求</button>}{props.mobileWorkspaceMenu}<button type="button" onClick={() => onSurfaceChange("PROJECT_HOME")}>项目与对话</button><button type="button" onClick={() => onSurfaceChange("RUNTIME")}>运行环境</button><button type="button" onClick={() => onSurfaceChange("NEW_WORK")}>新建工作</button></div></details></nav>;
  if (showStart) return <main className="unified-workbench v3-surface-start" aria-label="AI Work Router 统一工作台">{props.globalActions && <div key="global-actions" className="v4-global-shortcuts">{props.globalActions}</div>}{props.criticalNotice}<StartFocus onSurfaceChange={onSurfaceChange} /></main>;
  return <main className={`unified-workbench v5-shell ${focused?"v5-focused":""} v3-surface-${surface.toLowerCase()}`} style={{"--v5-rail-width":`${railWidth}px`} as React.CSSProperties} aria-label="AI Work Router 统一工作台">
    <header className="v5-global-navigation"><button type="button" onClick={()=>onSurfaceChange("BRIDGES")}><ArrowLeft size={18}/>Bridge</button><div><button type="button" aria-label={focused?"显示列表":"专注阅读"} onClick={()=>setFocused(!focused)}>{focused?<PanelLeftOpen size={18}/>:<PanelLeftClose size={18}/>}</button><button type="button" onClick={()=>onSurfaceChange("INBOX")}>待办</button>{props.globalActions}<button type="button" aria-label="设置" title="设置" onClick={()=>onSurfaceChange("SETTINGS")}><Settings size={18}/></button>{roleNavigation}</div></header>
    {!focused&&<PanelDivider value={railWidth} onChange={setWidth}/>}
    <WorkbenchRail {...props} surface={surface} onSurfaceChange={onSurfaceChange} />
    <section className="v3-workbench-main">{props.criticalNotice}<div hidden={surface !== "WORKSPACE"}>{props.bridgePanel}</div>{props.roleCompatible && surface === "WORKSPACE" ? null : focusPanel ?? (surface === "INBOX" ? inbox : workspace)}</section>
  </main>;
}
