import type { ConversationMessage, ExecutionDetail, ProtocolDebugEvent } from "./presentation";
import { MarkdownMessage } from "./MarkdownMessage";

interface Props {
  conversation: ConversationMessage[];
  execution: ExecutionDetail[];
  protocolEvents: ProtocolDebugEvent[];
  onCollectMessage?: (message: ConversationMessage) => void;
  isCollected?: (message: ConversationMessage) => boolean;
}

export function CodexFeed({ conversation, execution, protocolEvents, onCollectMessage, isCollected }: Props) {
  return (
    <>
      <section className="feed" aria-live="polite" aria-label="Conversation">
        {conversation.length === 0 && <p className="empty">添加或读取一个精确 Codex thread 后，会在这里显示完整对话。</p>}
        {conversation.map((message) => (
          <article className={`message ${message.role}`} key={`${message.role}:${message.id}`}>
            <header>{message.role === "user" ? "你" : "Codex"}{message.relaySource && <small> · 由 {message.relaySource} 交接</small>}{message.streaming && <small> 正在生成…</small>}</header>
            {message.role === "assistant" ? <MarkdownMessage text={message.text || "…"} /> : <p>{message.text || "…"}</p>}
            {onCollectMessage && <button type="button" className="message-collect-action" onClick={() => onCollectMessage(message)} disabled={isCollected?.(message)}>{isCollected?.(message) ? "已收集" : "＋ 交接"}</button>}
          </article>
        ))}
      </section>
      {execution.length > 0 && (
        <details className="execution-details">
          <summary>Execution details ({execution.length})</summary>
          {execution.map((detail) => <p key={detail.id}><strong>{detail.kind}</strong> {detail.text}</p>)}
        </details>
      )}
      {protocolEvents.length > 0 && (
        <details className="protocol-events">
          <summary>Debug / Protocol events ({protocolEvents.length})</summary>
          {protocolEvents.map((event) => (
            <article key={event.id}>
              <strong>{event.method}</strong><span>{event.kind}</span><p>{event.detail}</p>
              {event.raw && <pre>{event.raw}</pre>}
            </article>
          ))}
        </details>
      )}
    </>
  );
}
