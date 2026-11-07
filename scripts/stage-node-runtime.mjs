// Native-platform packaging boundary: the Node runtime the hosted connector runs on.
import { cp, copyFile, mkdir, readFile, writeFile, stat } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import path from 'node:path';
import {execFileSync} from 'node:child_process';
import {nodeSidecar} from './browser-executor-platform.mjs';

const root = path.resolve(import.meta.dirname, '..');
const staging = path.join(root, 'src-tauri', 'resources', 'node-runtime');
const target = path.join(root, 'src-tauri', 'binaries', nodeSidecar(process.platform,process.arch,process.env.TAURI_ENV_TARGET_TRIPLE));
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

if(process.platform==='win32'){await mkdir(path.join(root,'src-tauri/resources/node-runtime/node_modules'),{recursive:true});await cp(path.join(root,'tools/node-runtime/node_modules/ws'),path.join(root,'src-tauri/resources/node-runtime/node_modules/ws'),{recursive:true});}

if(process.platform==='darwin'){
 const samplerDirectory=path.join(root,'src-tauri/resources/macos-platform');await mkdir(samplerDirectory,{recursive:true});
 const sampler=path.join(samplerDirectory,'mac-resource-sampler'),samplerSource=path.join(root,'tools/macos-platform');
 execFileSync('/usr/bin/xcrun',['swiftc','-parse-as-library','-swift-version','5','-O','-target',`${process.arch==='arm64'?'arm64':'x86_64'}-apple-macosx13.5`,'-framework','Foundation','-framework','CryptoKit','-import-objc-header',path.join(samplerSource,'ResourceSampler.h'),path.join(samplerSource,'ResourceSampler.swift'),'-o',sampler],{stdio:'inherit',timeout:120000});
 execFileSync('/usr/bin/codesign',['--force','--sign','-','--identifier','com.geoffrey.aiworkrouter.resource-sampler',sampler],{stdio:'inherit',timeout:30000});
}
