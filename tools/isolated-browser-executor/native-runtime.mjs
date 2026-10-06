// Windows distribution/path boundary only. DOM/CDP operations stay in Executor.
import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { lstat, mkdir, readFile, writeFile, unlink, open } from 'node:fs/promises';
import path from 'node:path';
import puppeteer from 'puppeteer-core';
import { securityPlatform } from './security-browser.mjs';

export const CHROMIUM_DEBUGGING_PORT = 9229;
const BINARY_SHA = 'ac2b50c11c536aff132304f1c9ec33709e43ae7f34357153f7359e52fe35f12c';
export async function fileSha256(filename) {
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(filename)) hash.update(chunk);
  return hash.digest('hex');
}
export async function rejectLinkedAncestors(filename) {
  for (let current = path.resolve(filename); ; current = path.dirname(current)) {
    try {
      if ((await lstat(current)).isSymbolicLink()) throw new Error('ROUTER_PATH_LINK_REJECTED');
    } catch (error) { if (error.code !== 'ENOENT') throw error; }
    if (path.dirname(current) === current) break;
  }
}

export class ChromiumRuntime {
  constructor({ resourceDirectory, dataDirectory, platform = securityPlatform }) {
    if (!path.isAbsolute(resourceDirectory) || !path.isAbsolute(dataDirectory)) throw new Error('ROUTER_ABSOLUTE_PATHS_REQUIRED');
    this.resources = resourceDirectory;
    this.data = dataDirectory;
    this.profile = path.join(dataDirectory, 'browser-profile', 'chatgpt');
    this.securityMarker = path.join(dataDirectory, 'browser-auth-required.json');
    this.browser = null;
    this.started = false;
    this.platform = platform;
    this.lease = null;
  }
  async authRequired() {
    try { await readFile(this.securityMarker); return true; }
    catch (error) { if (error.code !== 'ENOENT') throw error; return false; }
  }
  async latchAuthRequired() {
    await mkdir(this.data, { recursive: true });
    await writeFile(this.securityMarker, JSON.stringify({ status: 'AUTH_REQUIRED', ownerActionRequired: true }), { mode: 0o600 });
  }
  async ownerConfirmedAuthentication() {
    // Only the explicit owner action may close the human window and release
    // the profile. Failure keeps AUTH_REQUIRED and ownership intact.
    await this.resolveProfile();
    const record = await this.readLease();
    if (!record || record.mode !== 'HUMAN_ONLY' || !Number.isSafeInteger(record.pid) || !record.started) throw new Error('SECURITY_RECOVERY_NOT_OPENED');
    return this.platform.withExclusiveProfile(this.profile, async assertHeld => {
      assertHeld();
      return this.completeHumanSecurity(assertHeld);
    });
  }
  async completeHumanSecurity(assertHeld) {
    const record = await this.readLease();
    if (!record || record.mode !== 'HUMAN_ONLY' || !Number.isSafeInteger(record.pid) || !record.started) throw new Error('SECURITY_RECOVERY_NOT_OPENED');
    const owners = await this.platform.inventory(this.profile);
    if (owners.length) {
      if (record?.mode !== 'HUMAN_ONLY' || owners.length !== 1 || !this.matchesOwner(record, owners[0])) throw new Error('PROFILE_IN_USE');
      await this.platform.close(owners[0]);
      const deadline = Date.now() + 10000;
      while ((await this.platform.inventory(this.profile)).length) {
        if (Date.now() >= deadline) throw new Error('SECURITY_BROWSER_CLOSE_PENDING');
        await new Promise(resolve => setTimeout(resolve, 200));
      }
    }
    assertHeld();
    await this.releaseLease();
    await unlink(this.securityMarker).catch(error => { if (error.code !== 'ENOENT') throw error; });
    this.started = false;
    return { status: 'OWNER_CONFIRMATION_RECORDED' };
  }
  async resolveProfile() {
    // Upgrade adoption retains one already-owned profile in place. No browser
    // state export/copy, daily-profile discovery, per-request switching or retry.
    if (this.started) return this.profile;
    const location = path.join(this.data, 'owned-profile-location.json');
    await rejectLinkedAncestors(location);
    let record;
    try { record = JSON.parse(await readFile(location, 'utf8')); }
    catch (error) { if (error.code === 'ENOENT') return this.profile; throw error; }
    if (record.schema !== 'AIWR_OWNED_CHROMIUM_PROFILE_V1' || !path.isAbsolute(record.profile ?? '')
      || !/^[a-f0-9-]{36}$/i.test(record.ownerId ?? '')) throw new Error('PROFILE_OWNERSHIP_UNPROVEN');
    await rejectLinkedAncestors(record.profile);
    const owner = JSON.parse(await readFile(path.join(record.profile, 'aiwr-profile-owner.json'), 'utf8'));
    if (owner.schema !== record.schema || owner.ownerId !== record.ownerId) throw new Error('PROFILE_OWNERSHIP_UNPROVEN');
    this.profile = path.resolve(record.profile);
    return this.profile;
  }
  async connection() {
    if (await this.authRequired()) throw Object.assign(new Error('AUTH_REQUIRED'), { code: 'AUTH_REQUIRED' });
    if (this.browser) {
      if (!this.browser.connected) throw Object.assign(new Error('BROWSER_UNAVAILABLE'), { code: 'BROWSER_UNAVAILABLE' });
      return this.browser;
    }
    if (this.started) throw Object.assign(new Error('BROWSER_REOPEN_REQUIRED'), { code: 'BROWSER_REOPEN_REQUIRED' });
    const binary = await this.verifiedBinary();
    await this.acquireLease('AUTOMATED');
    if (await this.authRequired()) { await this.releaseLease(); throw new Error('AUTH_REQUIRED'); }
    this.started = true;
    // Restore the exact native launch contract validated by the adopted Spike.
    // No UA/navigator override, anti-detection flags or sandbox bypass.
    try {
      this.browser = await puppeteer.launch({
        executablePath: binary, headless: false, defaultViewport: null,
        ignoreDefaultArgs: true, protocolTimeout: 15000, timeout: 30000,
        args: [`--user-data-dir=${this.profile}`, '--remote-debugging-address=127.0.0.1',
          `--remote-debugging-port=${CHROMIUM_DEBUGGING_PORT}`, '--disable-extensions', '--no-first-run',
          '--no-default-browser-check', 'about:blank'],
      });
    } catch (error) {
      if (!(await this.platform.inventory(this.profile)).length) await this.releaseLease();
      throw error;
    }
    return this.browser;
  }
  async verifiedBinary() {
    await this.resolveProfile();
    await rejectLinkedAncestors(this.resources);
    await rejectLinkedAncestors(this.profile);
    const binary = path.join(this.resources, 'chromium', 'chrome.exe');
    if (await fileSha256(binary) !== BINARY_SHA) throw Object.assign(new Error('CHROMIUM_HASH_MISMATCH'), { code: 'CHROMIUM_HASH_MISMATCH' });
    const manifest = JSON.parse(await readFile(path.join(this.resources, 'chromium-manifest.json'), 'utf8'));
    // The package's verified archive supplies this manifest. Verify every
    // shipped runtime file, including chrome.dll, before native execution.
    for (const [relative, expected] of Object.entries(manifest.files)) {
      const filename = path.resolve(this.resources, 'chromium', relative);
      if (!filename.startsWith(path.resolve(this.resources, 'chromium') + path.sep)) throw new Error('CHROMIUM_MANIFEST_PATH_INVALID');
      await rejectLinkedAncestors(filename);
      if (await fileSha256(filename) !== expected) throw new Error('CHROMIUM_PACKAGE_HASH_MISMATCH');
    }
    await mkdir(this.profile, { recursive: true });
    return binary;
  }
  leasePath() { return path.join(this.profile, 'aiwr-browser-session.json'); }
  async readLease() {
    await rejectLinkedAncestors(this.leasePath());
    try { return JSON.parse(await readFile(this.leasePath(), 'utf8')); }
    catch (error) { if (error.code === 'ENOENT') return null; throw error; }
  }
  async acquireLease(mode) {
    if ((await this.platform.inventory(this.profile)).length) throw new Error('PROFILE_IN_USE');
    await rejectLinkedAncestors(this.leasePath());
    let file;
    try { file = await open(this.leasePath(), 'wx', 0o600); }
    catch (error) { if (error.code === 'EEXIST') throw new Error('PROFILE_IN_USE'); throw error; }
    this.lease = { schema: 'AIWR_BROWSER_SESSION_V1', mode, executorPid: process.pid, profile: this.profile };
    try { await file.writeFile(JSON.stringify(this.lease)); } finally { await file.close(); }
  }
  async releaseLease() {
    await unlink(this.leasePath()).catch(error => { if (error.code !== 'ENOENT') throw error; });
    this.lease = null;
  }
  matchesOwner(record, owner) {
    return record.schema === 'AIWR_BROWSER_SESSION_V1' && record.profile === this.profile
      && record.pid === owner.pid && record.started === owner.started
      && path.resolve(record.binary).toLowerCase() === path.resolve(owner.binary).toLowerCase() && !owner.automated;
  }
  async openHumanSecurity() {
    if (!await this.authRequired()) throw new Error('SECURITY_RECOVERY_NOT_REQUIRED');
    const binary = await this.verifiedBinary();
    return this.platform.withExclusiveProfile(this.profile, async assertHeld => {
      assertHeld();
      return this.openHumanSecurityExclusively(binary, assertHeld);
    });
  }
  async openHumanSecurityExclusively(binary, assertHeld) {
    if (!await this.authRequired()) throw new Error('SECURITY_RECOVERY_NOT_REQUIRED');
    let existing = await this.readLease();
    // Owner-requested transition may end this Executor's existing automated
    // browser. Never attach to or close a browser owned by another process.
    if (this.browser && existing?.mode === 'AUTOMATED' && existing.executorPid === process.pid) {
      await this.browser.close();
      this.browser = null;
      if ((await this.platform.inventory(this.profile)).length) throw new Error('PROFILE_IN_USE');
      await this.releaseLease();
      this.started = false;
      existing = null;
    }
    if (existing) {
      const owners = await this.platform.inventory(this.profile);
      if (existing.mode === 'HUMAN_ONLY' && owners.length === 1 && this.matchesOwner(existing, owners[0])) {
        assertHeld();
        const focus = await this.platform.focus(owners[0]);
        return { status: 'OPEN', mode: 'HUMAN_ONLY', reused: true, focus };
      }
      // The owner may have closed the human window without confirming account
      // clearance. Reopen only a complete, proven SAME-binary/profile record
      // with zero actual profile owners. The kernel guard prevents two helpers
      // from reclaiming that closed session concurrently. Keep AUTH_REQUIRED.
      if (owners.length || existing.schema !== 'AIWR_BROWSER_SESSION_V1'
        || existing.mode !== 'HUMAN_ONLY' || existing.profile !== this.profile
        || !Number.isSafeInteger(existing.pid) || !existing.started
        || path.resolve(existing.binary ?? '').toLowerCase() !== path.resolve(binary).toLowerCase()) throw new Error('PROFILE_IN_USE');
      assertHeld();
      await this.releaseLease();
    }
    assertHeld();
    await this.acquireLease('HUMAN_ONLY');
    // This is an owner-requested native launch. Chromium restores its own last
    // session; Router issues no URLs, navigation, CDP connection or page input.
    try {
      assertHeld();
      const pid = await this.platform.launch(binary, [`--user-data-dir=${this.profile}`,
        '--disable-extensions', '--no-first-run', '--no-default-browser-check', '--restore-last-session']);
      const deadline = Date.now() + 10000;
      for (;;) {
        const owners = await this.platform.inventory(this.profile);
        const owner = owners.find(item => item.pid === pid && !item.automated
          && path.resolve(item.binary).toLowerCase() === path.resolve(binary).toLowerCase());
        if (owners.length === 1 && owner?.visible) {
          assertHeld();
          this.lease = { ...this.lease, ...owner, binary };
          await writeFile(this.leasePath(), JSON.stringify(this.lease), { mode: 0o600 });
          const focus = await this.platform.focus(owner);
          return { status: 'OPEN', mode: 'HUMAN_ONLY', reused: false, focus };
        }
        if (Date.now() >= deadline) throw new Error('SECURITY_BROWSER_START_UNPROVEN');
        await new Promise(resolve => setTimeout(resolve, 200));
      }
    } catch (error) {
      if (!(await this.platform.inventory(this.profile)).length) await this.releaseLease();
      throw error;
    }
  }
  async openForAuthentication() {
    if (await this.authRequired()) return this.openHumanSecurity();
    const reused = !!this.browser;
    const browser = await this.connection();
    const pages = await browser.pages();
    if (pages.length !== 1) return { status: 'AMBIGUOUS_PAGE' };
    if (pages[0].url() === 'about:blank') await pages[0].goto('https://chatgpt.com', { waitUntil: 'domcontentloaded', timeout: 20000 });
    // Explicit Open displays this already-owned native window. Binding and
    // background reads never focus, close or recreate it.
    const owners = await this.platform.inventory(this.profile);
    const owner = owners.length === 1 ? owners[0] : null;
    const pid = browser.process()?.pid;
    if (!owner || owner.pid !== pid || !owner.automated
      || path.resolve(owner.binary).toLowerCase() !== path.resolve(this.resources, 'chromium', 'chrome.exe').toLowerCase()) {
      throw new Error('SECURITY_BROWSER_IDENTITY_CHANGED');
    }
    const focus = await this.platform.focus(owner);
    return { status: 'OPEN', substrate: 'OFFICIAL_NON_BRANDED_CHROMIUM', reused, focus };
  }
  async shutdown() {
    // Explicit product Quit owns this cleanup; never invoked as a provider retry.
    if (this.browser) await this.browser.close();
    this.browser = null;
    // A human security session survives Host shutdown; only explicit completion
    // may end it. Automated sessions release only after the browser exited.
    if (this.lease?.mode === 'AUTOMATED' && !(await this.platform.inventory(this.profile)).length) await this.releaseLease();
  }
}
