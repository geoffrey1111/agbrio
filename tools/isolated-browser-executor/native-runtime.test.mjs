import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, rm, mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { ChromiumRuntime } from './native-runtime.mjs';

test('a durable auth stop survives Executor recreation and opens no browser', async t => {
  const root = fileURLToPath(new URL('../../runtime/isolated-browser-executor/', import.meta.url)).replace(/[\\/]$/, '');
  const directory = await mkdtemp(path.join(root, 'auth-fixture-'));
  t.after(() => { assert.equal(path.dirname(directory), root); return rm(directory, { recursive: true }); });
  const first = new ChromiumRuntime({ resourceDirectory: directory, dataDirectory: directory });
  await first.latchAuthRequired();
  const restarted = new ChromiumRuntime({ resourceDirectory: directory, dataDirectory: directory, platform: { inventory: async () => [] } });
  await assert.rejects(restarted.openForAuthentication(), /ENOENT/);
  assert.equal(restarted.started, false);
  await assert.rejects(restarted.connection(), /AUTH_REQUIRED/);
  await assert.rejects(restarted.ownerConfirmedAuthentication(), /SECURITY_RECOVERY_NOT_OPENED/);
  assert.equal(await restarted.authRequired(), true);
  assert.equal(restarted.started, false);
});


test('fixed upgrade adoption needs a matching owned-profile marker; never copies state', async t => {
  const root = fileURLToPath(new URL('../../runtime/isolated-browser-executor/', import.meta.url));
  const directory = await mkdtemp(path.join(root, 'profile-fixture-'));
  t.after(() => rm(directory, { recursive: true }));
  const profile = path.join(directory, 'previous-owned-profile');
  await mkdir(profile);
  const record = { schema: 'AIWR_OWNED_CHROMIUM_PROFILE_V1', ownerId: '11111111-1111-1111-1111-111111111111', profile };
  await writeFile(path.join(directory, 'owned-profile-location.json'), JSON.stringify(record));
  await writeFile(path.join(profile, 'aiwr-profile-owner.json'), JSON.stringify({ ...record, ownerId: '22222222-2222-2222-2222-222222222222' }));
  const rejected = new ChromiumRuntime({ resourceDirectory: directory, dataDirectory: directory });
  await assert.rejects(rejected.resolveProfile(), /PROFILE_OWNERSHIP_UNPROVEN/);
  assert.equal(rejected.started, false);
  await writeFile(path.join(profile, 'aiwr-profile-owner.json'), JSON.stringify(record));
  const adopted = new ChromiumRuntime({ resourceDirectory: directory, dataDirectory: directory });
  assert.equal(await adopted.resolveProfile(), profile);
  assert.equal(adopted.started, false);
});

test('explicit ordinary Open focuses only the existing owned process without navigating or closing', async () => {
  const resources = path.resolve('runtime/isolated-browser-executor/focus-fixture');
  const owner = { pid: 1234, binary: path.join(resources, 'chromium', 'chrome.exe'), started: 'exact-birth', automated: true };
  const focused = [];
  const runtime = new ChromiumRuntime({ resourceDirectory: resources, dataDirectory: resources,
    platform: { inventory: async () => [owner], focus: async record => { focused.push(record); return 'ATTENTION'; } } });
  runtime.authRequired = async () => false;
  runtime.browser = { connected: true }; // Existing browser, no component provisioning.
  runtime.connection = async () => ({ process: () => ({ pid: 1234 }), pages: async () => [{ url: () => 'https://chatgpt.com/c/exact' }] });
  assert.equal((await runtime.openForAuthentication()).focus, 'ATTENTION');
  assert.deepEqual(focused, [owner]);
  owner.pid = 5678;
  await assert.rejects(runtime.openForAuthentication(), /SECURITY_BROWSER_IDENTITY_CHANGED/);
  assert.equal(focused.length, 1);
});
