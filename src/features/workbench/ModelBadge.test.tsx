import {cleanup, render, screen} from "@testing-library/react";
import {afterEach, describe, expect, it} from "vitest";
import {ModelBadge} from "./ModelBadge";
import {chatRuntime, modelBadgeView, prettyModelName, runtimeFromPiSettings, watchRuntime} from "./modelLabel";
import {setLanguagePreference} from "../../i18n";

afterEach(() => { cleanup(); setLanguagePreference("zh-CN"); });

describe("prettyModelName", () => {
  it.each([
    ["claude-opus-5-5", "Opus 5.5"],
    ["anthropic/claude-opus-5-5", "Opus 5.5"],
    ["claude-sonnet-4-5-20250929", "Sonnet 4.5"],
    ["claude-opus-4-1[1m]", "Opus 4.1"],
    ["claude-3-5-haiku", "Haiku 3.5"],
    ["gpt-6.1-sol", "GPT-6.1 Sol"],
    ["gpt-6-luna", "GPT-6 Luna"],
    ["gpt-5.1-codex-max", "GPT-5.1 Codex Max"],
    ["o4-mini", "o4-mini"],
    ["some-new-model", "some-new-model"],
  ])("%s → %s", (id, expected) => expect(prettyModelName(id)).toBe(expected));
});

describe("modelBadgeView", () => {
  it("prefers the catalog display label and keeps the raw effort", () => {
    expect(modelBadgeView({model: "gpt-6.1-sol", modelLabel: "GPT-6.1 Sol", effort: "high", source: "NATIVE"}, "zh-CN"))
      .toMatchObject({text: "GPT-6.1 Sol · high", unknown: false, title: "gpt-6.1-sol · 原生记录"});
  });
  it("shows only the model name when effort is null, and marks a missing model", () => {
    expect(modelBadgeView({model: "claude-opus-5-5", effort: null}, "zh-CN").text).toBe("Opus 5.5");
    expect(modelBadgeView({model: "claude-opus-5-5", modelLabel: "Opus 5.5", effort: null, source: "NATIVE"}, "zh-CN").text).not.toContain("null");
    expect(modelBadgeView({model: null, effort: "high"}, "zh-CN").text).toBe("模型未知 · high");
  });
  it("shows one unknown label when nothing is known, including blank strings", () => {
    for (const runtime of [undefined, null, {model: " ", effort: ""}]) {
      expect(modelBadgeView(runtime, "zh-CN")).toMatchObject({text: "模型未知", unknown: true});
    }
  });
  it("adapts checked Pi creation settings", () => {
    const runtime = runtimeFromPiSettings({provider: "anthropic", model: "claude-opus-5-5", thinking: "high"}, "EXPLICIT");
    expect(runtime).toEqual({model: "claude-opus-5-5", effort: "high", source: "SELECTED"});
    expect(modelBadgeView(runtime, "zh-CN").text).toBe("Opus 5.5 · high");
    expect(runtimeFromPiSettings(null)).toBeNull();
    expect(runtimeFromPiSettings({provider: "x", model: "m", thinking: "low"}, "DEFAULT")?.source).toBe("DEFAULT");
  });
});

describe("runtime projection", () => {
  const settings = {provider: "anthropic", model: "claude-opus-5-5", thinking: "high"};
  it("reads watch.runtime for list rows", () => {
    const runtime = {model: "gpt-6.1-sol", modelLabel: "GPT-6.1 Sol", effort: "medium", source: "NATIVE" as const, observedAt: 1};
    expect(watchRuntime({threadId: "codex-1", runtime})).toBe(runtime);
    expect(modelBadgeView(watchRuntime({threadId: "codex-1", runtime}), "zh-CN").text).toBe("GPT-6.1 Sol · medium");
  });
  it("reads a Claude runtime without effort as the model name only", () => {
    const runtime = {model: "claude-opus-5-5", modelLabel: null, effort: null, source: "NATIVE" as const, observedAt: 1};
    expect(modelBadgeView(watchRuntime({threadId: "claude_code:1", runtime}), "zh-CN").text).toBe("Opus 5.5");
  });
  it("keeps model unknown when the watch carries no runtime", () => {
    for (const watch of [{threadId: "a"}, {threadId: "b", runtime: null}, null, undefined]) {
      expect(watchRuntime(watch)).toBeNull();
      expect(modelBadgeView(watchRuntime(watch), "zh-CN")).toMatchObject({text: "模型未知", unknown: true});
    }
    expect(modelBadgeView({model: null, modelLabel: null, effort: null, source: "DEFAULT", observedAt: 1}, "zh-CN")).toMatchObject({text: "模型未知", unknown: true});
  });
  it("prefers state.runtime and falls back to Pi creation settings", () => {
    const runtime = {model: "claude-sonnet-5", modelLabel: "Sonnet 5", effort: "low", source: "SELECTED" as const, observedAt: 2};
    expect(chatRuntime({runtime, piTask: {settings, selectionSource: "EXPLICIT"}})).toBe(runtime);
    expect(chatRuntime({runtime: null, piTask: {settings, selectionSource: "EXPLICIT"}})).toEqual({model: "claude-opus-5-5", effort: "high", source: "SELECTED"});
    expect(chatRuntime({})).toBeNull();
    expect(chatRuntime(null)).toBeNull();
  });
});

describe("ModelBadge", () => {
  it("renders the visible chip with an accessible full description", () => {
    const {container} = render(<ModelBadge runtime={{model: "claude-opus-5-5", effort: "high"}}/>);
    expect(container.querySelector(".v7-model")).toHaveTextContent("Opus 5.5 · high");
    expect(screen.getByText("模型 Opus 5.5，推理强度 high")).toHaveClass("v7-sr");
    expect(container.querySelector(".v7-model")).not.toHaveAttribute("data-unknown");
  });
  it("renders the unknown state distinctly", () => {
    const {container} = render(<ModelBadge runtime={null}/>);
    const chip = container.querySelector(".v7-model")!;
    expect(chip).toHaveAttribute("data-unknown", "true");
    expect(chip.querySelector("[aria-hidden]")).toHaveTextContent("模型未知");
  });
  it("follows the application language", () => {
    setLanguagePreference("en");
    const {container} = render(<ModelBadge runtime={{model: "gpt-6.1-sol", effort: null}}/>);
    expect(container.querySelector("[aria-hidden]")).toHaveTextContent(/^GPT-6\.1 Sol$/);
    expect(screen.getByText("Model GPT-6.1 Sol, Reasoning unknown")).toBeInTheDocument();
  });
});
