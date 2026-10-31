param([Parameter(Mandatory)][string]$Request,[Parameter(Mandatory)][ValidatePattern('^[a-f0-9]{64}$')][string]$Digest)
$ErrorActionPreference='Stop'
$client=$null;$job=$null
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
 if ($spec.ownerSid -cne $identity.User.Value -or [Math]::Abs([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()-[long]$spec.createdAt) -gt 600000) { throw 'PI_ELEVATION_OWNER_CHANGED' }
 $caller=Get-CimInstance Win32_Process -Filter ('ProcessId='+[int]$spec.callerPid)
 if (!$caller -or $caller.ExecutablePath -ine $spec.callerImage -or (Get-Process -Id $spec.callerPid).StartTime.ToFileTimeUtc().ToString() -cne $spec.callerCreatedAt -or (Get-FileHash -LiteralPath $caller.ExecutablePath).Hash.ToLowerInvariant() -cne $spec.callerHash) { throw 'PI_ELEVATION_CALLER_CHANGED' }
 $callerOwner=Invoke-CimMethod -InputObject $caller -MethodName GetOwnerSid
 if ($callerOwner.Sid -cne $identity.User.Value) { throw 'PI_ELEVATION_CALLER_CHANGED' }
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

 $claim=[IO.File]::Open((Join-Path $directory 'claimed'),[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None);$claim.Dispose()
 # This process owns a nested kill-on-close job; parent socket loss collects all relays.
 $self=[Diagnostics.Process]::GetCurrentProcess()
 $job=[AgbrioPiJob]::new($self.Handle)
 $client=[Net.Sockets.TcpClient]::new();$client.Connect('127.0.0.1',[int]$spec.port)
 $stream=$client.GetStream();$reader=[IO.StreamReader]::new($stream,[Text.UTF8Encoding]::new($false),$false,1024,$true)
 $writer=[IO.StreamWriter]::new($stream,[Text.UTF8Encoding]::new($false),1024,$true);$writer.AutoFlush=$true
 $writer.WriteLine((@{nonce=$spec.nonce;requestHash=$Digest;pid=$PID;ownerSid=$identity.User.Value;administrator=[AgbrioPiJob]::Elevated($self.Handle);createdAt=$self.StartTime.ToFileTimeUtc().ToString()}|ConvertTo-Json -Compress))
 while ($null -ne ($line=$reader.ReadLine())) {
  if ($line.Length -gt 8192) {throw 'PI_COHORT_REQUEST_INVALID'}
  $call=$line|ConvertFrom-Json
  if ($call.operation -eq 'PING') {$writer.WriteLine('{"ready":true}');continue}
  if ($call.operation -ne 'LAUNCH' -or $call.digest -notmatch '^[a-f0-9]{64}$') {throw 'PI_COHORT_REQUEST_INVALID'}
  $childPath=[IO.Path]::GetFullPath([string]$call.request)
  if (!$childPath.StartsWith($base+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase) -or [IO.Path]::GetFileName($childPath) -ne 'request.json') {throw 'PI_COHORT_REQUEST_INVALID'}
  if ((Get-FileHash -LiteralPath $childPath).Hash.ToLowerInvariant() -cne $call.digest) {throw 'PI_ELEVATION_REQUEST_CHANGED'}
  $child=Get-Content -LiteralPath $childPath -Raw -Encoding utf8|ConvertFrom-Json
  foreach($field in @('ownerSid','callerPid','callerCreatedAt','callerImage','callerHash')) {if ([string]$child.$field -cne [string]$spec.$field) {throw 'PI_ELEVATION_CALLER_CHANGED'}}
  $helper=Join-Path $PSScriptRoot 'elevated.ps1'
  if ((Get-FileHash -LiteralPath $helper).Hash.ToLowerInvariant() -cne $spec.helperHash) {throw 'PI_ELEVATION_HELPER_CHANGED'}
  $start=[Diagnostics.ProcessStartInfo]::new((Join-Path $env:ProgramFiles 'PowerShell/7/pwsh.exe'));$start.UseShellExecute=$false;$start.CreateNoWindow=$true
  foreach($arg in @('-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',$helper,'-Request',$childPath,'-Digest',[string]$call.digest)) {$start.ArgumentList.Add($arg)}
  $relay=[Diagnostics.Process]::Start($start)
  $writer.WriteLine((@{accepted=$true;relayPid=$relay.Id}|ConvertTo-Json -Compress));$relay.Dispose()
 }
} catch {
 $code=$_.Exception.Message;if($code -notmatch '^PI_[A-Z_]+$'){$code='PI_COHORT_FAILED'}
 if($directory -and (Test-Path -LiteralPath $directory)){@{errorCode=$code}|ConvertTo-Json -Compress|Set-Content -LiteralPath (Join-Path $directory 'failed.json') -Encoding utf8}
} finally {if($client){$client.Dispose()};if($job){$job.Dispose()}}
