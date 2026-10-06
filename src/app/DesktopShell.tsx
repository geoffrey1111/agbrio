import { Allotment, LayoutPriority } from "allotment";
import { useEffect, useState, type ReactNode } from "react";

type DesktopShellProps = {
  navigation: ReactNode;
  focus: ReactNode;
  depth: ReactNode;
};

export const preferenceKey = "ai-work-router.v0-007.desktop-pane-sizes";
export const fallbackSizes = [268, 840, 336];
export const paneBounds = {
  glance: { min: 220, max: 350 },
  focus: { min: 520 },
  depth: { min: 270, max: 450 },
} as const;

function isUsablePaneSizes(value: unknown): value is [number, number, number] {
  return Array.isArray(value) && value.length === 3 && value.every((size) => typeof size === "number" && Number.isFinite(size) && size > 0);
}

function clamp(value: number, bounds: { min: number; max?: number }) {
  return Math.min(bounds.max ?? Number.POSITIVE_INFINITY, Math.max(bounds.min, value));
}

/** Keeps local geometry useful without letting an old preference crowd out Focus. */
export function normalizePaneSizes(value: unknown, narrowDesktop = false): number[] {
  if (!isUsablePaneSizes(value)) return [...fallbackSizes];
  const [glance, focus, depth] = value;
  return [
    clamp(glance, paneBounds.glance),
    Math.max(paneBounds.focus.min, focus),
    narrowDesktop ? fallbackSizes[2] : clamp(depth, paneBounds.depth),
  ];
}

export function savedPaneSizes(narrowDesktop = window.matchMedia("(max-width: 1320px)").matches): number[] {
  try {
    const parsed: unknown = JSON.parse(window.localStorage.getItem(preferenceKey) ?? "null");
    return normalizePaneSizes(parsed, narrowDesktop);
  } catch { return [...fallbackSizes]; }
}

/** The only geometry owner for the desktop Glance → Focus → Depth layout. */
export function DesktopShell({ navigation, focus, depth }: DesktopShellProps) {
  const [paneSizes, setPaneSizes] = useState(savedPaneSizes);
  const [narrowDesktop, setNarrowDesktop] = useState(() => window.matchMedia("(max-width: 1320px)").matches);
  useEffect(() => {
    const media = window.matchMedia("(max-width: 1320px)");
    const update = () => {
      setNarrowDesktop(media.matches);
      setPaneSizes((current) => normalizePaneSizes(current, media.matches));
    };
    media.addEventListener("change", update);
    return () => media.removeEventListener("change", update);
  }, []);
  const persistPaneSizes = (sizes: number[]) => {
    if (!isUsablePaneSizes(sizes)) return;
    const normalized = normalizePaneSizes(sizes, narrowDesktop);
    setPaneSizes(normalized);
    try { window.localStorage.setItem(preferenceKey, JSON.stringify(normalized)); } catch { /* preferences are non-critical */ }
  };

  return <Allotment className="desktop-allotment" defaultSizes={paneSizes} minSize={180} separator onDragEnd={persistPaneSizes}>
    <Allotment.Pane minSize={paneBounds.glance.min} maxSize={paneBounds.glance.max} preferredSize={paneSizes[0]}>{navigation}</Allotment.Pane>
    <Allotment.Pane minSize={paneBounds.focus.min} preferredSize={paneSizes[1]} priority={LayoutPriority.High}>{focus}</Allotment.Pane>
    <Allotment.Pane minSize={paneBounds.depth.min} maxSize={paneBounds.depth.max} preferredSize={paneSizes[2]} visible={!narrowDesktop}>{depth}</Allotment.Pane>
  </Allotment>;
}
