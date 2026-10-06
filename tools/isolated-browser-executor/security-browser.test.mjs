import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile, readFile, rm } from 'node:fs/promises';
import path from 'node:path';
import { ChromiumRuntime } from './native-runtime.mjs';
import { securityPlatform } from './security-browser.mjs';

const fixtureRoot = path.resolve(import.meta.dirname, '../../runtime/isolated-browser-executor');
async function fixture(t) {
  await mkdir(fixtureRoot, { recursive: true });
  const data = await mkdtemp(path.join(fixtureRoot, 'security-flow-'));
  t.after(() => rm(data, { recursive: true }));
  let owners = [], launches = 0, focuses = 0, focusFails = false, closeFails = false, locked = false;
  const binary = path.join(data, 'chromium', 'chrome.exe');
  const platform = {
    withExclusiveProfile: async (_profile, action) => {
      if (locked) throw new Error('SECURITY_RECOVERY_BUSY');
      locked = true;
      try { return await action(() => assert.equal(locked, true)); }
      finally { locked = false; }
    },
    inventory: async () => owners,
    focus: async record => {
      assert.equal(record, owners[0]);
      focuses++;
      if (focusFails) return 'ATTENTION';
      return 'FOREGROUND';
    },
    launch: async (actual, args) => {
      launches++;
      assert.equal(actual, binary);
      assert.deepEqual(args, [`--user-data-dir=${path.join(data, 'browser-profile', 'chatgpt')}`,
        '--disable-extensions', '--no-first-run', '--no-default-browser-check', '--restore-last-session']);
      assert.ok(!args.some(arg => /debugging|automation|headless|https:/.test(arg)));
      owners = [{ pid: 123, binary, started: 'unique-start', automated: false, visible: true }];
      return 123;
    },
    close: async () => { if (closeFails) throw new Error('SECURITY_BROWSER_CLOSE_PENDING'); owners = []; },
  };
  function runtime() {
    const result = new ChromiumRuntime({ resourceDirectory: data, dataDirectory: data, platform });
    result.verifiedBinary = async () => { await result.resolveProfile(); await mkdir(result.profile, { recursive: true }); return binary; };
    return result;
  }
  const first = runtime();
  await first.latchAuthRequired();
  return { data, first, runtime, get launches() { return launches; }, get focuses() { return focuses; }, set focusFails(value) { focusFails = value; }, set owners(value) { owners = value; }, set closeFails(value) { closeFails = value; } };
}

test('human open keeps stop, has no automation, reuses one owner across helper restart, explicit completion releases profile', async t => {
  const f = await fixture(t);
  const stop = await readFile(f.first.securityMarker);
  assert.equal((await f.first.openForAuthentication()).mode, 'HUMAN_ONLY');
  assert.equal(f.first.browser, null);
  assert.equal(f.first.started, false);
  assert.deepEqual(await readFile(f.first.securityMarker), stop);
  await assert.rejects(f.first.connection(), /AUTH_REQUIRED/);
  await f.first.shutdown();
  const restarted = f.runtime();
  assert.equal((await restarted.openForAuthentication()).reused, true);
  assert.equal(f.launches, 1);
  assert.equal(f.focuses, 2);
  await assert.rejects(restarted.connection(), /AUTH_REQUIRED/);
  assert.equal(await restarted.authRequired(), true);
  await restarted.ownerConfirmedAuthentication();
  assert.equal(await restarted.authRequired(), false);
  assert.equal(await restarted.readLease(), null);
  assert.equal(restarted.browser, null);
});

test('blocked native focus reports attention without launching another browser or clearing the stop', async t => {
  const f = await fixture(t);
  await f.first.openForAuthentication();
  const stop = await readFile(f.first.securityMarker);
  f.focusFails = true;
  const result = await f.runtime().openForAuthentication();
  assert.equal(result.focus, 'ATTENTION');
  assert.equal(result.reused, true);
  assert.equal(f.launches, 1);
  assert.deepEqual(await readFile(f.first.securityMarker), stop);
  f.focusFails = false;
  assert.equal((await f.runtime().openForAuthentication()).reused, true);
  await f.first.ownerConfirmedAuthentication();
});

test('concurrent human opens acquire one exclusive profile owner', async t => {
  const f = await fixture(t);
  const results = await Promise.allSettled([f.first.openForAuthentication(), f.runtime().openForAuthentication()]);
  assert.equal(results.filter(item => item.status === 'fulfilled').length, 1);
  assert.equal(f.launches, 1);
  assert.equal(await f.first.authRequired(), true);
  await f.first.ownerConfirmedAuthentication();
});

