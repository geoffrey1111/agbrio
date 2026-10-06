// Native Windows lifecycle only. Never imports a browser automation framework.
import { execFile, spawn } from 'node:child_process';
import { promisify } from 'node:util';
import path from 'node:path';
import { createHash } from 'node:crypto';
const execute = promisify(execFile);
const PROFILE_GUARD = String.raw`
$ErrorActionPreference = 'Stop'
$guard = [Threading.Mutex]::new($false, $env:AIWR_SECURITY_MUTEX)
$held = $false
try {
  try { $held = $guard.WaitOne(0) } catch [Threading.AbandonedMutexException] { $held = $true }
  if (!$held) { [Console]::Out.WriteLine('BUSY'); exit 0 }
  [Console]::Out.WriteLine('ACQUIRED'); [Console]::Out.Flush()
  [Console]::In.ReadLine() | Out-Null
} finally { if ($held) { $guard.ReleaseMutex() }; $guard.Dispose() }
`;
// One short-lived kernel mutex serializes explicit human recovery actions.
// EOF/termination releases it; there is no persistent stale lock to remove.
async function withExclusiveProfile(profile, action) {
  if (process.platform !== 'win32') throw new Error('SECURITY_RECOVERY_WINDOWS_ONLY');
  const shell = path.join(process.env.SystemRoot, 'System32', 'WindowsPowerShell', 'v1.0', 'powershell.exe');
  const name = 'Local\\AIWR-Security-' + createHash('sha256').update(path.resolve(profile).replaceAll('/', '\\').toLowerCase()).digest('hex');
  const guard = spawn(shell, ['-NoProfile', '-NonInteractive', '-Command', PROFILE_GUARD], {
    windowsHide: true, stdio: ['pipe', 'pipe', 'ignore'], env: { ...process.env, AIWR_SECURITY_MUTEX: name },
  });
  let locked = false;
  let guardLost = false;
  guard.on('exit', () => { guardLost = true; });
  guard.stdin.on('error', () => {});
  try {
    await new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error('SECURITY_RECOVERY_GUARD_UNAVAILABLE')), 10000);
      let output = '';
      const finish = (error) => { clearTimeout(timer); error ? reject(error) : resolve(); };
      guard.once('error', () => finish(new Error('SECURITY_RECOVERY_GUARD_UNAVAILABLE')));
      guard.once('exit', () => { if (!locked) finish(new Error('SECURITY_RECOVERY_GUARD_UNAVAILABLE')); });
      guard.stdout.on('data', chunk => {
        output += chunk;
        if (output.includes('ACQUIRED')) { locked = true; finish(); }
        else if (output.includes('BUSY')) finish(new Error('SECURITY_RECOVERY_BUSY'));
      });
    });
    if (guardLost) throw new Error('SECURITY_RECOVERY_GUARD_UNAVAILABLE');
    return await action(() => { if (guardLost) throw new Error('SECURITY_RECOVERY_GUARD_UNAVAILABLE'); });
  } finally {
    const ended = new Promise(resolve => {
      if (!guard.pid || guard.exitCode !== null || guard.signalCode !== null) return resolve();
      const timeout = setTimeout(() => guard.kill(), 5000);
      guard.once('close', () => { clearTimeout(timeout); resolve(); });
    });
    guard.stdin.end('\n');
    if (!locked) guard.kill();
    await ended;
  }
}
const INVENTORY = String.raw`
$ErrorActionPreference = 'Stop'
$target = [IO.Path]::GetFullPath($env:AIWR_SECURITY_PROFILE).TrimEnd('\').Replace('/', '\')
$rows = @(Get-CimInstance Win32_Process -Filter "Name='chrome.exe'" | ForEach-Object {
  $line = $_.CommandLine
  if ($line -and $line -notmatch '--type=' -and $line -match '(?:"--user-data-dir=([^"]+)"|--user-data-dir="([^"]+)"|--user-data-dir=([^\s"]+))') {
    $profile = if ($Matches[1]) { $Matches[1] } elseif ($Matches[2]) { $Matches[2] } else { $Matches[3] }
    if ([IO.Path]::GetFullPath($profile).TrimEnd('\').Replace('/', '\') -ieq $target) {
      @{ pid = $_.ProcessId; binary = $_.ExecutablePath; started = $_.CreationDate.ToUniversalTime().ToString('o'); automated = ($line -match '--remote-debugging|--enable-automation|--headless'); visible = ([Diagnostics.Process]::GetProcessById($_.ProcessId).MainWindowHandle -ne 0) }
    }
  }
})
ConvertTo-Json -InputObject $rows -Compress
`;
const CLOSE = String.raw`
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class AIWRRecoveryWindow {
  public delegate bool Callback(IntPtr window, IntPtr data);
  [DllImport("user32.dll")] public static extern bool EnumWindows(Callback callback, IntPtr data);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr window, uint message, IntPtr w, IntPtr l);
  public static void Close(uint owner) { EnumWindows((window, data) => { uint pid; GetWindowThreadProcessId(window, out pid); if(pid == owner) PostMessage(window, 0x0010, IntPtr.Zero, IntPtr.Zero); return true; }, IntPtr.Zero); }
}
'@
$owner = Get-CimInstance Win32_Process -Filter ('ProcessId=' + [uint32]$env:AIWR_SECURITY_PID)
if (!$owner -or $owner.ExecutablePath -ine $env:AIWR_SECURITY_BINARY -or $owner.CreationDate.ToUniversalTime().ToString('o') -ne $env:AIWR_SECURITY_STARTED) { throw 'SECURITY_BROWSER_IDENTITY_CHANGED' }
[AIWRRecoveryWindow]::Close([uint32]$env:AIWR_SECURITY_PID)
`;
const FOCUS = String.raw`
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class AIWRRecoveryFocus {
  [DllImport("user32.dll")] public static extern bool ShowWindowAsync(IntPtr window, int command);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
  [StructLayout(LayoutKind.Sequential)] public struct FlashInfo { public uint size; public IntPtr window; public uint flags; public uint count; public uint timeout; }
  [DllImport("user32.dll")] public static extern bool FlashWindowEx(ref FlashInfo info);
  public static void Attention(IntPtr window) {
    var info = new FlashInfo { size = (uint)Marshal.SizeOf(typeof(FlashInfo)), window = window, flags = 2, count = 3, timeout = 0 };
    FlashWindowEx(ref info);
  }
}
'@
$owner = Get-CimInstance Win32_Process -Filter ('ProcessId=' + [uint32]$env:AIWR_SECURITY_PID)
if (!$owner -or $owner.ExecutablePath -ine $env:AIWR_SECURITY_BINARY -or $owner.CreationDate.ToUniversalTime().ToString('o') -ne $env:AIWR_SECURITY_STARTED) { throw 'SECURITY_BROWSER_IDENTITY_CHANGED' }
$window = [Diagnostics.Process]::GetProcessById($owner.ProcessId).MainWindowHandle
if ($window -eq 0) { throw 'SECURITY_BROWSER_WINDOW_UNAVAILABLE' }
[AIWRRecoveryFocus]::ShowWindowAsync($window, 9) | Out-Null
[AIWRRecoveryFocus]::SetForegroundWindow($window) | Out-Null
$deadline = [DateTime]::UtcNow.AddSeconds(1)
do {
  [uint32]$foregroundPid = 0
  [AIWRRecoveryFocus]::GetWindowThreadProcessId([AIWRRecoveryFocus]::GetForegroundWindow(), [ref]$foregroundPid) | Out-Null
  if ($foregroundPid -eq $owner.ProcessId) { [Console]::Out.WriteLine('FOREGROUND'); exit 0 }
  Start-Sleep -Milliseconds 50
} while ([DateTime]::UtcNow -lt $deadline)
[AIWRRecoveryFocus]::Attention($window)
[Console]::Out.WriteLine('ATTENTION')
`;
async function powershell(script, environment) {
  if (process.platform !== 'win32') throw new Error('SECURITY_RECOVERY_WINDOWS_ONLY');
  const shell = path.join(process.env.SystemRoot, 'System32', 'WindowsPowerShell', 'v1.0', 'powershell.exe');
  return execute(shell, ['-NoProfile', '-NonInteractive', '-Command', script], {
    windowsHide: true, timeout: 10000, maxBuffer: 65536, env: { ...process.env, ...environment },
  });
}
export const securityPlatform = {
  withExclusiveProfile,
  async inventory(profile) {
    const { stdout } = await powershell(INVENTORY, { AIWR_SECURITY_PROFILE: profile });
    return JSON.parse(stdout.trim());
  },
  async launch(binary, args) {
    const child = spawn(binary, args, { detached: true, stdio: 'ignore', windowsHide: false });
    await new Promise((resolve, reject) => { child.once('spawn', resolve); child.once('error', reject); });
    child.unref();
    return child.pid;
  },
  async close(record) {
    await powershell(CLOSE, { AIWR_SECURITY_PID: String(record.pid), AIWR_SECURITY_BINARY: record.binary, AIWR_SECURITY_STARTED: record.started });
  },
  async focus(record) {
    try {
      const { stdout } = await powershell(FOCUS, { AIWR_SECURITY_PID: String(record.pid), AIWR_SECURITY_BINARY: record.binary, AIWR_SECURITY_STARTED: record.started });
      const result = stdout.trim();
      if (result !== 'FOREGROUND' && result !== 'ATTENTION') throw new Error('SECURITY_BROWSER_FOCUS_FAILED');
      return result;
    } catch (error) {
      const code = ['SECURITY_BROWSER_IDENTITY_CHANGED', 'SECURITY_BROWSER_WINDOW_UNAVAILABLE', 'SECURITY_BROWSER_FOCUS_BLOCKED']
        .find(value => String(error.stderr).includes(value));
      throw new Error(code ?? 'SECURITY_BROWSER_FOCUS_FAILED');
    }
  },
};
