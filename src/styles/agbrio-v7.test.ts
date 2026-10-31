import {readFileSync} from "node:fs";
import {resolve} from "node:path";
import {describe, expect, it} from "vitest";

const css = readFileSync(resolve(process.cwd(), "src/styles/agbrio-v7.css"), "utf8");

describe("agbrio v7 token layer", () => {
  it("keeps the brand accent and warm neutrals", () => {
    for (const token of ["--v7-accent:#b4532a", "--v7-bg:#f7f6f4", "--v7-ink:#1e1b18", "--v7-muted:#766e66", "--v7-run:#2c7a57", "--v7-fail:#c0453a"]) expect(css).toContain(token);
  });
  it("re-maps the shared tokens older surfaces read", () => {
    for (const token of ["--awr-canvas", "--awr-ink", "--awr-muted", "--awr-accent", "--awr-line", "--awr-warning", "--r2-accent", "--r2-muted"]) expect(css).toMatch(new RegExp(`${token}:var\\(--v7-`));
  });
  it("never sets readable type below 12px (only explicitly marked glyphs are exempt)", () => {
    const text = css.split("\n").filter(line => !line.includes("/* glyph:")).join("\n");
    const sizes = [...text.matchAll(/font-size:\s*([\d.]+)px/g)].map(m => Number(m[1]));
    const tokens = [...css.matchAll(/--v7-fs-[\w-]+:\s*([\d.]+)px/g)].map(m => Number(m[1]));
    expect(sizes.length + tokens.length).toBeGreaterThan(0);
    for (const size of [...sizes, ...tokens]) expect(size).toBeGreaterThanOrEqual(12);
  });
  it("uses only the approved radii for tags, controls and cards", () => {
    expect(css).toContain("--v7-r-tag:6px");
    expect(css).toContain("--v7-r-ctl:10px");
    expect(css).toContain("--v7-r-card:14px");
  });
  it("styles the model badge including its unknown state", () => {
    expect(css).toMatch(/\.v7-model\{/);
    expect(css).toMatch(/\.v7-model\[data-unknown\]\{/);
    expect(css).toMatch(/\.v7-sr\{/);
  });
  it("lets the phone meta line yield source text to the model chip", () => {
    const phone = css.slice(css.indexOf("@media(max-width:760px)"));
    expect(phone).toContain(".v6-row-meta:has(>.v7-model)>.v7-meta-text{display:none}");
  });
});
