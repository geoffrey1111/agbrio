import { useMemo, useState } from "react";
import { ArrowRight, ChevronRight, UserRound } from "lucide-react";
import type {
  BackendStatus,
  ChatGptStatus,
  DashboardProjection,
  WorkstreamDashboardItem,
} from "../features/codex/types";

type Props = {
  dashboard?: DashboardProjection;
  codex: BackendStatus;
  chatGpt: ChatGptStatus;
  onOpen: (item: WorkstreamDashboardItem) => void;
  readOnly?: boolean;
};

function isDevelopmentFixture(value: string | undefined) {
  return Boolean(
    value && /^(V0-\d+|CODEX-(?:SAFETY|SUCCESS)|CHATGPT-MANUAL|Disposable ChatGPT|AI Work Router Setup)/i.test(value),
  );
}

function displayBoardTitle(item: WorkstreamDashboardItem) {
  const label = item.chatgptEndpoint?.label ?? item.codexEndpoint?.label ?? item.workstream.name;
  if (!isDevelopmentFixture(label) && !isDevelopmentFixture(item.projectName)) return item.workstream.name;
  if (/SAFETY/i.test(item.workstream.name)) return "开发测试 · 安全路径";
  if (/SUCCESS/i.test(item.workstream.name)) return "开发测试 · 成功路径";
  if (/MANUAL/i.test(item.workstream.name)) return "开发测试 · 手动会话";
  if (/AI Work Router Setup/i.test(label)) return "开发测试 · 初始化";
  return "开发测试对话";
}

function displayBoardProject(item: WorkstreamDashboardItem) {
  return isDevelopmentFixture(item.projectName) ? "开发与验收记录" : item.projectName;
}

function displayBoardEndpoint(label: string | undefined) {
  if (!label) return "未绑定";
  if (/^Codex\s+[a-z0-9]/i.test(label)) return "已绑定 Codex 对话";
  if (!isDevelopmentFixture(label)) return label;
  if (/SAFETY/i.test(label)) return "安全路径";
  if (/SUCCESS/i.test(label)) return "成功路径";
  if (/MANUAL/i.test(label)) return "手动会话";
  if (/AI Work Router Setup/i.test(label)) return "开发初始化";
  return "开发测试对话";
}

function waitingCopy(item: WorkstreamDashboardItem) {
  if (item.codexRun?.originHandoffId) {
    if (
      item.codexRun.status === "RUNNING" ||
      item.codexRun.status === "STARTING"
    )
      return "交接已发送 · Codex 执行中";
    if (item.codexRun.status === "COMPLETED") return "交接已发送 · 执行完成";
    if (
      item.codexRun.status === "FAILED" ||
      item.codexRun.status === "CANCELLED"
    )
      return "交接已发送 · 执行未完成";
    return "交接已发送 · 等待执行";
  }
  const run =
    item.codexRun?.status === "RUNNING" || item.codexRun?.status === "STARTING"
      ? "Codex 正在执行"
      : item.chatgptRun?.status === "RUNNING" ||
          item.chatgptRun?.status === "STARTING"
        ? "ChatGPT 正在思考"
        : item.attentionItems.length
          ? "等待你审批"
          : item.codexRun?.status === "COMPLETED"
            ? "执行完成"
            : item.chatgptRun?.status === "COMPLETED"
              ? "等待 Codex"
              : item.codexRun?.status === "UNKNOWN" ||
                  item.chatgptRun?.status === "UNKNOWN"
                ? "状态未知"
                : !item.chatgptEndpoint || !item.codexEndpoint
                  ? "连接中断"
                  : "等待 ChatGPT";
  return run;
}

function providerCopy(
  item: WorkstreamDashboardItem,
  provider: "CHATGPT" | "CODEX",
  bridgeConnected: boolean,
) {
  const endpoint =
    provider === "CHATGPT" ? item.chatgptEndpoint : item.codexEndpoint;
  const run = provider === "CHATGPT" ? item.chatgptRun : item.codexRun;
  if (!endpoint) return { text: "未绑定", tone: "offline" };
  if (!bridgeConnected) return { text: "连接中断", tone: "offline" };
  if (run?.status === "UNKNOWN") return { text: "状态未知", tone: "offline" };
  if (run?.status === "RUNNING" || run?.status === "STARTING")
    return {
      text: provider === "CHATGPT" ? "正在思考" : "正在执行",
      tone: "online",
    };
  if (run?.status === "COMPLETED") return { text: "已完成", tone: "online" };
  if (run?.status === "FAILED" || run?.status === "CANCELLED")
    return { text: "执行未完成", tone: "offline" };
  return {
    text: provider === "CHATGPT" ? "等待 ChatGPT" : "等待 Codex",
    tone: "online",
  };
}

