// Sidecar selection follows the build machine. A Windows PE is never renamed
// into a Darwin artifact. Cross-architecture packaging needs a native runner.
export function nodeSidecar(platform, arch, target) {
  const supported = {
    'win32-x64': ['x86_64-pc-windows-msvc', '.exe'],
    'darwin-arm64': ['aarch64-apple-darwin', ''],
    'darwin-x64': ['x86_64-apple-darwin', ''],
  };
  const row = supported[`${platform}-${arch}`];
  if (!row || (target && target !== row[0])) throw Error('NODE_SIDECAR_TARGET_MISMATCH');
  return `browser-executor-node-${row[0]}${row[1]}`;
}
