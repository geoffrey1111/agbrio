// Pinned Windows packaging boundary, not a provider operation or updater.
import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { cp, mkdir, readFile, readdir, writeFile, copyFile, stat, mkdtemp, rm, lstat } from 'node:fs/promises';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

const root = path.resolve(import.meta.dirname, '..');
const source = path.join(root, 'tools', 'isolated-browser-executor');
const policy = JSON.parse(await readFile(path.join(source, 'runtime-policy.json'), 'utf8'));
const staging = path.join(root, 'src-tauri', 'resources', 'browser-executor');
const snapshot = path.join(root, 'runtime', 'isolated-browser-executor', 'browser-runtime', `chromium-snapshot-${policy.snapshot}`);
const archive = path.join(snapshot, 'chrome-win.zip');
async function hash(filename) {
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(filename)) hash.update(chunk);
  return hash.digest('hex');
}
await mkdir(snapshot, { recursive: true });
if (!await stat(archive).catch(() => null)) {
  const response = await fetch(policy.source);
  if (!response.ok) throw new Error('CHROMIUM_DOWNLOAD_FAILED');
  // Download only this exact public archive, not a browser-selected build.
  await writeFile(archive, Buffer.from(await response.arrayBuffer()), { flag: 'wx' });
}
if (await hash(archive) !== policy.archiveSha256) throw new Error('CHROMIUM_ARCHIVE_HASH_MISMATCH');
// Always extract the verified archive into a fresh generated build directory.
// A previously launched/mutated runtime tree is never packaging authority.
const packageSource = await mkdtemp(path.join(snapshot, 'package-source-'));
{
  const expansion = spawnSync('pwsh.exe', ['-NoProfile', '-NonInteractive', '-Command',
    'Expand-Archive -LiteralPath $env:AIWR_PACKAGE_ARCHIVE -DestinationPath $env:AIWR_PACKAGE_SNAPSHOT'],
  { env: { ...process.env, AIWR_PACKAGE_ARCHIVE: archive, AIWR_PACKAGE_SNAPSHOT: packageSource }, windowsHide: true, stdio: 'inherit' });
  if (expansion.status !== 0) throw new Error('CHROMIUM_ARCHIVE_EXPANSION_FAILED');
}
if (await hash(path.join(packageSource, 'chrome-win', 'chrome.exe')) !== policy.executableSha256) throw new Error('CHROMIUM_BINARY_HASH_MISMATCH');
if (await stat(staging).catch(() => null)) {
  const previous = JSON.parse(await readFile(path.join(staging, 'chromium-manifest.json'), 'utf8'));
  if (previous.schema !== policy.schema || previous.archiveSha256 !== policy.archiveSha256
    || (await lstat(staging)).isSymbolicLink()
    || path.resolve(staging) !== path.join(root, 'src-tauri', 'resources', 'browser-executor')) {
    throw new Error('GENERATED_PACKAGE_OWNERSHIP_REQUIRED');
  }
  await rm(staging, { recursive: true });
}
await mkdir(staging, { recursive: true });
const chromium = path.join(staging, 'chromium');
// The snapshot's interactive_ui_tests is a 364 MB test runner, not a product
// dependency. Keep the browser's other native resources unchanged.
await cp(path.join(packageSource, 'chrome-win'), chromium, {
  recursive: true, filter: filename => path.basename(filename) !== 'interactive_ui_tests.exe',
});
if (!path.resolve(packageSource).startsWith(path.resolve(snapshot) + path.sep)
  || !path.basename(packageSource).startsWith('package-source-')) throw new Error('PACKAGE_SCRATCH_SCOPE_INVALID');
await rm(packageSource, { recursive: true });
for (const filename of ['executor.mjs', 'native-runtime.mjs', 'security-browser.mjs', 'attachments.mjs', 'production-entry.mjs', 'runtime-policy.json']) {
  await copyFile(path.join(source, filename), path.join(staging, filename));
}
// Copy only Puppeteer's actual lockfile dependency closure. No Playwright,
// extension, Native Messaging, MCP or legacy sidecar is a consumer here.
const lock = JSON.parse(await readFile(path.join(source, 'package-lock.json'), 'utf8'));
const seen = new Set();
async function copyDependency(name) {
  if (seen.has(name)) return;
  const row = lock.packages[`node_modules/${name}`];
  if (!row) throw new Error(`DEPENDENCY_LOCK_REQUIRED:${name}`);
  seen.add(name);
  await cp(path.join(source, 'node_modules', name), path.join(staging, 'node_modules', name), { recursive: true });
  for (const dependency of Object.keys(row.dependencies ?? {})) await copyDependency(dependency);
  for (const dependency of Object.keys(row.optionalDependencies ?? {})) {
    if (lock.packages[`node_modules/${dependency}`]) await copyDependency(dependency);
  }
}
await copyDependency('puppeteer-core');
const files = {};
async function inventory(directory, prefix = '') {
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isSymbolicLink()) throw new Error('CHROMIUM_PACKAGE_LINK_REJECTED');
    const relative = prefix + entry.name;
    if (entry.isDirectory()) await inventory(path.join(directory, entry.name), relative + '/');
    else files[relative] = await hash(path.join(directory, entry.name));
  }
}
await inventory(chromium);
await writeFile(path.join(staging, 'chromium-manifest.json'), JSON.stringify({ ...policy, files }, null, 2));
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
const puppeteerLicense = await fetch('https://www.apache.org/licenses/LICENSE-2.0.txt');
if (!puppeteerLicense.ok) throw new Error('PUPPETEER_NOTICE_REQUIRED');
await writeFile(path.join(staging, 'PUPPETEER-LICENSE.txt'), await puppeteerLicense.text());
// Chromium itself retains chrome://credits for its bundled third-party notices.
const license = await fetch('https://chromium.googlesource.com/chromium/src/+/refs/heads/main/LICENSE?format=TEXT');
if (!license.ok) throw new Error('CHROMIUM_NOTICE_REQUIRED');
await writeFile(path.join(staging, 'CHROMIUM-LICENSE.txt'), Buffer.from(await license.text(), 'base64'));
console.log(JSON.stringify({ status: 'PACKAGE_STAGED', snapshot: policy.snapshot,
  executableSha256: files['chrome.exe'], runtimeFiles: Object.keys(files).length,
  dependencies: [...seen].sort(), nodeVersion: process.versions.node, nodeExecutableReused: !!nodeTargetAlreadyMatches }));
