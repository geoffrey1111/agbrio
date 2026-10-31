/**
 * Presentation helpers for "model · reasoning effort".
 * Input comes only from native/selected runtime facts supplied by the Host
 * (proposed `runtime` field on CodexWatch / WatchChatState). Titles and message
 * text are never parsed for this. Missing values render as "unknown".
 */
export type RuntimeSource = "NATIVE" | "SELECTED" | "DEFAULT";
export type ConversationRuntime = {
  model: string | null;
  modelLabel?: string | null;
  effort: string | null;
  source?: RuntimeSource | null;
  observedAt?: number | null;
};
type Lang = "zh-CN" | "zh-TW" | "en";

// Local catalog until the shared i18n catalogs can be edited on the committed baseline.
const copy: Record<Lang, Record<string, string>> = {
  "zh-CN": { unknownModel: "模型未知", unknown: "未知", model: "模型", effort: "推理强度", NATIVE: "原生记录", SELECTED: "明确选择", DEFAULT: "默认设置" },
  "zh-TW": { unknownModel: "模型未知", unknown: "未知", model: "模型", effort: "推理強度", NATIVE: "原生記錄", SELECTED: "明確選擇", DEFAULT: "預設設定" },
  en: { unknownModel: "Model unknown", unknown: "unknown", model: "Model", effort: "Reasoning", NATIVE: "native record", SELECTED: "explicitly selected", DEFAULT: "default" },
};
export const modelCopy = (lang: string, key: string) => (copy[(lang in copy ? lang : "zh-CN") as Lang][key] ?? key);

const clean = (v: string | null | undefined) => {
  const s = typeof v === "string" ? v.trim() : "";
  return s ? s : null;
};
const cap = (s: string) => (s ? s[0].toUpperCase() + s.slice(1) : s);

/** "claude-opus-5-5" → "Opus 5.5", "gpt-6.1-sol" → "GPT-6.1 Sol". Unknown shapes stay verbatim. */
export function prettyModelName(id: string): string {
  let s = id.trim();
  s = s.slice(s.lastIndexOf("/") + 1); // provider prefix "anthropic/…"
  s = s.replace(/\[[^\]]*\]$/, "").replace(/-\d{8}$/, "").replace(/-latest$/, "");
  const lower = s.toLowerCase();
  let m = /^claude-(opus|sonnet|haiku)-(\d+)(?:[-.](\d+))?$/.exec(lower);
  if (m) return `${cap(m[1])} ${m[2]}${m[3] ? "." + m[3] : ""}`;
  m = /^claude-(\d+)(?:[-.](\d+))?-(opus|sonnet|haiku)$/.exec(lower);
  if (m) return `${cap(m[3])} ${m[1]}${m[2] ? "." + m[2] : ""}`;
  m = /^gpt-(\d+(?:\.\d+)?)((?:-[a-z0-9]+)*)$/.exec(lower);
  if (m) return `GPT-${m[1]}${m[2] ? " " + m[2].slice(1).split("-").map(cap).join(" ") : ""}`;
  return s || id;
}

/** Pi tasks already expose the checked creation settings; adapt them to the shared shape. */
export function runtimeFromPiSettings(
  settings: { provider: string; model: string; thinking: string } | null | undefined,
  selectionSource?: string | null,
): ConversationRuntime | null {
  if (!settings) return null;
  return { model: clean(settings.model), effort: clean(settings.thinking), source: selectionSource === "DEFAULT" ? "DEFAULT" : "SELECTED" };
}

type RuntimeCarrier = { runtime?: ConversationRuntime | null };
const projected = (runtime: ConversationRuntime | null | undefined): ConversationRuntime | null =>
  runtime && typeof runtime === "object" ? runtime : null;

/** Runtime for a list row: the Host's `watch.runtime` projection (native/selected facts only). */
export function watchRuntime(watch: ({ threadId: string } & RuntimeCarrier) | null | undefined): ConversationRuntime | null {
  return projected(watch?.runtime);
}

/** Runtime for an open conversation: `state.runtime` first, then the Pi creation settings it already carries. */
export function chatRuntime(state: (RuntimeCarrier & { piTask?: { settings: { provider: string; model: string; thinking: string } | null; selectionSource?: string | null } | null }) | null | undefined): ConversationRuntime | null {
  return projected(state?.runtime) ?? runtimeFromPiSettings(state?.piTask?.settings, state?.piTask?.selectionSource);
}

export type ModelBadgeView = { text: string; name: string | null; effort: string | null; unknown: boolean; accessible: string; title: string };
export function modelBadgeView(runtime: ConversationRuntime | null | undefined, lang: string): ModelBadgeView {
  const t = (k: string) => modelCopy(lang, k);
  const raw = clean(runtime?.model);
  const name = clean(runtime?.modelLabel) ?? (raw ? prettyModelName(raw) : null);
  const effort = clean(runtime?.effort);
  const unknown = !name && !effort;
  const text = unknown ? t("unknownModel") : !effort ? name! : `${name ?? t("unknownModel")} · ${effort}`;
  const accessible = `${t("model")} ${name ?? t("unknown")}，${t("effort")} ${effort ?? t("unknown")}`.replace("，", lang === "en" ? ", " : "，");
  const source = runtime?.source ? t(runtime.source) : null;
  const title = [raw && raw !== name ? raw : null, source].filter(Boolean).join(" · ") || accessible;
  return { text, name, effort, unknown, accessible, title };
}
