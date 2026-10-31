import {getLanguage, useLanguage} from "../../i18n";
import {modelBadgeView, type ConversationRuntime} from "./modelLabel";

/** Small "model · effort" chip for list rows and reader headers. Never part of the title. */
export function ModelBadge({runtime, className}: {runtime?: ConversationRuntime | null; className?: string}) {
  useLanguage();
  const view = modelBadgeView(runtime, getLanguage());
  return (
    <span className={`v7-model${className ? ` ${className}` : ""}`} data-unknown={view.unknown || undefined} title={view.title}>
      <span className="v7-sr">{view.accessible}</span>
      <span aria-hidden="true">{view.text}</span>
    </span>
  );
}
