import type {MediaResolver} from "../codex/messageMedia";
import { MarkdownMessage } from "../codex/MarkdownMessage";
import type { RelayTextBlock } from "./RoleBridgePanel";

export function RelayBlockSelection({ blocks, selected, disabled, onChange, media }: {
  blocks: RelayTextBlock[]; selected: string[]; disabled: boolean; onChange: (ids: string[]) => void;media?:MediaResolver;
}) {
  function toggle(id: string) {
    if (!disabled) onChange(selected.includes(id) ? selected.filter(item => item !== id) : [...selected, id]);
  }
  return <section className="v4-relay-blocks" aria-label="选择原文内容">
    <p className="v4-meta">点选整段，可多选。</p>
    <div className="v4-relay-block-list">{blocks.map((block, index) => {
      const checked = selected.includes(block.id);
      const name = `第 ${index + 1} 段${block.recommended ? " · 推荐指令" : ""}`;
      return <div key={block.id} role="group" aria-label={name} className={`v4-relay-block${checked ? " is-selected" : ""}`} onClick={event => {
        // Reading links and selecting/copying text must not change the payload.
        if ((event.target as Element).closest("a,button,input")) return;
        const selection = window.getSelection();
        if (selection && !selection.isCollapsed && selection.anchorNode && event.currentTarget.contains(selection.anchorNode)) return;
        toggle(block.id);
      }}>
        <div className="v4-relay-block-heading"><input type="checkbox" aria-label={name} disabled={disabled} checked={checked} onChange={() => toggle(block.id)}/><span>{block.kind==="INSTRUCTION"?"指令":block.kind==="CODE"?"代码":"正文"}{block.recommended && <small>建议转发</small>}</span></div>
        {block.kind==="CODE"&&!block.text.trimStart().startsWith("```")?<pre className="r2-block-code">{block.text}</pre>:<MarkdownMessage text={block.text} media={media}/>}
      </div>;
    })}</div>
  </section>;
}
