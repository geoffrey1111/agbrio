import {readFileSync} from "node:fs";
import {resolve} from "node:path";
import {expect, it} from "vitest";

// Touch browsers can keep a synthetic :hover after a finger passes over a row,
// which reads as a stuck selection. List-row and child-tree hover styles must
// only apply where the primary pointer can genuinely hover.
const files = ["agbrio-v7.css", "agbrio-v7-r2.css", "agbrio-v6-theme.css", "conversation-v6.css"];
const target = /\.(v6-row|v6-pi-child-toggle|v7-tree-[\w-]+)[^,{]*:hover/;

function hoverRules(css: string) {
  const out: {selector: string; media: string[]}[] = [];
  const stack: string[] = [];
  let head = "";
  for (const ch of css.replace(/\/\*[\s\S]*?\*\//g, "")) {
    if (ch === "{") { stack.push(head.trim()); head = ""; }
    else if (ch === "}") { stack.pop(); head = ""; }
    else if (ch === ";" ) head = "";
    else head += ch;
    if (ch === "{") {
      const selector = stack[stack.length - 1];
      if (!selector.startsWith("@") && target.test(selector)) out.push({selector, media: stack.slice(0, -1).filter(s => s.startsWith("@media"))});
    }
  }
  return out;
}

it("limits list-row and child-tree hover to fine, hover-capable pointers", () => {
  const rules = files.flatMap(file => hoverRules(readFileSync(resolve(process.cwd(), "src/styles", file), "utf8")).map(rule => ({file, ...rule})));
  expect(rules.length).toBeGreaterThan(5);
  for (const rule of rules) expect(rule.media.some(m => /hover:\s*hover/.test(m) && /pointer:\s*fine/.test(m)), `${rule.file}: ${rule.selector}`).toBe(true);
});
