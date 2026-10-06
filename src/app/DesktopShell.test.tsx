import { afterEach, describe, expect, it } from "vitest";
import { cleanup } from "@testing-library/react";
import { fallbackSizes, normalizePaneSizes, paneBounds, preferenceKey, savedPaneSizes } from "./DesktopShell";

afterEach(() => { cleanup(); window.localStorage.clear(); });

describe("desktop pane preferences", () => {
  it("falls back safely from corrupt stored sizes", () => {
    window.localStorage.setItem(preferenceKey, "{broken");
    expect(savedPaneSizes()).toEqual(fallbackSizes);
  });

  it("rejects incomplete and negative pane preferences", () => {
    window.localStorage.setItem(preferenceKey, JSON.stringify([268, -1, 336]));
    expect(savedPaneSizes()).toEqual(fallbackSizes);
    window.localStorage.setItem(preferenceKey, JSON.stringify([268, 840]));
    expect(savedPaneSizes()).toEqual(fallbackSizes);
  });

  it("clamps syntactically valid extreme side panes so Focus remains dominant", () => {
    window.localStorage.setItem(preferenceKey, JSON.stringify([9999, 840, 9999]));
    expect(savedPaneSizes(false)).toEqual([paneBounds.glance.max, 840, paneBounds.depth.max]);
  });

  it("normalizes hidden Depth preferences at narrow desktop widths", () => {
    expect(normalizePaneSizes([260, 840, 9999], true)).toEqual([260, 840, fallbackSizes[2]]);
  });
});
