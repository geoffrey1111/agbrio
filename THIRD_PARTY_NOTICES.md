# Third-party software

Agbrio's own code is MIT licensed. Dependencies retain their own licenses.
The locked build inventory and copied copyright/license texts are in
`third-party/inventory.json` and the corresponding `third-party/` folders.
The inventory intentionally includes development dependencies as well as runtime
dependencies, and is generated from the installed lockfiles, not a guessed list.

Unmodified MPL-2.0 Rust components include cssparser, cssparser-macros, dtoa-short,
option-ext and selectors. Their covered source remains under MPL-2.0. Exact
versioned source archives are linked in the inventory; recipients may obtain
those sources without charge and exercise the rights of that license. Agbrio
does not replace their license with MIT.

Windows packages include Node.js, Puppeteer and the pinned non-branded Chromium
runtime. Full Node notices and Chromium/Puppeteer license texts are installed
under `browser-executor/`. Chromium's bundled component notices remain available
at `chrome://credits`. `tools/isolated-browser-executor/runtime-policy.json`
identifies the exact public Chromium archive and verified hashes.

Noto Sans SC is under the SIL Open Font License; its notice is retained at
`public/fonts/OFL-NotoSansSC.txt` and in the web build. Generated Agbrio marks are
project assets. Codex Desktop is not redistributed. Microsoft WebView2 is an
external prerequisite installed through Microsoft's official bootstrapper and
governed by Microsoft's terms.

For source rebuilds, run `python scripts/license-inventory.py` after installing
locked dependencies. Keep these notices with redistributed builds. See the
[Mozilla MPL FAQ](https://www.mozilla.org/en-US/MPL/2.0/FAQ/#q8-i-want-to-distribute-outside-my-organization-executable-programs-or-libraries-that-i-have-compiled-from-someone-elses-unchanged-mpl-licensed-source-code-either-standalone-or-part-of-a-larger-work-what-do-i-have-to-do)
for its source-availability requirement.

GSAP 3.15.0 provides optional motion feedback. It is under the GSAP Standard
no-charge license, **not MIT**: https://gsap.com/standard-license/ . Its upstream
package attribution and license URL are retained in the `third-party/` inventory; its
copyright/license header stays in the distributed JavaScript chunk.
