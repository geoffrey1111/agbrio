// Pinned Windows packaging boundary, not a provider operation or updater.
import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { cp, mkdir, readFile, writeFile, copyFile, stat } from 'node:fs/promises';
import path from 'node:path';

const root = path.resolve(import.meta.dirname, '..');
const source = path.join(root, 'tools', 'isolated-browser-executor');
const policy = JSON.parse(await readFile(path.join(source, 'runtime-policy.json'), 'utf8'));
const staging = path.join(root, 'src-tauri', 'resources', 'browser-executor-lite');
async function hash(filename) {
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(filename)) hash.update(chunk);
  return hash.digest('hex');
}
await mkdir(staging, {recursive:true});
// This staging tree contains helpers only; Chromium is never shipped by default.
for (const filename of ['executor.mjs','native-runtime.mjs','browser-package.mjs','security-browser.mjs','attachments.mjs','production-entry.mjs','runtime-policy.json','chromium-manifest.json']) await copyFile(path.join(source,filename),path.join(staging,filename));
const lock=JSON.parse(await readFile(path.join(source,'package-lock.json'),'utf8'));const seen=new Set();
async function copyDependency(name){if(seen.has(name))return;const row=lock.packages[`node_modules/${name}`];if(!row)throw Error(`DEPENDENCY_LOCK_REQUIRED:${name}`);seen.add(name);await cp(path.join(source,'node_modules',name),path.join(staging,'node_modules',name),{recursive:true});for(const dep of Object.keys(row.dependencies??{}))await copyDependency(dep);for(const dep of Object.keys(row.optionalDependencies??{}))if(lock.packages[`node_modules/${dep}`])await copyDependency(dep);}
await copyDependency('puppeteer-core');
// Development/native tests retain the historical helper path, without downloading
// a browser. The installer maps ONLY browser-executor-lite to browser-executor.
const development=path.join(root,'src-tauri','resources','browser-executor');await mkdir(development,{recursive:true});
for(const name of ['executor.mjs','native-runtime.mjs','browser-package.mjs','security-browser.mjs','attachments.mjs','production-entry.mjs','runtime-policy.json','chromium-manifest.json'])await copyFile(path.join(staging,name),path.join(development,name));
await cp(path.join(staging,'node_modules'),path.join(development,'node_modules'),{recursive:true});

const files=JSON.parse(await readFile(path.join(source,'chromium-manifest.json'),'utf8')).files;
await mkdir(path.join(root, 'src-tauri', 'binaries'), { recursive: true });
const nodeTarget = path.join(root, 'src-tauri', 'binaries', 'browser-executor-node-x86_64-pc-windows-msvc.exe');
// An unchanged resident Node executable may be locked by Windows. Keep it in
// place after exact hash verification; a build never stops/restarts providers.
const nodeTargetAlreadyMatches = await stat(nodeTarget).catch(() => null)
  && await hash(nodeTarget) === await hash(process.execPath);
if (!nodeTargetAlreadyMatches) await copyFile(process.execPath, nodeTarget);
await writeFile(path.join(staging, 'NODE-NOTICES.txt'), process.release.sourceUrl + '\n' +
  'Node.js is distributed under its license at https://github.com/nodejs/node/blob/v' + process.versions.node + '/LICENSE\n');
const nodeLicenseLocal = path.join(path.dirname(process.execPath), 'LICENSE');
let nodeLicense = await readFile(nodeLicenseLocal).catch(() => null);
if (!nodeLicense) {
  const response = await fetch(`https://raw.githubusercontent.com/nodejs/node/v${process.versions.node}/LICENSE`);
  if (!response.ok) throw new Error('NODE_NOTICE_REQUIRED');
  nodeLicense = Buffer.from(await response.arrayBuffer());
}
await writeFile(path.join(staging, 'NODE-LICENSE.txt'), nodeLicense);
// License texts change rarely; a transient network failure must not fail a
// release. Retry, then fall back to the last copy this machine fetched.
const licenseCache = path.join(root, 'runtime', 'license-cache');
async function licenseText(name, url, decode, error) {
  const cached = path.join(licenseCache, name);
  for (let attempt = 0; attempt < 4; attempt++) {
    try {
      const response = await fetch(url, { signal: AbortSignal.timeout(20000) });
      if (response.ok) { const text = decode(await response.text()); await mkdir(licenseCache, { recursive: true }); await writeFile(cached, text); return text; }
    } catch {}
    await new Promise(resolve => setTimeout(resolve, 2000 * (attempt + 1)));
  }
  const fallback = await readFile(cached).catch(() => null);
  if (!fallback || fallback.length < 1000) throw new Error(error);
  console.warn(`${name}: network unavailable, using cached copy`);
  return fallback;
}
await writeFile(path.join(staging, 'PUPPETEER-LICENSE.txt'), await licenseText('PUPPETEER-LICENSE.txt', 'https://www.apache.org/licenses/LICENSE-2.0.txt', text => text, 'PUPPETEER_NOTICE_REQUIRED'));
// Chromium itself retains chrome://credits for its bundled third-party notices.
await writeFile(path.join(staging, 'CHROMIUM-LICENSE.txt'), await licenseText('CHROMIUM-LICENSE.txt', 'https://chromium.googlesource.com/chromium/src/+/refs/heads/main/LICENSE?format=TEXT', text => Buffer.from(text, 'base64'), 'CHROMIUM_NOTICE_REQUIRED'));
console.log(JSON.stringify({ status: 'PACKAGE_STAGED', snapshot: policy.snapshot,
  executableSha256: files['chrome.exe'], runtimeFiles: Object.keys(files).length,
  dependencies: [...seen].sort(), nodeVersion: process.versions.node, nodeExecutableReused: !!nodeTargetAlreadyMatches }));
