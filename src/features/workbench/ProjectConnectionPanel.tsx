import { CodexThreadPicker } from "../codex/CodexThreadPicker";
import { useEffect, useState, type ReactNode } from "react";
import type {
  ExistingCodexThreadCandidate,
  ExternalProjectLink,
} from "../codex/types";
import type { ExplicitChatGptBindingCandidate } from "../codex/types";

export type ConnectionChoice = {
  /** Canonical provider identity. Display titles are intentionally non-unique. */
  id: string;
  provider: "CHATGPT" | "CODEX";
  title: string;
  detail: string;
  selected?: boolean;
  blocked?: boolean;
  /** Present only for a candidate returned by a verified native source. */
  onSelect?: () => void;
};

export type ConnectionScreen =
  | "CONVERSATIONS"
  | "REPLACE_CHATGPT"
  | "CODEX_PROJECT"
  | "CODEX_THREAD"
  | "UNPROJECTED"
  | "PAIR_REVIEW";
export type ExplicitChatGptConfirmationState =
  "IDLE" | "CONFIRMING" | "FAILED" | "SUCCEEDED";

export interface ProjectConnectionPanelProps {
  guidedBindingEntry?: boolean;
  onManageBindings?: () => void;
  projectName?: string | null;
  workstreamName?: string | null;
  links?: ExternalProjectLink[];
  choices: ConnectionChoice[];
  codexProjectId?: string;
  explicitChatGptUrl?: string;
  explicitChatGptCandidate?: ExplicitChatGptBindingCandidate | null;
  explicitChatGptConfirmationState?: ExplicitChatGptConfirmationState;
  explicitChatGptConfirmationMessage?: string | null;
  activeChatGptLabel?: string | null;
  activeChatGptId?: string | null;
  activeCodexLabel?: string | null;
  activeCodexThreadId?: string | null;
  codexThreadId: string;
  codexThreadLabel: string;
  error?: string | null;
  unprojectedDetail?: string | null;
  unprojectedThreadId?: string | null;
  unprojectedDirectory?: string;
  unprojectedCreating?: boolean;
  listenerStatus?: { enabled: boolean; unattendedMode: boolean } | null;
  listenerBusy?: boolean;
  listenerCanEnable?: boolean;
  onListenerChange?: (enabled: boolean) => void;
  onUnattendedChange?: (enabled: boolean) => void;
  onCodexProjectIdChange?: (value: string) => void;
  onSaveCodexProject?: () => void;
  onExplicitChatGptUrlChange?: (value: string) => void;
  onPrepareExplicitChatGptBinding?: () => void;
  onPrepareOwnerConfirmedChatGptBinding?: () => void;
  onConfirmExplicitChatGptBinding?: () => void;
  /** Opens the existing Host-owned browser only for a user-managed login/setup. */
  onOpenHostChatGptSetup?: () => void;
  onUseCurrentChatGptConversation?: () => void;
  existingCodexThreads?: ExistingCodexThreadCandidate[];
  existingCodexThreadsLoading?: boolean;
  selectedExistingCodexThread?: ExistingCodexThreadCandidate | null;
  onLoadExistingCodexThreads?: () => Promise<void>;
  onSelectExistingCodexThread?: (threadId: string) => Promise<void>;
  onCodexThreadIdChange: (value: string) => void;
  onCodexThreadLabelChange: (value: string) => void;
  onPair: () => void;
  onCreateUnprojected?: () => void;
  onCopyUnprojectedThreadId?: () => void;
  onUnprojectedDirectoryChange?: (value: string) => void;
  /** A host-selected Figma entry route; provider data never chooses it. */
  initialScreen?: ConnectionScreen;
  onBack?: () => void;
}

