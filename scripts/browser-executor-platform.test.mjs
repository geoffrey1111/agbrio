import {test} from 'node:test';
import assert from 'node:assert/strict';
import {nodeSidecar} from './browser-executor-platform.mjs';

test('sidecars require a matching native OS and architecture', () => {
  assert.equal(nodeSidecar('win32','x64'), 'browser-executor-node-x86_64-pc-windows-msvc.exe');
  assert.equal(nodeSidecar('darwin','arm64','aarch64-apple-darwin'), 'browser-executor-node-aarch64-apple-darwin');
  assert.equal(nodeSidecar('darwin','x64'), 'browser-executor-node-x86_64-apple-darwin');
  assert.throws(() => nodeSidecar('win32','x64','aarch64-apple-darwin'));
  assert.throws(() => nodeSidecar('darwin','arm64','x86_64-apple-darwin'));
  assert.throws(() => nodeSidecar('linux','x64'));
});