test('close failure and PID reuse keep auth latched; no competing browser', async t => {
  const f = await fixture(t);
  await f.first.openForAuthentication();
  f.closeFails = true;
  await assert.rejects(f.first.ownerConfirmedAuthentication(), /CLOSE_PENDING/);
  assert.equal(await f.first.authRequired(), true);
  assert.equal((await f.first.openForAuthentication()).reused, true);
  f.owners = [{ pid: 123, binary: path.join(f.data, 'chromium', 'chrome.exe'), started: 'reused-pid', automated: false }];
  await assert.rejects(f.first.ownerConfirmedAuthentication(), /PROFILE_IN_USE/);
  await assert.rejects(f.runtime().openForAuthentication(), /PROFILE_IN_USE/);
  assert.equal(f.launches, 1);
  assert.equal(await f.first.authRequired(), true);
});

test('unowned or automated existing browser is never attached, closed or duplicated', async t => {
  const f = await fixture(t);
  f.owners = [{ pid: 456, binary: 'unowned', started: 'different', automated: true }];
  await assert.rejects(f.first.openForAuthentication(), /PROFILE_IN_USE/);
  await assert.rejects(f.first.ownerConfirmedAuthentication(), /SECURITY_RECOVERY_NOT_OPENED/);
  assert.equal(f.launches, 0);
  assert.equal(await f.first.authRequired(), true);
});

test('authentication cannot implicitly release a pending human lease after marker drift', async t => {
  const f = await fixture(t);
  await f.first.openForAuthentication();
  await rm(f.first.securityMarker);
  await assert.rejects(f.runtime().connection(), /PROFILE_IN_USE/);
  await writeFile(f.first.securityMarker, '{"status":"AUTH_REQUIRED"}');
  assert.equal(f.launches, 1);
});

test('explicit completion before human launch is proven cannot clear latch or release a starting lease', async t => {
  const f = await fixture(t);
  await assert.rejects(f.first.ownerConfirmedAuthentication(), /SECURITY_RECOVERY_NOT_OPENED/);
  await f.first.verifiedBinary();
  await f.first.acquireLease('HUMAN_ONLY');
  await assert.rejects(f.runtime().ownerConfirmedAuthentication(), /SECURITY_RECOVERY_NOT_OPENED/);
  assert.equal(await f.first.authRequired(), true);
  assert.equal((await f.first.readLease()).mode, 'HUMAN_ONLY');
});

test('closed human window reopens with the same profile and unchanged stop; repeats reuse it', async t => {
  const f = await fixture(t);
  const stop = await readFile(f.first.securityMarker);
  await f.first.openForAuthentication();
  f.owners = [];
  const restarted = f.runtime();
  assert.equal((await restarted.openForAuthentication()).reused, false);
  assert.equal(f.launches, 2);
  assert.deepEqual(await readFile(restarted.securityMarker), stop);
  await assert.rejects(restarted.connection(), /AUTH_REQUIRED/);
  assert.equal((await f.first.openForAuthentication()).reused, true);
  assert.equal(f.launches, 2);
  await restarted.ownerConfirmedAuthentication();
});

test('two helpers reopening a closed human session create exactly one browser', async t => {
  const f = await fixture(t);
  await f.first.openForAuthentication();
  f.owners = [];
  const results = await Promise.allSettled([f.runtime().openForAuthentication(), f.runtime().openForAuthentication()]);
  assert.equal(results.filter(result => result.status === 'fulfilled').length, 1);
  assert.equal(f.launches, 2);
  assert.equal(await f.first.authRequired(), true);
  await f.first.ownerConfirmedAuthentication();
});

test('unfinished and mismatched stale records cannot be reclaimed', async t => {
  const f = await fixture(t);
  await f.first.openForAuthentication();
  f.owners = [];
  const record = await f.first.readLease();
  await writeFile(f.first.leasePath(), JSON.stringify({ ...record, binary: path.join(f.data, 'different.exe') }));
  await assert.rejects(f.runtime().openForAuthentication(), /PROFILE_IN_USE/);
  await writeFile(f.first.leasePath(), JSON.stringify({ schema: record.schema, profile: record.profile, mode: 'HUMAN_ONLY' }));
  await assert.rejects(f.runtime().openForAuthentication(), /PROFILE_IN_USE/);
  assert.equal(f.launches, 1);
  assert.equal(await f.first.authRequired(), true);
});

test('native Windows profile mutex excludes concurrent actions and releases after failure', { skip: process.platform !== 'win32' }, async t => {
  const f = await fixture(t);
  let entered;
  const ready = new Promise(resolve => { entered = resolve; });
  let release;
  const held = securityPlatform.withExclusiveProfile(f.first.profile, async assertHeld => {
    assertHeld(); entered();
    await new Promise(resolve => { release = resolve; });
    assertHeld();
  });
  await ready;
  try {
    await assert.rejects(securityPlatform.withExclusiveProfile(f.first.profile, async () => assert.fail('second action entered')), /SECURITY_RECOVERY_BUSY/);
  } finally { release(); await held; }
  await assert.rejects(securityPlatform.withExclusiveProfile(f.first.profile, async () => { throw new Error('FIXTURE_FAILURE'); }), /FIXTURE_FAILURE/);
  assert.equal(await securityPlatform.withExclusiveProfile(f.first.profile, async assertHeld => { assertHeld(); return 'released'; }), 'released');
});