function ProviderColumn({
  provider,
  projectName,
  choices,
  children,
  onChangeProject,
  changeProjectLabel = "更换现有项目",
  mobile,
}: {
  provider: "CHATGPT" | "CODEX";
  projectName?: string | null;
  choices: ConnectionChoice[];
  children?: ReactNode;
  onChangeProject?: () => void;
  changeProjectLabel?: string;
  mobile?: boolean;
}) {
  const [query, setQuery] = useState("");
  const [mobileSearchOpen, setMobileSearchOpen] = useState(false);
  const relevant = choices.filter((choice) => choice.provider === provider);
  const normalizedQuery = query.trim().toLocaleLowerCase();
  const visible = normalizedQuery
    ? relevant.filter((choice) =>
        `${choice.title} ${choice.detail}`
          .toLocaleLowerCase()
          .includes(normalizedQuery),
      )
    : relevant;
  const heading = provider === "CHATGPT" ? "ChatGPT" : "Codex";
  return (
    <section
      className="v3-connection-column"
      data-provider={provider}
      data-mobile-list={mobile ? "true" : undefined}
      aria-label={`${heading} 对话选择`}
    >
      <header>
        <div>
          <h2>
            {mobile && provider === "CHATGPT" ? (
              (projectName ?? "尚未选择项目")
            ) : (
              <>
                {heading} · {projectName ?? "尚未选择项目"}
              </>
            )}
          </h2>
          <p>
            {provider === "CHATGPT"
              ? "仅接受上方已验证的精确对话链接。"
              : "项目已显式选择，不会按 ChatGPT 名称猜测。"}
          </p>
        </div>
        {onChangeProject && (
          <button
            type="button"
            className="v3-change-project"
            onClick={onChangeProject}
          >
            {changeProjectLabel}
          </button>
        )}
      </header>
      {provider === "CHATGPT" && (!mobile || mobileSearchOpen) && (
        <label className="v3-connection-search">
          <span>搜索对话</span>
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            aria-label={`搜索 ${heading} 对话`}
            placeholder="输入标题关键词…"
          />
        </label>
      )}
      <div className="v3-connection-list">
        {visible.map((choice) =>
          choice.onSelect && !choice.blocked && !choice.selected ? (
            <button
              key={`${choice.provider}-${choice.id}`}
              type="button"
              className="candidate"
              onClick={choice.onSelect}
            >
              <span aria-hidden="true">○</span>
              <div>
                <strong>{choice.title}</strong>
                <small>{choice.detail}</small>
              </div>
            </button>
          ) : (
            <article
              key={`${choice.provider}-${choice.id}`}
              aria-current={choice.selected ? "true" : undefined}
              className={
                choice.selected ? "selected" : choice.blocked ? "blocked" : ""
              }
            >
              <span aria-hidden="true">{choice.selected ? "●" : "!"}</span>
              <div>
                <strong>{choice.title}</strong>
                <small>{choice.detail}</small>
              </div>
            </article>
          ),
        )}
        {!relevant.length && (
          <article className="blocked">
            <span aria-hidden="true">!</span>
            <div>
              <strong>
                {provider === "CHATGPT"
                  ? "请粘贴精确对话链接"
                  : "目录当前不可用"}
              </strong>
              <small>
                {provider === "CHATGPT"
                  ? "不再读取或控制 ChatGPT 项目目录、侧栏或最近列表。"
                  : "当前 Codex 版本尚无可核验的原生项目成员来源；没有按 cwd、同名或最近列表推断。"}
              </small>
            </div>
          </article>
        )}
        {relevant.length > 0 && !visible.length && (
          <article className="blocked">
            <span aria-hidden="true">!</span>
            <div>
              <strong>没有匹配的可验证对话</strong>
              <small>
                筛选仅在当前已读取的精确目录结果中进行；不会改用全局最近列表。
              </small>
            </div>
          </article>
        )}
      </div>
      {provider === "CHATGPT" && mobile && !mobileSearchOpen && (
        <button
          type="button"
          className="v3-mobile-directory-search"
          onClick={() => setMobileSearchOpen(true)}
        >
          搜索
        </button>
      )}
      {children}
    </section>
  );
}

/** Presentation-only flow for D44/D46/M51: titles and dates lead; exact IDs
 * remain in the Core and are only exposed in the explicitly opened detail. */
