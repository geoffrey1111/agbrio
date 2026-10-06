import type { AttachmentCandidate } from "../domain/attachmentCandidate";

interface Props {
  candidates: AttachmentCandidate[];
  selected: Record<string, boolean>;
  onSelected: (id: string, value: boolean) => void;
  onCopyPath: (path: string) => void;
}

function formatSize(size: number | null): string {
  if (size === null) return "Unknown size";
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
  return `${(size / (1024 * 1024)).toFixed(1)} MB`;
}

function displayPath(path: string): string {
  return path.startsWith("\\\\?\\") ? path.slice(4) : path;
}

export function AttachmentCandidates({ candidates, selected, onSelected, onCopyPath }: Props) {
  if (candidates.length === 0) return null;
  return (
    <section className="attachments" aria-label="Attachments detected">
      <h2>Attachments detected</h2>
      {candidates.map((candidate) => (
        <article className="attachment" key={candidate.id}>
          <label className="attachment-title">
            <input
              type="checkbox"
              checked={selected[candidate.id] ?? candidate.default_selected}
              disabled={!candidate.exists || !candidate.is_file}
              onChange={(event) => onSelected(candidate.id, event.target.checked)}
            />
            <span>{candidate.filename}</span>
            <small>{formatSize(candidate.size)}</small>
          </label>
          <code>{displayPath(candidate.normalized_path ?? candidate.raw_path)}</code>
          <p>{candidate.detection_reason} · {candidate.confidence} confidence · SHA256 {candidate.integrity_status}</p>
          {candidate.warnings.map((warning) => <p className="warning" key={warning}>{warning}</p>)}
          <button type="button" onClick={() => onCopyPath(displayPath(candidate.normalized_path ?? candidate.raw_path))}>Copy Path</button>
        </article>
      ))}
    </section>
  );
}
