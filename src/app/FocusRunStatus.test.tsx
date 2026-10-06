import { fireEvent, render, screen } from "@testing-library/react";
import { MotionConfig } from "motion/react";
import { describe, expect, it, vi } from "vitest";
import { FocusRunStatus } from "./FocusRunStatus";

describe("Focus run status with reduced motion", () => {
  it("keeps the selected current status and its attention action available", () => {
    window.matchMedia = vi.fn((query: string) => ({
      matches: query === "(prefers-reduced-motion: reduce)", media: query, onchange: null,
      addEventListener: () => undefined, removeEventListener: () => undefined,
      addListener: () => undefined, removeListener: () => undefined, dispatchEvent: () => false,
    })) as typeof window.matchMedia;
    const openAttention = vi.fn();

    render(<MotionConfig reducedMotion="user"><FocusRunStatus codexStatus="RUNNING" chatGptStatus="Bound" codexConnected chatGptConnected={false} attentionCount={1} onOpenAttention={openAttention} /></MotionConfig>);

    expect(screen.getByText("Codex RUNNING")).toBeVisible();
    const action = screen.getByRole("button", { name: "1 needs attention · Open context" });
    expect(action).toBeEnabled();
    fireEvent.click(action);
    expect(openAttention).toHaveBeenCalledOnce();
  });
});
