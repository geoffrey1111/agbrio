import { useCallback, useEffect, useRef, useState } from "react";
import type { WorkbenchDraft } from "./models";

export type PersistedDraft = {
  workstreamId: string;
  text: string;
  revision: number;
  updatedAt: number;
};

type SaveDraft = (workstreamId: string, text: string, expectedRevision: number | null) => Promise<PersistedDraft>;

type Projection = {
  workstreamId: string;
  generation: number;
  state?: WorkbenchDraft["saveState"];
  savedAt: number | null;
};

/**
 * Keeps the V3 draft indicator attached to the exact workstream and edit
 * revision that caused it. A late response can never certify a newer draft or
 * a different workstream as saved.
 */
export function useDraftAutosaveProjection(save: SaveDraft) {
  const saveRef = useRef(save);
  saveRef.current = save;
  const generation = useRef(0);
  const [draft, setDraft] = useState("");
  const [revision, setRevision] = useState<number | null>(null);
  const [projection, setProjection] = useState<Projection | null>(null);
  const inFlight = useRef(false);
  const [saveCycle, setSaveCycle] = useState(0);

  const beginLoad = useCallback((workstreamId: string) => {
    const nextGeneration = ++generation.current;
    setDraft("");
    setRevision(null);
    setProjection({ workstreamId, generation: nextGeneration, savedAt: null });
    return nextGeneration;
  }, []);

  const hydrate = useCallback((workstreamId: string, persisted: PersistedDraft | null, loadGeneration: number) => {
    setProjection((current) => {
      if (!current || current.workstreamId !== workstreamId || current.generation !== loadGeneration) return current;
      setDraft(persisted?.text ?? "");
      setRevision(persisted?.revision ?? null);
      return persisted
        ? { ...current, state: "SAVED", savedAt: persisted.updatedAt }
        : { ...current, state: undefined, savedAt: null };
    });
  }, []);

  const changeDraft = useCallback((text: string) => {
    setProjection((current) => {
      if (!current) return current;
      const nextGeneration = ++generation.current;
      return { workstreamId: current.workstreamId, generation: nextGeneration, state: "SAVING", savedAt: null };
    });
    setDraft(text);
  }, []);

  useEffect(() => {
    if (!projection || projection.state !== "SAVING") return;
    const { workstreamId, generation: editGeneration } = projection;
    const expectedRevision = revision;
    const text = draft;
    const timer = window.setTimeout(() => {
      // The Core revision is a single-writer guard. Do not issue a second
      // request against an older revision while the first request is still
      // resolving; retry the newest local edit after it settles instead.
      if (inFlight.current) {
        setSaveCycle((current) => current + 1);
        return;
      }
      inFlight.current = true;
      void saveRef.current(workstreamId, text, expectedRevision).then((saved) => {
        setProjection((current) => {
          if (!current || current.workstreamId !== workstreamId) return current;
          if (saved.workstreamId !== workstreamId) return { ...current, state: "FAILED", savedAt: null };
          // A prior request may complete after a newer local edit. Its revision
          // is still authoritative for that same workstream, but it must never
          // turn the newer draft into a false SAVED projection.
          setRevision(saved.revision);
          if (current.generation !== editGeneration) return current;
          return { ...current, state: "SAVED", savedAt: saved.updatedAt };
        });
      }).catch(() => {
        setProjection((current) => current && current.workstreamId === workstreamId && current.generation === editGeneration
          ? { ...current, state: "FAILED", savedAt: null }
          : current);
      }).finally(() => {
        inFlight.current = false;
        setSaveCycle((current) => current + 1);
      });
    }, 500);
    return () => window.clearTimeout(timer);
  }, [draft, projection, revision, saveCycle]);

  const visibleDraft = projection ? {
    value: draft,
    revision,
    savedAt: projection.savedAt,
    saveState: projection.state,
  } satisfies WorkbenchDraft : { value: draft, revision } satisfies WorkbenchDraft;

  return { beginLoad, changeDraft, draft: visibleDraft, hydrate };
}
