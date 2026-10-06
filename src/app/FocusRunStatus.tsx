type Props = {
  codexStatus: string;
  chatGptStatus: string;
  codexConnected: boolean;
  chatGptConnected: boolean;
  attentionCount: number;
  onOpenAttention: () => void;
};

/** Status remains textual and actionable even when MotionConfig reduces animation. */
export function FocusRunStatus({ codexStatus, chatGptStatus, codexConnected, chatGptConnected, attentionCount, onOpenAttention }: Props) {
  return <div className="focus-status" aria-label="Current provider status">
    <span className={codexConnected ? "connected" : "warning"}>Codex {codexStatus}</span>
    <span className={chatGptConnected ? "connected" : "warning"}>ChatGPT {chatGptStatus}</span>
    {attentionCount > 0 && <button type="button" className="attention-status" onClick={onOpenAttention}>{attentionCount} needs attention · Open context</button>}
  </div>;
}
