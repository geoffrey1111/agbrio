// Pinned Windows packaging boundary: the Node runtime the hosted connector runs on.
import { copyFile, mkdir, readFile, writeFile, stat } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import path from 'node:path';

const root = path.resolve(import.meta.dirname, '..');
const staging = path.join(root, 'src-tauri', 'resources', 'node-runtime');
const target = path.join(root, 'src-tauri', 'binaries', 'browser-executor-node-x86_64-pc-windows-msvc.exe');
async function hash(filename) {
  const digest = createHash('sha256');
  for await (const chunk of createReadStream(filename)) digest.update(chunk);
  return digest.digest('hex');
}
await mkdir(staging, { recursive: true });
await mkdir(path.dirname(target), { recursive: true });
// An unchanged resident Node executable may be locked by Windows. Keep it in
// place after exact hash verification; a build never stops running processes.
const reused = await stat(target).catch(() => null) && await hash(target) === await hash(process.execPath);
if (!reused) await copyFile(process.execPath, target);
await writeFile(path.join(staging, 'NODE-NOTICES.txt'), `${process.release.sourceUrl}\nNode.js is distributed under its license at https://github.com/nodejs/node/blob/v${process.versions.node}/LICENSE\n`);
let license = await readFile(path.join(path.dirname(process.execPath), 'LICENSE')).catch(() => null);
if (!license) {
  const response = await fetch(`https://raw.githubusercontent.com/nodejs/node/v${process.versions.node}/LICENSE`);
  if (!response.ok) throw new Error('NODE_NOTICE_REQUIRED');
  license = Buffer.from(await response.arrayBuffer());
}
await writeFile(path.join(staging, 'NODE-LICENSE.txt'), license);
console.log(JSON.stringify({ status: 'NODE_RUNTIME_STAGED', nodeVersion: process.versions.node, nodeExecutableReused: Boolean(reused) }));