export function RouterBoard({ dashboard, codex, chatGpt, onOpen, readOnly = false }: Props) {
  const items = dashboard?.workstreams ?? [];
  const [selectedId, setSelectedId] = useState<string>();
  const selected =
    items.find((item) => item.workstream.id === selectedId) ?? items[0];
  const waiting = useMemo(
    () => items.filter((item) => waitingCopy(item) !== "执行完成"),
    [items],
  );
  return (
    <section className="router-board" aria-label="Router Board">
      <header className="router-board-header">
        <div>
          <p>总览</p>
          <h2>下一步是谁？</h2>
          <span>
            关系与等待状态只是投影；不会改变工作流、已绑定对话或交接记录。
          </span>
        </div>
        <div className="provider-inventory" aria-label="Provider inventory">
          <span className={codex.connected ? "provider-live" : "provider-offline"}>
            Codex · {codex.connected ? "已连接" : "已断开"}
          </span>
          <span
            className={chatGpt.connected ? "provider-live" : "provider-offline"}
          >
            ChatGPT ·{" "}
            {chatGpt.connected ? `${chatGpt.clients} 个兼容连接` : "未连接"}
          </span>
        </div>
      </header>
      <div className="router-board-grid">
        <aside className="board-attention">
          <h3>等待队列</h3>
          {waiting.length ? (
            waiting.map((item) => (
              <button
                type="button"
                key={item.workstream.id}
                className={
                  selected?.workstream.id === item.workstream.id
                    ? "selected"
                    : ""
                }
                onClick={() => setSelectedId(item.workstream.id)}
              >
                <i aria-hidden="true" />
                <span>
                  <strong>{displayBoardTitle(item)}</strong>
                  <small>{waitingCopy(item)}</small>
                </span>
                <ChevronRight size={14} aria-hidden="true" />
              </button>
            ))
          ) : (
            <p>没有需要处理的工作会话。</p>
          )}
        </aside>
        <section
          className="relationship-lanes"
          aria-label="工作会话关系"
        >
          {items.map((item) => {
            const chatGptState = providerCopy(
              item,
              "CHATGPT",
              chatGpt.connected,
            );
            const codexState = providerCopy(item, "CODEX", codex.connected);
            return (
              <article
                key={item.workstream.id}
                className={`relationship-lane${selected?.workstream.id === item.workstream.id ? " selected" : ""}`}
              >
                <button
                  type="button"
                  className="relationship-select"
                  onClick={() => setSelectedId(item.workstream.id)}
                >
                  <header>
                    <span>{displayBoardProject(item)}</span>
                    <strong>{displayBoardTitle(item)}</strong>
                    <em>{waitingCopy(item)}</em>
                  </header>
                  <div className="lane-flow">
                    <div>
                      <small>ChatGPT</small>
                      <b>{displayBoardEndpoint(item.chatgptEndpoint?.label)}</b>
                      <span className={chatGptState.tone}>
                        {chatGptState.text}
                      </span>
                    </div>
                    <i className="lane-link" aria-hidden="true">
                      <ArrowRight size={15} />
                    </i>
                    <div className="handoff-node">
                      <small>交接</small>
                      <b>
                        {item.codexRun?.originHandoffId
                          ? "已发送"
                          : item.attentionItems.length
                            ? "待审查"
                            : "准备中"}
                      </b>
                      <span>
                        {item.codexRun?.originHandoffId
                          ? "等待执行"
                          : "人工批准"}
                      </span>
                    </div>
                    <i className="lane-link" aria-hidden="true">
                      <ArrowRight size={15} />
                    </i>
                    <div>
                      <small>Codex</small>
                      <b>{displayBoardEndpoint(item.codexEndpoint?.label)}</b>
                      <span className={codexState.tone}>{codexState.text}</span>
                    </div>
                  </div>
                </button>
                <button
                  type="button"
                  className="lane-open"
                  onClick={() => onOpen(item)}
                  disabled={readOnly}
                >
                  打开会话 <ChevronRight size={14} aria-hidden="true" />
                </button>
              </article>
            );
          })}
        </section>
        <aside
          className="board-detail"
          aria-label="Selected relationship detail"
        >
          {selected ? (
            <>
              <p>已选关系</p>
              <h3>{displayBoardTitle(selected)}</h3>
              <strong>{waitingCopy(selected)}</strong>
              <dl>
                <div>
                  <dt>ChatGPT</dt>
                  <dd>{displayBoardEndpoint(selected.chatgptEndpoint?.label)}</dd>
                </div>
                <div>
                  <dt>Codex</dt>
                  <dd>{displayBoardEndpoint(selected.codexEndpoint?.label)}</dd>
                </div>
                <div>
                  <dt>运行</dt>
                  <dd>
                    {waitingCopy(selected)}
                  </dd>
                </div>
              </dl>
              <button type="button" onClick={() => onOpen(selected)} disabled={readOnly}>
                打开会话 <ChevronRight size={15} aria-hidden="true" />
              </button>
            </>
          ) : (
            <p>选择一个工作会话查看关系。</p>
          )}
        </aside>
      </div>
      <footer>
        <UserRound size={14} aria-hidden="true" /> 人工门禁：所有交接仍需
        审查、编辑与明确批准，总览只解释“谁在等谁”。
      </footer>
    </section>
  );
}
