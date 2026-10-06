interface ObservationAttachmentSelectionProps {
  files: { id: string; filename: string; integrityStatus: string; warnings?: string[] }[];
  selectedIds: string[];
  onToggle: (attachmentId: string) => void;
}

export function ObservationAttachmentSelection({ files, selectedIds, onToggle }: ObservationAttachmentSelectionProps) {
  if (!files.length) return null;
  return <fieldset className="v3-codex-result-attachments">
    <legend>选择本次转交附件</legend>
    <p>只转交你勾选的文件；哈希不匹配或不可读的文件不能选择。</p>
    {files.map(file => {
      const selectable = file.integrityStatus === "VERIFIED";
      return <label key={file.id}>
        <input type="checkbox" checked={selectedIds.includes(file.id)} disabled={!selectable} onChange={() => onToggle(file.id)} />
        {file.filename} · {selectable ? "已验证" : "不可转交"}
        {file.warnings?.length ? <small>{file.warnings.join("；")}</small> : null}
      </label>;
    })}
  </fieldset>;
}