export function ProjectConnectionPanel(props: ProjectConnectionPanelProps) {
  const [advancedLegacy, setAdvancedLegacy] = useState(false);
  const [mobileProvider, setMobileProvider] = useState<"CHATGPT" | "CODEX">(
    "CHATGPT",
  );
  // This is a Host-selected entry only.  It deliberately skips the generic
  // "current pair" overview, but it does not mutate either Endpoint.
  const startsReplacingChatGpt = props.initialScreen === "REPLACE_CHATGPT";
  const [screen, setScreen] = useState<ConnectionScreen>(() =>
    startsReplacingChatGpt
      ? "CONVERSATIONS"
      : (props.initialScreen ?? "CONVERSATIONS"),
  );
  const [replaceBinding, setReplaceBinding] = useState(startsReplacingChatGpt);
  // A current binding can be missing only its canonical URL metadata.  That
  // repair must not be presented as a replacement of the conversation itself.
  const [chatGptBindingAction, setChatGptBindingAction] = useState<
    "REPLACE" | "CONFIRM_CURRENT_URL"
  >(startsReplacingChatGpt ? "REPLACE" : "CONFIRM_CURRENT_URL");
  const [currentUrlError, setCurrentUrlError] = useState<string | null>(null);
  const [technicalThreadId, setTechnicalThreadId] = useState("");
  const [isMobileViewport, setIsMobileViewport] = useState(
    () =>
      typeof window !== "undefined" &&
      window.matchMedia?.("(max-width: 680px)").matches === true,
  );
  useEffect(() => {
    const media = window.matchMedia?.("(max-width: 680px)");
    if (!media) return;
    const update = () => setIsMobileViewport(media.matches);
    update();
    media.addEventListener?.("change", update);
    return () => media.removeEventListener?.("change", update);
  }, []);
  useEffect(() => {
    if (!window.navigator.userAgent.toLocaleLowerCase().includes("jsdom"))
      window.scrollTo(0, 0);
    const scroll = document.querySelector<HTMLElement>(".v3-connection-scroll");
    if (scroll) {
      scroll.scrollTop = 0;
      scroll.scrollLeft = 0;
    }
  }, [screen]);
  const hasChoice = Boolean(props.codexThreadId.trim());
  const explicitConfirmationState =
    props.explicitChatGptConfirmationState ?? "IDLE";
  const explicitCandidatePending = Boolean(props.explicitChatGptCandidate);
  const explicitCandidateIsOwnerConfirmed =
    props.explicitChatGptCandidate?.verification ===
    "OWNER_CONFIRMED_EXACT_URL";
  const currentPairBound = Boolean(
    props.activeChatGptLabel &&
    props.activeCodexLabel &&
    !explicitCandidatePending,
  );
  const confirmingCurrentChatGptUrl =
    replaceBinding && chatGptBindingAction === "CONFIRM_CURRENT_URL";
  const missingMetadata = Boolean(
    props.codexThreadId.trim() && !props.codexThreadLabel.trim(),
  );
  const canPair = hasChoice && !missingMetadata;
  const pairingReason = !hasChoice
    ? "先选择一条已核验的 Codex 精确对话；ChatGPT 绑定必须通过上方精确链接单独确认。"
    : missingMetadata
      ? "每个已选对话都需要标题、日期和来源信息。"
      : "配对会保存 Codex 精确 ID，并保留显示用标题、日期和来源。";
  const mobileHeading =
    mobileProvider === "CHATGPT" ? "选择 ChatGPT 对话" : "选择 Codex 主对话";
  const codexProject =
    props.links?.find((link) => link.provider === "CODEX") ?? null;
  const pendingLegacyChoice = explicitCandidatePending || Boolean(props.selectedExistingCodexThread && props.selectedExistingCodexThread.id !== props.activeCodexThreadId) || Boolean(props.unprojectedThreadId && props.unprojectedThreadId !== props.activeCodexThreadId && props.codexThreadId === props.unprojectedThreadId);
  if (props.guidedBindingEntry && !advancedLegacy && !startsReplacingChatGpt && screen === "CONVERSATIONS") return <section className="v3-connection-surface v4-project-binding-entry" aria-label="项目与对话连接">
    <header className="v3-surface-heading"><h1>项目与对话</h1><p className="v4-meta">当前工作 · {props.workstreamName || "尚未选择工作区"}</p></header>
    <article className="v4-project-binding-card">
      <div><h2>控制端</h2><p>决定下一步、下达指令</p></div>
      <p className="v4-binding-flow" aria-label="控制端下达指令，执行端返回结果">↓ 指令　↑ 结果</p>
      <div><h2>执行端</h2><p>完成任务、返回结果</p></div>
      <p className="v4-meta">两端都可使用 Codex 或 ChatGPT。</p>
      <button type="button" className="v3-primary" disabled={!props.onManageBindings || pendingLegacyChoice} onClick={props.onManageBindings}>选择控制端与执行端</button>
      {!props.onManageBindings && <p role="status">先选择一个工作区，再为它绑定对话。</p>}
      {pendingLegacyChoice && <p role="status">有一份尚未保存的选择。<button type="button" onClick={() => {setAdvancedLegacy(true);setScreen(canPair ? "PAIR_REVIEW" : "CONVERSATIONS");}}>继续核对现有选择</button></p>}
    </article>
    <button type="button" className="v4-project-advanced-entry" onClick={() => {setAdvancedLegacy(true);setScreen("CONVERSATIONS");}}>项目链接与高级连接 ›</button>
    {props.onBack && <footer><button type="button" onClick={props.onBack}>返回工作区</button></footer>}
  </section>;
  // A bound Workstream is not an unfinished setup form.  Showing a blank URL
  // field here caused owners to re-verify or accidentally attempt to replace a
  // perfectly good pair merely to discover what was already selected.
  if (screen === "CONVERSATIONS" && currentPairBound && !replaceBinding)
    return (
      <section
        className="v3-connection-surface v3-current-binding-flow"
        aria-label="当前对话已绑定"
      >
        <header className="v3-surface-heading">
          {props.guidedBindingEntry && advancedLegacy && <button type="button" className="v4-advanced-back" onClick={() => {setAdvancedLegacy(false);setScreen("CONVERSATIONS");}}>返回简洁绑定</button>}
          <button
            type="button"
            className="v3-mobile-connection-back"
            onClick={props.onBack}
          >
            ‹ 返回
          </button>
          <div>
            <h1>当前对话已绑定</h1>
            <span>这个工作区已经连接完成；不需要粘贴链接或再次核对。</span>
          </div>
        </header>
        <div className="v3-connection-scroll">
          <article className="v3-pair-review-card">
            <p className="v3-pair-review-work-label">当前工作区</p>
            <h2>{props.workstreamName ?? props.projectName ?? "当前工作台"}</h2>
            <section className="v3-pair-review-provider">
              <p>ChatGPT · 当前已绑定</p>
              <h3>{props.activeChatGptLabel}</h3>
              <small>如需阅读或继续，请返回工作区；不会自动发送消息。</small>
            </section>
            <section className="v3-pair-review-provider">
              <p>Codex · 当前已绑定</p>
              <h3>{props.activeCodexLabel}</h3>
              <small>这是当前工作区的已保存 Codex 对话，不需要再次核对。</small>
            </section>
            <details>
              <summary>精确绑定标识</summary>
              <p>
                ChatGPT conversation_id：<code>{props.activeChatGptId}</code>
              </p>
              <p>
                Codex thread_id：<code>{props.activeCodexThreadId}</code>
              </p>
            </details>
          </article>
        </div>
        <footer>
          <button
            type="button"
            className="v3-secondary-action v3-desktop-connection-back"
            onClick={props.onBack}
          >
            返回工作区
          </button>
          <button
            type="button"
            onClick={() => {
              setChatGptBindingAction("CONFIRM_CURRENT_URL");
              setCurrentUrlError(null);
              setReplaceBinding(true);
            }}
          >
            补充已核对的 ChatGPT 链接
          </button>
          <button
            type="button"
            onClick={() => {
              setChatGptBindingAction("REPLACE");
              setCurrentUrlError(null);
              setReplaceBinding(true);
            }}
          >
            更换这组对话
          </button>
        </footer>
      </section>
    );
  if (screen === "CODEX_PROJECT")
    return (
      <section
        className="v3-connection-surface v3-codex-project-flow"
        aria-label="选择 Codex 项目"
      >
        <header className="v3-surface-heading">
          {props.guidedBindingEntry && advancedLegacy && <button type="button" className="v4-advanced-back" onClick={() => {setAdvancedLegacy(false);setScreen("CONVERSATIONS");}}>返回简洁绑定</button>}
          <button
            type="button"
            className="v3-mobile-connection-back"
            onClick={() => {
              setScreen("CONVERSATIONS");
              setMobileProvider("CHATGPT");
            }}
          >
            ‹ 返回
          </button>
          <div>
            <h1>
              {isMobileViewport ? "选择 Codex 项目" : "选择已有 Codex 项目"}
            </h1>
            <span>
              {explicitCandidatePending
                ? "这是可选的 Codex 项目选择；不会影响等待确认的 ChatGPT 绑定。"
                : isMobileViewport
                  ? "只显示已核验的原生项目来源；不会按工作目录或同名推断。"
                  : "选择你在 Codex 中实际使用的项目；这里不创建新项目。"}
            </span>
          </div>
        </header>
        <div className="v3-connection-scroll">
          <div className="v3-codex-project-list">
            {codexProject && (
              <label className="v3-codex-project-search">
                搜索项目
                <input
                  aria-describedby="v3-codex-project-source-status"
                  placeholder="按名称或已登记目录查找…"
                />
              </label>
            )}
            {codexProject ? (
              <article className="selected">
                <span aria-hidden="true">●</span>
                <div>
                  <strong>{codexProject.label}</strong>
                  <small>
                    已保存的既有 Codex 项目 · {codexProject.sourceKind}
                  </small>
                </div>
              </article>
            ) : (
              <article className="blocked">
                <span aria-hidden="true">!</span>
                <div>
                  <strong id="v3-codex-project-source-status">
                    项目目录当前不可用
                  </strong>
                  <small>
                    当前 Codex 版本没有可核验的原生项目成员来源；不会把
                    cwd、同名目录或最近记录当作项目。
                  </small>
                </div>
              </article>
            )}
          </div>
          {!codexProject && (
            <details
              key="codex-project-technical-detail"
              className="v3-technical-connection"
            >
              <summary>技术详情：保存已核验 Codex 项目</summary>
              <p>仅当你已独立核验既有原生项目身份时使用；不会创建新项目。</p>
              <label>
                Codex 项目 ID
                <input
                  value={props.codexProjectId ?? ""}
                  onChange={(event) =>
                    props.onCodexProjectIdChange?.(event.target.value)
                  }
                />
              </label>
              <button
                type="button"
                className="v3-secondary-action"
                disabled={
                  !props.codexProjectId?.trim() || !props.onSaveCodexProject
                }
                onClick={props.onSaveCodexProject}
              >
                保存已核验项目
              </button>
            </details>
          )}
          {isMobileViewport && !codexProject && (
            <div className="v3-codex-project-secondary">
              <p>或者明确创建一条不属于任何 Codex 项目的新对话。</p>
              <button
                type="button"
                className="v3-secondary-action"
                onClick={() => setScreen("UNPROJECTED")}
              >
                新建无项目 Codex 对话
              </button>
            </div>
          )}
          {props.error && (
            <p className="v3-connection-error" role="alert">
              {props.error}
            </p>
          )}
        </div>
        <footer>
          <button
            type="button"
            className="v3-secondary-action v3-codex-later-footer"
            onClick={() => {
              setScreen("CONVERSATIONS");
              setMobileProvider("CHATGPT");
            }}
          >
            返回 ChatGPT 对话
          </button>
          {codexProject ? (
            <button
              type="button"
              className="v3-primary"
              onClick={() => {
                setScreen("CONVERSATIONS");
                setMobileProvider("CODEX");
              }}
            >
              使用所选项目
            </button>
          ) : (
            <button
              type="button"
              className="v3-primary"
              onClick={() => {
                setScreen("CONVERSATIONS");
                setMobileProvider(isMobileViewport ? "CODEX" : "CHATGPT");
              }}
            >
              稍后关联并继续
            </button>
          )}
        </footer>
      </section>
    );
  if (screen === "CODEX_THREAD")
    return (
      <section
        className="v3-connection-surface v3-codex-thread-flow"
        aria-label="绑定已有 Codex 对话"
      >
        <header className="v3-surface-heading">
          {props.guidedBindingEntry && advancedLegacy && <button type="button" className="v4-advanced-back" onClick={() => {setAdvancedLegacy(false);setScreen("CONVERSATIONS");}}>返回简洁绑定</button>}
          <button
            type="button"
            className="v3-mobile-connection-back"
            onClick={() => setScreen("CONVERSATIONS")}
          >
            ‹ 返回
          </button>
          <div>
            <h1>绑定已有 Codex 对话</h1>
            <span>
              从官方 Codex 对话目录读取；项目归属只作显示，不是绑定前提。
            </span>
          </div>
        </header>
        <div className="v3-connection-scroll">
          <p className="v3-connection-note">
            选择后会重新用精确 `thread/read`
            验证身份。不会恢复对话、发送消息、读取历史或取得写入控制。
          </p>
          {!props.existingCodexThreadsLoading &&
            !props.existingCodexThreads?.length && (
              <button
                type="button"
                className="v3-primary"
                disabled={!props.onLoadExistingCodexThreads}
                onClick={() => {
                  if (props.onLoadExistingCodexThreads)
                    void props.onLoadExistingCodexThreads().catch(() => {});
                }}
              >
                读取已有 Codex 对话
              </button>
            )}
          {props.existingCodexThreadsLoading && (
            <p role="status">正在读取已有 Codex 对话…</p>
          )}
          <div className="v3-connection-list">
            {!!props.existingCodexThreads?.length && <CodexThreadPicker threads={props.existingCodexThreads} value="" ariaLabel="绑定已有 Codex 对话" mode="list" disabled={!props.onSelectExistingCodexThread||props.existingCodexThreadsLoading} onChange={id=>{if(props.onSelectExistingCodexThread)void props.onSelectExistingCodexThread(id).then(()=>setScreen("PAIR_REVIEW")).catch(()=>{});}}/>}
          </div>
          <details className="v3-technical-connection">
            <summary>使用精确 Codex thread ID</summary>
            <p>仅供技术排障；粘贴后仍必须通过官方精确读取验证。</p>
            <label>
              Codex thread ID
              <input
                aria-label="Codex thread ID"
                value={technicalThreadId}
                onChange={(event) => setTechnicalThreadId(event.target.value)}
              />
            </label>
            <button
              type="button"
              className="v3-secondary-action"
              disabled={
                !technicalThreadId.trim() || !props.onSelectExistingCodexThread
              }
              onClick={() => {
                if (props.onSelectExistingCodexThread)
                  void props
                    .onSelectExistingCodexThread(technicalThreadId)
                    .then(() => setScreen("PAIR_REVIEW"))
                    .catch(() => {});
              }}
            >
              验证并核对
            </button>
          </details>
          {props.error && (
            <p className="v3-connection-error" role="alert">
              {props.error}
            </p>
          )}
        </div>
        <footer>
          <button
            type="button"
            className="v3-secondary-action"
            onClick={() => setScreen("CONVERSATIONS")}
          >
            返回选择
          </button>
        </footer>
      </section>
    );
  if (screen === "UNPROJECTED")
    return (
      <section
        className="v3-connection-surface v3-unprojected-flow"
        aria-label="无项目 Codex 对话"
      >
        <header className="v3-surface-heading">
          {props.guidedBindingEntry && advancedLegacy && <button type="button" className="v4-advanced-back" onClick={() => {setAdvancedLegacy(false);setScreen("CONVERSATIONS");}}>返回简洁绑定</button>}
          <button
            type="button"
            className="v3-mobile-connection-back"
            onClick={() => setScreen("CODEX_PROJECT")}
          >
            ‹ 返回
          </button>
          <div>
            <h1 aria-label="无项目 Codex 对话">
              {isMobileViewport ? "无项目 Codex 对话" : "新建无项目 Codex 对话"}
            </h1>
            <span>
              {isMobileViewport
                ? "创建前先确认独立目录与权限边界。"
                : "明确不属于 Codex 项目；不会静默放到 AI Work Router 仓库。"}
            </span>
          </div>
        </header>
        <div className="v3-connection-scroll">
          <article className="v3-unprojected-card">
            <p className="v3-unprojected-name-label">对话名称</p>
            <p className="v3-unprojected-name-value">临时讨论</p>
            <p className="v3-unprojected-project">Codex 项目：无项目</p>
            <section>
              {props.onUnprojectedDirectoryChange ? (
                <label>
                  工作目录 · 创建前可更改
                  <input
                    aria-label="工作目录"
                    disabled={
                      props.unprojectedCreating ||
                      Boolean(props.unprojectedThreadId)
                    }
                    value={props.unprojectedDirectory ?? ""}
                    onChange={(event) =>
                      props.onUnprojectedDirectoryChange?.(event.target.value)
                    }
                    placeholder="Router 专用独立工作目录（非业务仓库）"
                  />
                  <small className="v3-unprojected-mobile-directory-note">
                    创建前可修改；不使用当前业务仓库作为隐式默认目录。
                  </small>
                </label>
              ) : (
                <>
                  <p>工作目录 · 创建前可更改</p>
                  <strong>
                    {props.unprojectedDirectory?.trim() ||
                      "Router 专用独立工作目录（非业务仓库）"}
                  </strong>
                </>
              )}
            </section>
            <p className="v3-unprojected-boundary">
              工作目录是执行上下文，不是对话身份或原生项目归属。
            </p>
            <p className="v3-unprojected-contract">
              创建不发送首条消息。取得真实 thread ID 后，才登记为可连接的对话。
            </p>
            <p className="v3-unprojected-persistence">
              若 provider
              需要首轮才能持久化，将明确提示；不能制造一个假的成功状态。
            </p>
            {props.unprojectedDetail && (
              <p className="v3-connection-note">
                {props.unprojectedDetail}
                {props.unprojectedThreadId &&
                props.onCopyUnprojectedThreadId ? (
                  <button
                    type="button"
                    onClick={props.onCopyUnprojectedThreadId}
                  >
                    复制真实 ID
                  </button>
                ) : null}
              </p>
            )}
            {props.unprojectedThreadId && (
              <p className="v3-connection-note" role="status">
                已通过 Codex `thread/read`
                验证，并已带入本次配对候选；返回后仍需核对并确认，当前 Endpoint
                不会自动改变。
              </p>
            )}
          </article>
          {props.error && (
            <p className="v3-connection-error" role="alert">
              {props.error}
            </p>
          )}
        </div>
        <footer>
          <button
            type="button"
            className="v3-secondary-action v3-desktop-connection-back"
            onClick={() => setScreen("CODEX_PROJECT")}
          >
            {isMobileViewport ? "返回项目" : "返回选择"}
          </button>
          <button
            type="button"
            className="v3-primary"
            aria-label="创建无项目对话"
            disabled={
              !props.onCreateUnprojected ||
              props.unprojectedCreating ||
              Boolean(props.unprojectedThreadId)
            }
            onClick={props.onCreateUnprojected}
          >
            {props.unprojectedCreating
              ? "正在创建…"
              : props.unprojectedThreadId
                ? "已创建候选"
                : isMobileViewport
                  ? "创建无项目对话"
                  : "创建对话"}
          </button>
          {props.unprojectedThreadId && (
            <button
              type="button"
              className="v3-primary"
              disabled={!canPair || explicitCandidatePending}
              onClick={() => setScreen("PAIR_REVIEW")}
            >
              核对并绑定此 Codex 对话
            </button>
          )}
        </footer>
      </section>
    );
  if (screen === "PAIR_REVIEW")
    return (
      <section
        className="v3-connection-surface v3-pair-review-flow"
        aria-label="确认对话配对"
      >
        <header className="v3-surface-heading">
          {props.guidedBindingEntry && advancedLegacy && <button type="button" className="v4-advanced-back" onClick={() => {setAdvancedLegacy(false);setScreen("CONVERSATIONS");}}>返回简洁绑定</button>}
          <button
            type="button"
            className="v3-mobile-connection-back"
            onClick={() => {
              setScreen("CONVERSATIONS");
              setMobileProvider("CODEX");
            }}
          >
            ‹ 返回
          </button>
          <div>
            <h1>{isMobileViewport ? "确认对话配对" : "确认这一组对话"}</h1>
            <span>
              一次保存双向配对；这一步不会发送消息，也不会改动外部项目。
            </span>
          </div>
        </header>
        <div className="v3-connection-scroll">
          <article className="v3-pair-review-card">
            <p className="v3-pair-review-work-label">工作名称 · 当前工作区</p>
            <h2>{props.workstreamName ?? props.projectName ?? "当前工作台"}</h2>
            {(props.explicitChatGptCandidate || props.activeChatGptLabel) && (
              <section className="v3-pair-review-provider">
                <p>
                  {props.explicitChatGptCandidate
                    ? explicitCandidateIsOwnerConfirmed
                      ? "ChatGPT · 你已人工核对精确链接，等待同组保存"
                      : "ChatGPT · Router 专用载体已精确验证，等待同组保存"
                    : "ChatGPT · 当前已绑定"}
                </p>
                <h3>
                  {props.explicitChatGptCandidate?.label ??
                    props.activeChatGptLabel}
                </h3>
                {props.explicitChatGptCandidate && (
                  <small>
                    conversation_id：
                    <code>{props.explicitChatGptCandidate.externalId}</code>
                  </small>
                )}
                <small>不会重新读取项目目录或发送消息</small>
              </section>
            )}
            {props.codexThreadId.trim() && (
              <section className="v3-pair-review-provider">
                <p>
                  Codex ·{" "}
                  {props.unprojectedThreadId === props.codexThreadId
                    ? "无原生项目归属"
                    : (props.selectedExistingCodexThread?.projectProvenance ??
                      codexProject?.label ??
                      "Project 归属未确认")}
                </p>
                <h3>{props.codexThreadLabel}</h3>
                {props.selectedExistingCodexThread?.updatedAt && (
                  <small>
                    活动：{props.selectedExistingCodexThread.updatedAt}
                  </small>
                )}
                {props.unprojectedThreadId === props.codexThreadId && (
                  <small>工作目录：{props.unprojectedDirectory}</small>
                )}
                <small>标题、日期和来源已保留</small>
              </section>
            )}
            <span className="v3-pair-review-badge">
              {props.explicitChatGptCandidate
                ? "原子保存 ChatGPT 与 Codex 精确配对"
                : "保存 Codex 精确配对"}
            </span>
            <p className="v3-pair-review-contract">
              {props.explicitChatGptCandidate
                ? explicitCandidateIsOwnerConfirmed
                  ? "ChatGPT 链接由你在默认浏览器人工核对；Router 没有读取或控制该页面。保存会在同一事务中写入两个 ACTIVE Endpoint。不会发送消息，也不会修改外部项目。"
                  : "两端都已精确验证；保存会在同一事务中写入两个 ACTIVE Endpoint。不会发送消息，也不会修改外部项目。"
                : "ChatGPT 绑定须先通过上方精确链接单独确认；此处只保存 Codex 精确配对。不会发送消息，也不会修改外部项目。"}
            </p>
          </article>
          {props.error && (
            <p className="v3-connection-error" role="alert">
              {props.error}
            </p>
          )}
        </div>
        <footer>
          <button
            type="button"
            className="v3-secondary-action v3-desktop-connection-back"
            onClick={() => {
              setScreen("CONVERSATIONS");
              setMobileProvider("CODEX");
            }}
          >
            返回选择
          </button>
          <button type="button" className="v3-primary" disabled={!canPair} onClick={props.onPair}>
            保存并进入工作
          </button>
        </footer>
      </section>
    );
  return (
    <section
      className="v3-connection-surface v3-conversation-flow"
      data-mobile-provider={mobileProvider}
      data-replace-chatgpt={replaceBinding ? "true" : undefined}
      aria-label="项目与对话连接"
    >
      <header className="v3-surface-heading">
          {props.guidedBindingEntry && advancedLegacy && <button type="button" className="v4-advanced-back" onClick={() => {setAdvancedLegacy(false);setScreen("CONVERSATIONS");}}>返回简洁绑定</button>}
        <button
          type="button"
          className="v3-mobile-connection-back"
          onClick={() =>
            mobileProvider === "CODEX"
              ? setScreen("CODEX_PROJECT")
              : props.onBack?.()
          }
        >
          ‹ 返回
        </button>
        <div>
          <h1>
            {replaceBinding
              ? confirmingCurrentChatGptUrl
                ? "补充当前 ChatGPT 精确链接"
                : "更换当前 ChatGPT 对话"
              : isMobileViewport
                ? mobileHeading
                : "选择两端对话"}
          </h1>
          <span>
            {replaceBinding
              ? confirmingCurrentChatGptUrl
                ? `只记录 ${props.workstreamName ?? "当前工作"} 已核对的当前链接；不会更换 ChatGPT 或 Codex 对话。`
                : `只会更换 ${props.workstreamName ?? "当前工作"} 的 ChatGPT 绑定；项目和 Codex 对话不会改变。`
              : "按标题、日期和项目上下文选择；保存时使用真实 ID，不按同名自动配对。"}
          </span>
        </div>
      </header>
      <div className="v3-connection-scroll">
        {replaceBinding && (
          <aside
            className="v3-replace-chatgpt-context"
            aria-label={
              confirmingCurrentChatGptUrl ? "本次补充链接范围" : "本次更换范围"
            }
          >
            <strong>
              {confirmingCurrentChatGptUrl
                ? "本次只补充当前链接"
                : "本次只更换 ChatGPT"}
            </strong>
            <p>
              当前工作：
              {props.workstreamName ?? props.projectName ?? "当前工作台"}
            </p>
            <small>
              {confirmingCurrentChatGptUrl
                ? "当前 ChatGPT conversation_id 必须完全一致"
                : `保留 Codex：${props.activeCodexLabel ?? "当前 Codex 对话"}`}
            </small>
            <small>
              {confirmingCurrentChatGptUrl
                ? "粘贴其他对话会被拒绝；请使用“更换这组对话”。"
                : "项目关联和已有 Codex 对话均不会改动。"}
            </small>
          </aside>
        )}
        <section
          className="v3-explicit-chatgpt-binding"
          aria-label="粘贴具体 ChatGPT 对话链接"
        >
          {props.onUseCurrentChatGptConversation && !props.explicitChatGptCandidate && <section className="v3-current-browser-binding" aria-label="从 Router 浏览器选择对话">
            <h2>连接现有 ChatGPT 对话</h2><p>在 AI Work Router Browser 打开你要连接的对话，再选择下方按钮。下一步会显示具体对话，由你确认保存。</p>
            <div className="v3-control-row"><button type="button" onClick={props.onOpenHostChatGptSetup}>打开 Router 浏览器</button><button type="button" className="v3-primary" onClick={props.onUseCurrentChatGptConversation}>选择浏览器中的当前对话</button></div>
          </section>}
          <h2>
            {replaceBinding
              ? confirmingCurrentChatGptUrl
                ? "当前 ChatGPT 对话链接"
                : "新的 ChatGPT 对话链接"
              : "粘贴具体 ChatGPT 对话链接"}
          </h2>
          <p>
            {props.onUseCurrentChatGptConversation ? "也可提供你已核对的完整对话链接；确认保存前不会改变绑定。" : "请先在默认浏览器打开并核对这条精确链接。Router 不会读取、控制或自动验证该页面；确认前不会改变当前绑定。"}
          </p>
          {props.explicitChatGptCandidate ? (
            <article className="v3-explicit-chatgpt-candidate">
              <strong>{explicitCandidateIsOwnerConfirmed ? "你已人工核对的具体 ChatGPT 对话" : "Router 浏览器核对的具体 ChatGPT 对话"}</strong>
              <span>{props.explicitChatGptCandidate.label}</span>
              <small>
                conversation_id：
                <code>{props.explicitChatGptCandidate.externalId}</code>
              </small>
              <small>
                {explicitCandidateIsOwnerConfirmed ? "来源：你已人工核对这条精确链接；Router 没有读取或控制该页面。" : "来源：Router 浏览器已核对精确对话身份。"}
              </small>
              <small>
                {confirmingCurrentChatGptUrl
                  ? "确认后只保存当前对话的 canonical URL；不会更换 Endpoint。"
                  : "确认前不会改变当前绑定。"}
              </small>
              <small>
                {props.selectedExistingCodexThread
                  ? "已选择 Codex 对话；下一步会进入同一份配对复核，两个 Endpoint 将原子保存。"
                  : confirmingCurrentChatGptUrl
                    ? "不需要重新读取 Codex 项目目录。"
                    : props.explicitChatGptCandidate.expectedOldEndpointId
                      ? "确认后会替换当前 ACTIVE ChatGPT 对话，并保留历史关联。"
                      : "确认后会成为此工作流的 ACTIVE ChatGPT 对话。"}
              </small>
              {explicitConfirmationState === "CONFIRMING" && (
                <p role="status">正在确认绑定…</p>
              )}
              {explicitConfirmationState === "SUCCEEDED" && (
                <p role="status">
                  {props.explicitChatGptConfirmationMessage ??
                    (confirmingCurrentChatGptUrl
                      ? "已保存当前 ChatGPT 链接"
                      : "ChatGPT 对话已绑定")}
                </p>
              )}
              {explicitConfirmationState === "FAILED" && (
                <p className="v3-connection-error" role="alert">
                  {props.explicitChatGptConfirmationMessage ??
                    "链接没有保存。当前绑定没有改变。"}
                </p>
              )}
              <button
                type="button"
                className="v3-primary"
                disabled={
                  explicitConfirmationState === "CONFIRMING" ||
                  explicitConfirmationState === "SUCCEEDED"
                }
                onClick={() => {
                  props.onConfirmExplicitChatGptBinding?.();
                  if (props.selectedExistingCodexThread && canPair)
                    setScreen("PAIR_REVIEW");
                }}
              >
                {explicitConfirmationState === "CONFIRMING"
                  ? "正在确认绑定…"
                  : explicitConfirmationState === "SUCCEEDED"
                    ? confirmingCurrentChatGptUrl
                      ? "当前链接已保存"
                      : "ChatGPT 对话已绑定"
                    : props.selectedExistingCodexThread
                      ? "进入配对复核"
                      : confirmingCurrentChatGptUrl
                        ? "确认保存当前链接"
                        : "确认绑定"}
              </button>
            </article>
          ) : (
            <div className="v3-explicit-chatgpt-input">
              <label>
                ChatGPT 对话链接
                <input
                  aria-label="具体 ChatGPT 对话链接"
                  value={props.explicitChatGptUrl ?? ""}
                  onChange={(event) => {
                    setCurrentUrlError(null);
                    props.onExplicitChatGptUrlChange?.(event.target.value);
                  }}
                  placeholder="https://chatgpt.com/g/.../c/..."
                />
              </label>
              {currentUrlError && (
                <p className="v3-connection-error" role="alert">
                  {currentUrlError}
                </p>
              )}
              {props.onPrepareOwnerConfirmedChatGptBinding && (
                <>
                  <button
                    type="button"
                    className="v3-primary"
                    disabled={!props.explicitChatGptUrl?.trim()}
                    onClick={() => {
                      if (confirmingCurrentChatGptUrl) {
                        try {
                          const url = new URL(props.explicitChatGptUrl ?? "");
                          const segments = url.pathname
                            .split("/")
                            .filter(Boolean);
                          const supplied = segments.at(-1);
                          if (
                            url.origin !== "https://chatgpt.com" ||
                            supplied !== props.activeChatGptId
                          ) {
                            setCurrentUrlError(
                              "这不是当前已绑定的 ChatGPT 对话。若要换对话，请返回后选择“更换这组对话”。",
                            );
                            return;
                          }
                        } catch {
                          setCurrentUrlError(
                            "请输入当前已绑定 ChatGPT 对话的完整 https://chatgpt.com 链接。",
                          );
                          return;
                        }
                      }
                      props.onPrepareOwnerConfirmedChatGptBinding?.();
                    }}
                  >
                    我已在默认浏览器核对，
                    {confirmingCurrentChatGptUrl
                      ? "准备保存当前链接"
                      : "准备绑定"}
                  </button>
                  <p className="v3-connection-note">
                    此操作只记录你已核对的精确
                    URL，不会打开、读取或控制任何浏览器，也不会自动发送消息。
                  </p>
                </>
              )}
            </div>
          )}
        </section>
        {!replaceBinding && (
          <div className="v3-connection-columns">
            <ProviderColumn
              provider="CODEX"
              projectName={props.projectName}
              choices={props.choices}
            >
              <div className="v3-codex-conversation-actions">
                <button
                  type="button"
                  className="v3-primary"
                  onClick={() => {
                    setScreen("CODEX_THREAD");
                    if (props.onLoadExistingCodexThreads)
                      void props.onLoadExistingCodexThreads().catch(() => {});
                  }}
                >
                  绑定已有 Codex 对话
                </button>
                <button
                  type="button"
                  onClick={() => setScreen("PAIR_REVIEW")}
                  disabled={!canPair || explicitCandidatePending}
                >
                  核对已选 Codex 对话
                </button>
                <button type="button" onClick={() => setScreen("UNPROJECTED")}>
                  ＋ 新建无项目 Codex 对话
                </button>
              </div>
              <p className="v3-codex-conversation-detail">
                {explicitCandidatePending
                  ? "当前 ChatGPT 绑定等待复核；Codex 选择与它无关。"
                  : "对话 ID 与来源验证可在每条详情中查看。"}
              </p>
            </ProviderColumn>
          </div>
        )}
        {props.error && (
          <p className="v3-connection-error" role="alert">
            {props.error}
          </p>
        )}
      </div>
      <footer>
        <button
          type="button"
          className="v3-secondary-action v3-desktop-connection-back"
          onClick={props.onBack}
        >
          返回工作区
        </button>
        {replaceBinding ? (
          <p className="v3-pairing-reason">
            选定候选后，仍需在上方明确确认绑定；Codex 与项目不会改变。
          </p>
        ) : explicitCandidatePending ? (
          <p className="v3-pairing-reason">
            请先在上方完成或处理 ChatGPT 绑定；Codex 配对是独立操作。
          </p>
        ) : (
          <>
            <button
              type="button"
              className="v3-primary v3-mobile-connection-next"
              onClick={() => setScreen("CODEX_PROJECT")}
            >
              下一步：选择 Codex 项目
            </button>
            <div className="v3-pair-action">
              <p className="v3-pairing-reason" id="v3-pairing-reason">
                {pairingReason}
              </p>
              <button
                type="button"
                className="v3-primary"
                aria-describedby="v3-pairing-reason"
                disabled={!canPair}
                onClick={() => setScreen("PAIR_REVIEW")}
              >
                核对这组配对
              </button>
            </div>
          </>
        )}
      </footer>
    </section>
  );
}
