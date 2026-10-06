import { useCallback, useEffect, useRef, useState } from "react";

export type ResultScopedPersistedDraft = {
  text: string;
  revision: number;
  updatedAt: number;
};

export type ResultScopedDraft = {
  value: string;
  revision: number | null;
  saveState?: "LOADING" | "SAVING" | "SAVED" | "FAILED";
  savedAt: number | null;
};

type DraftRecord = ResultScopedDraft & {
  localGeneration: number;
  inFlight: boolean;
};

const emptyDraft = (): DraftRecord => ({
  value: "",
  revision: null,
  savedAt: null,
  localGeneration: 0,
  inFlight: false,
});

/**
 * Keeps multiple direct-feedback drafts durable without treating their source
 * run IDs as interchangeable. Each scope is independently revision-guarded,
 * so returning from one result and opening another cannot cancel or overwrite
 * the first result's pending autosave.
 */
export function useResultScopedDraftAutosave(
  read: (scopeId: string) => Promise<ResultScopedPersistedDraft | null>,
  save: (scopeId: string, text: string, expectedRevision: number | null) => Promise<ResultScopedPersistedDraft>,
) {
  const readRef = useRef(read);
  const saveRef = useRef(save);
  readRef.current = read;
  saveRef.current = save;
  const records = useRef<Record<string, DraftRecord>>({});
  const timers = useRef<Record<string, number>>({});
  const [drafts, setDrafts] = useState<Record<string, DraftRecord>>({});

  const publish = useCallback((scopeId: string, next: DraftRecord) => {
    records.current = { ...records.current, [scopeId]: next };
    setDrafts(records.current);
  }, []);

  const clearTimer = useCallback((scopeId: string) => {
    const timer = timers.current[scopeId];
    if (timer !== undefined) window.clearTimeout(timer);
    delete timers.current[scopeId];
  }, []);

  const persist = useCallback((scopeId: string) => {
    const current = records.current[scopeId];
    if (!current || current.inFlight || current.saveState !== "SAVING") return;
    const submittedGeneration = current.localGeneration;
    const submittedText = current.value;
    const submittedRevision = current.revision;
    publish(scopeId, { ...current, inFlight: true });
    void saveRef.current(scopeId, submittedText, submittedRevision).then((saved) => {
      const latest = records.current[scopeId];
      if (!latest) return;
      const changedDuringSave = latest.localGeneration !== submittedGeneration;
      publish(scopeId, {
        ...latest,
        revision: saved.revision,
        savedAt: changedDuringSave ? latest.savedAt : saved.updatedAt,
        saveState: changedDuringSave ? "SAVING" : "SAVED",
        inFlight: false,
      });
      if (changedDuringSave) {
        clearTimer(scopeId);
        timers.current[scopeId] = window.setTimeout(() => persist(scopeId), 0);
      }
    }).catch(() => {
      const latest = records.current[scopeId];
      if (!latest) return;
      const changedDuringSave = latest.localGeneration !== submittedGeneration;
      publish(scopeId, {
        ...latest,
        inFlight: false,
        saveState: changedDuringSave ? "SAVING" : "FAILED",
      });
      if (changedDuringSave) {
        clearTimer(scopeId);
        timers.current[scopeId] = window.setTimeout(() => persist(scopeId), 0);
      }
    });
  }, [clearTimer, publish]);

  const schedule = useCallback((scopeId: string, delay = 500) => {
    clearTimer(scopeId);
    timers.current[scopeId] = window.setTimeout(() => {
      delete timers.current[scopeId];
      persist(scopeId);
    }, delay);
  }, [clearTimer, persist]);

  const load = useCallback((scopeId: string) => {
    const current = records.current[scopeId] ?? emptyDraft();
    if (!records.current[scopeId]) publish(scopeId, { ...current, saveState: "LOADING" });
    const loadGeneration = current.localGeneration;
    void readRef.current(scopeId).then((saved) => {
      const latest = records.current[scopeId];
      if (!latest || latest.localGeneration !== loadGeneration || latest.saveState === "SAVING") return;
      publish(scopeId, saved
        ? { ...latest, value: saved.text, revision: saved.revision, savedAt: saved.updatedAt, saveState: "SAVED" }
        : { ...latest, saveState: undefined, savedAt: null });
    }).catch(() => {
      const latest = records.current[scopeId];
      if (latest && latest.localGeneration === loadGeneration && latest.saveState !== "SAVING") {
        publish(scopeId, { ...latest, saveState: "FAILED" });
      }
    });
  }, [publish]);

  const change = useCallback((scopeId: string, value: string) => {
    const current = records.current[scopeId] ?? emptyDraft();
    publish(scopeId, {
      ...current,
      value,
      localGeneration: current.localGeneration + 1,
      savedAt: null,
      saveState: "SAVING",
    });
    schedule(scopeId);
  }, [publish, schedule]);

  const flush = useCallback((scopeId: string) => {
    const current = records.current[scopeId];
    if (!current || current.saveState !== "SAVING") return;
    schedule(scopeId, 0);
  }, [schedule]);

  useEffect(() => () => {
    Object.values(timers.current).forEach((timer) => window.clearTimeout(timer));
  }, []);

  return {
    load,
    change,
    flush,
    draft: (scopeId: string | null): ResultScopedDraft => {
      const current = scopeId ? drafts[scopeId] : undefined;
      return current ?? emptyDraft();
    },
  };
}
