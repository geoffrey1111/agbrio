param([Parameter(Mandatory)][string]$Request,[Parameter(Mandatory)][ValidatePattern('^[a-f0-9]{64}$')][string]$Digest)
$ErrorActionPreference='Stop'
$job=$null;$process=$null;$client=$null;$mutex=$null;$held=$false;$closedProof=$null
function Test-AgbrioPiToolList([string]$Value,$Resources,[bool]$Writable,[bool]$Terminal) {
 foreach($tool in $Value.Split(',')) {
  if($tool -cin @('read','edit','write','grep','find','ls','powershell')) { continue }
  if(!$Writable -or !$Terminal -or $tool -cnotmatch '^mcp__([A-Za-z0-9_]+)__\*$') { return $false }
  if($Matches[1] -cnotin @($Resources.mcpServers)) { return $false }
 }
 return $true
}
try {
 $identity=[Security.Principal.WindowsIdentity]::GetCurrent()
 if (!([Security.Principal.WindowsPrincipal]::new($identity)).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'PI_ADMINISTRATOR_REQUIRED' }
 $resolved=[IO.Path]::GetFullPath($Request)
 $base=[IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'AIWorkRouter/data/pi-elevation'))
 if (!$resolved.StartsWith($base+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase) -or [IO.Path]::GetFileName($resolved) -ne 'request.json') { throw 'PI_ELEVATION_REQUEST_INVALID' }
 $directory=Split-Path -Parent $resolved
 if ((Split-Path -Leaf $directory) -notmatch '^[a-f0-9-]{36}$') { throw 'PI_ELEVATION_REQUEST_INVALID' }
 $walk=Get-Item -LiteralPath $directory
 while ($walk -and $walk.FullName.Length -ge $base.Length) { if ($walk.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'PI_ELEVATION_LINK_DENIED' };$walk=$walk.Parent }
 if ((Get-Item -LiteralPath $resolved).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'PI_ELEVATION_LINK_DENIED' }
 if ((Get-FileHash -LiteralPath $resolved -Algorithm SHA256).Hash.ToLowerInvariant() -cne $Digest) { throw 'PI_ELEVATION_REQUEST_CHANGED' }
 $spec=Get-Content -LiteralPath $resolved -Raw -Encoding utf8 | ConvertFrom-Json
 if ($spec.ownerSid -cne $identity.User.Value -or [Math]::Abs([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()-[long]$spec.createdAt) -gt 120000) { throw 'PI_ELEVATION_OWNER_CHANGED' }
 $caller=Get-CimInstance Win32_Process -Filter ('ProcessId='+[int]$spec.callerPid)
 if (!$caller -or $caller.ExecutablePath -ine $spec.callerImage -or (Get-Process -Id $spec.callerPid).StartTime.ToFileTimeUtc().ToString() -cne $spec.callerCreatedAt -or (Get-FileHash -LiteralPath $caller.ExecutablePath).Hash.ToLowerInvariant() -cne $spec.callerHash) { throw 'PI_ELEVATION_CALLER_CHANGED' }
 $callerOwner=Invoke-CimMethod -InputObject $caller -MethodName GetOwnerSid
 if ($callerOwner.Sid -cne $identity.User.Value) { throw 'PI_ELEVATION_CALLER_CHANGED' }
 $node=Join-Path $env:ProgramFiles 'nodejs/node.exe'
 $cli=Join-Path $env:APPDATA 'npm/node_modules/@earendil-works/pi-coding-agent/dist/bundle/cli.js'
 if ([IO.Path]::GetFullPath([string]$spec.node) -ine [IO.Path]::GetFullPath($node) -or [IO.Path]::GetFullPath([string]$spec.argv[0]) -ine [IO.Path]::GetFullPath($cli) -or !(Test-Path -LiteralPath $cli -PathType Leaf)) { throw 'PI_ELEVATION_EXECUTABLE_INVALID' }
 $approvedResources=[pscustomobject]@{extensions=@();mcpServers=@()}
 if($spec.environment.AGBRIO_PI_ACCESS_FILE){$accessBytes=[IO.File]::ReadAllBytes([string]$spec.environment.AGBRIO_PI_ACCESS_FILE);$accessHash=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($accessBytes)).ToLowerInvariant();if($accessHash -cne $spec.environment.AGBRIO_PI_ACCESS_DIGEST){throw 'PI_ACCESS_CHANGED'};$approvedResources=([Text.Encoding]::UTF8.GetString($accessBytes)|ConvertFrom-Json).resources}
 $allowed=@('--mode','--session-dir','--extension','--tools','--session','--skill','--append-system-prompt')
 $switches=@('--no-extensions','--no-mcp','--no-skills','--no-prompt-templates','--no-themes','--no-context-files','--no-tools')
 if ($spec.argv.Count -gt 1024 -or !$spec.argv.Contains('--no-extensions') -or !$spec.argv.Contains('--no-mcp')) { throw 'PI_ELEVATION_ARGUMENTS_INVALID' }
 for ($i=1;$i -lt $spec.argv.Count;$i++) {
  $arg=[string]$spec.argv[$i]
  if ($switches -contains $arg) { continue }
  if ($allowed -notcontains $arg -or $i+1 -ge $spec.argv.Count) { throw 'PI_ELEVATION_ARGUMENTS_INVALID' }
  $value=[string]$spec.argv[++$i]
  if ($value.Contains([char]0) -or $value.Length -gt 8192) { throw 'PI_ELEVATION_ARGUMENTS_INVALID' }
  if ($arg -eq '--mode' -and $value -ne 'rpc') { throw 'PI_ELEVATION_ARGUMENTS_INVALID' }
  if ($arg -eq '--tools' -and !(Test-AgbrioPiToolList $value $approvedResources ($spec.environment.AGBRIO_PI_WRITE -ceq '1') ($spec.environment.AGBRIO_PI_TERMINAL -ceq '1'))) { throw 'PI_ELEVATION_ARGUMENTS_INVALID' }
  if ($arg -eq '--extension') {
   $guard=[IO.Path]::GetFullPath($value);$nativeBase=[IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'AIWorkRouter/data/pi-executor'))
   $guardOK=$guard.StartsWith($nativeBase+'\',[StringComparison]::OrdinalIgnoreCase) -and [IO.Path]::GetFileName($guard) -eq 'agbrio-guard.mjs' -and (Get-FileHash -LiteralPath $guard).Hash -ceq (Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'guard.mjs')).Hash
   $shimOK=$guard.StartsWith($nativeBase+'\',[StringComparison]::OrdinalIgnoreCase) -and [IO.Path]::GetFileName($guard) -eq 'agbrio-mcp.mjs' -and $approvedResources.mcpServers.Count -gt 0 -and (Get-FileHash -LiteralPath $guard).Hash -ceq (Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'mcp.mjs')).Hash
   $extensionOK=@($approvedResources.extensions|Where-Object {[IO.Path]::GetFullPath([string]$_.path) -ieq $guard -and $_.sha256 -ceq (Get-FileHash -LiteralPath $guard).Hash.ToLowerInvariant()}).Count -eq 1
   if(!$guardOK -and !$shimOK -and !$extensionOK){throw 'PI_ELEVATION_GUARD_INVALID'}
  }
 }
 $sessionIndex=[Array]::IndexOf([object[]]$spec.argv,'--session-dir')
 if ($sessionIndex -lt 0 -or $sessionIndex+1 -ge $spec.argv.Count) { throw 'PI_ELEVATION_ARGUMENTS_INVALID' }
 $sessionDirectory=[IO.Path]::GetFullPath([string]$spec.argv[$sessionIndex+1])
 $sha=[Security.Cryptography.SHA256]::Create();try{$scopeKey=([BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($sessionDirectory)))).Replace('-','')}finally{$sha.Dispose()}
 $mutex=[Threading.Mutex]::new($false,('Local\Agbrio.Pi.Writer.'+$scopeKey))
 try{$held=$mutex.WaitOne(0)}catch [Threading.AbandonedMutexException]{$held=$true}
 if (!$held) { throw 'PI_WRITER_ALREADY_ACTIVE' }
 $claim=[IO.File]::Open((Join-Path $directory 'claimed'),[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None);$claim.Dispose()
 Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public sealed class AgbrioPiJob : IDisposable {
 [StructLayout(LayoutKind.Sequential)] struct Basic {public long ProcessTime,JobTime;public uint Flags;public UIntPtr Minimum,Maximum;public uint Active;public UIntPtr Affinity;public uint Priority,Scheduling;}
 [StructLayout(LayoutKind.Sequential)] struct IO {public ulong A,B,C,D,E,F;}
 [StructLayout(LayoutKind.Sequential)] struct Extended {public Basic Basic;public IO IO;public UIntPtr ProcessMemory,JobMemory,PeakProcess,PeakJob;}
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern IntPtr CreateJobObject(IntPtr a,string n);
 [DllImport("kernel32.dll",SetLastError=true)] static extern bool SetInformationJobObject(IntPtr h,int c,ref Extended v,uint n);
 [DllImport("kernel32.dll",SetLastError=true)] static extern bool AssignProcessToJobObject(IntPtr h,IntPtr p);
 [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
 [DllImport("kernel32.dll")] static extern bool TerminateJobObject(IntPtr h,uint code);
 [DllImport("kernel32.dll")] static extern bool QueryInformationJobObject(IntPtr h,int c,IntPtr v,uint n,IntPtr len);
 [DllImport("kernel32.dll")] static extern IntPtr LocalFree(IntPtr h);
 [DllImport("advapi32.dll",EntryPoint="GetTokenInformation",SetLastError=true)] static extern bool TokenInfo(IntPtr t,int c,IntPtr v,int n,out int l);
 [DllImport("advapi32.dll",CharSet=CharSet.Unicode)] static extern bool ConvertSidToStringSid(IntPtr sid,out IntPtr text);
 [DllImport("advapi32.dll",SetLastError=true)] static extern bool OpenProcessToken(IntPtr p,uint a,out IntPtr t);
 [DllImport("advapi32.dll",SetLastError=true)] static extern bool GetTokenInformation(IntPtr t,int c,out int v,int n,out int l);
 IntPtr handle;
 public AgbrioPiJob(IntPtr process){handle=CreateJobObject(IntPtr.Zero,null);var info=new Extended();info.Basic.Flags=0x2000;if(handle==IntPtr.Zero||!SetInformationJobObject(handle,9,ref info,(uint)Marshal.SizeOf<Extended>())||!AssignProcessToJobObject(handle,process)){Dispose();throw new Exception("PI_JOB_ASSIGN_FAILED");}}
 public static bool Elevated(IntPtr process){IntPtr token;int elevated,length;if(!OpenProcessToken(process,8,out token))throw new Exception("PI_TOKEN_UNCONFIRMED");try{if(!GetTokenInformation(token,20,out elevated,4,out length))throw new Exception("PI_TOKEN_UNCONFIRMED");return elevated!=0;}finally{CloseHandle(token);}}
 public static string OwnerSid(IntPtr process){IntPtr token;if(!OpenProcessToken(process,8,out token))throw new Exception("PI_OWNER_UNCONFIRMED");try{int size;TokenInfo(token,1,IntPtr.Zero,0,out size);if(size<=0||size>65536)throw new Exception("PI_OWNER_UNCONFIRMED");var buffer=Marshal.AllocHGlobal(size);try{if(!TokenInfo(token,1,buffer,size,out size))throw new Exception("PI_OWNER_UNCONFIRMED");IntPtr text;if(!ConvertSidToStringSid(Marshal.ReadIntPtr(buffer),out text))throw new Exception("PI_OWNER_UNCONFIRMED");try{return Marshal.PtrToStringUni(text);}finally{LocalFree(text);}}finally{Marshal.FreeHGlobal(buffer);}}finally{CloseHandle(token);}}
 public bool CloseConfirmed(){if(handle==IntPtr.Zero)return true;TerminateJobObject(handle,0);var buffer=Marshal.AllocHGlobal(48);try{for(int i=0;i<150;i++){if(QueryInformationJobObject(handle,1,buffer,48,IntPtr.Zero)&&Marshal.ReadInt32(buffer,40)==0){Dispose();return true;}System.Threading.Thread.Sleep(20);}return false;}finally{Marshal.FreeHGlobal(buffer);}}
 public void Dispose(){if(handle!=IntPtr.Zero){CloseHandle(handle);handle=IntPtr.Zero;}}
}
'@
 $start=[Diagnostics.ProcessStartInfo]::new($node);$start.UseShellExecute=$false;$start.CreateNoWindow=$true;$start.WorkingDirectory=$spec.cwd
 $start.RedirectStandardInput=$true;$start.RedirectStandardOutput=$true;$start.RedirectStandardError=$true
 foreach($arg in $spec.argv){$start.ArgumentList.Add([string]$arg)}
 $start.Environment.Clear()
 $environmentKeys=@('SystemRoot','WINDIR','USERPROFILE','HOME','HOMEDRIVE','HOMEPATH','APPDATA','LOCALAPPDATA','ProgramFiles','ProgramFiles(x86)','ProgramData','PATH','PATHEXT','ComSpec','TEMP','TMP','LANG','LC_ALL','TZ','SYSTEMDRIVE','NUMBER_OF_PROCESSORS','HTTP_PROXY','HTTPS_PROXY','ALL_PROXY','NO_PROXY','http_proxy','https_proxy','all_proxy','no_proxy','NODE_USE_ENV_PROXY','AGBRIO_PI_SCOPE','AGBRIO_PI_WRITE','AGBRIO_PI_CAP','AGBRIO_PI_TERMINAL','AGBRIO_PI_ACCESS','AGBRIO_PI_ACCESS_FILE','AGBRIO_PI_ACCESS_DIGEST','AGBRIO_PI_RESOURCES')
 foreach($property in $spec.environment.PSObject.Properties){if($environmentKeys -notcontains $property.Name){throw 'PI_ELEVATION_ENVIRONMENT_INVALID'};$start.Environment[$property.Name]=[string]$property.Value}
 $client=[Net.Sockets.TcpClient]::new();$client.Connect('127.0.0.1',[int]$spec.port);$stream=$client.GetStream()
 $process=[Diagnostics.Process]::Start($start);$job=[AgbrioPiJob]::new($process.Handle)
 $childSid=[AgbrioPiJob]::OwnerSid($process.Handle)
 if ($childSid -cne $identity.User.Value) { throw 'PI_ELEVATION_OWNER_CHANGED' }
 if (![AgbrioPiJob]::Elevated($process.Handle)) { throw 'PI_ADMINISTRATOR_TOKEN_MISMATCH' }
 $proof=@{nonce=$spec.nonce;requestHash=$Digest;pid=$process.Id;ownerSid=$childSid;administrator=$true;createdAt=$process.StartTime.ToFileTimeUtc().ToString()}|ConvertTo-Json -Compress
 $bytes=[Text.Encoding]::UTF8.GetBytes($proof+"`n");$stream.Write($bytes,0,$bytes.Length);$stream.Flush()
 $outputCopy=$process.StandardOutput.BaseStream.CopyToAsync($stream)
 $errorDrain=$process.StandardError.BaseStream.CopyToAsync([IO.Stream]::Null)
 $inputCopy=$stream.CopyToAsync($process.StandardInput.BaseStream)
 [Threading.Tasks.Task]::WaitAny(@($outputCopy,$inputCopy))|Out-Null
 $process.StandardInput.Close()
 if (!$process.WaitForExit(3000)) { $job.Dispose() }
 if (!$process.WaitForExit(3000)) { throw 'PI_ELEVATED_CLOSE_UNCONFIRMED' }
 if (!$job.CloseConfirmed()) { throw 'PI_ELEVATED_CLOSE_UNCONFIRMED' }
 $closedProof=@{closed=$true;pid=$process.Id;requestHash=$Digest}
} catch {
 $code=$_.Exception.Message
 if ($code -notmatch '^PI_[A-Z_]+$') { $code='PI_ELEVATION_FAILED' }
 if ($directory -and (Test-Path -LiteralPath $directory)) { @{errorCode=$code}|ConvertTo-Json -Compress|Set-Content -LiteralPath (Join-Path $directory 'failed.json') -Encoding utf8 }
 exit 1
} finally {
 if ($job) {$job.Dispose()};if ($client) {$client.Dispose()};if ($process) {try{if (!$process.HasExited){$process.Kill($true);$process.WaitForExit(3000)|Out-Null}}catch{};$process.Dispose()}
 if ($mutex) {if ($held) {$mutex.ReleaseMutex()};$mutex.Dispose()}
 if ($closedProof) {$closedProof|ConvertTo-Json -Compress|Set-Content -LiteralPath (Join-Path $directory 'closed.json') -Encoding utf8}
}
