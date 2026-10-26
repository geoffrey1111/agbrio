param([ValidateSet('Inspect','Start','Child')][string]$Action,[string]$Directory,[string]$JobId,[ValidateSet('administrator','standard')][string]$Mode)
$ErrorActionPreference='Stop'
$stage='IDENTITY'
try {
 $identity=[Security.Principal.WindowsIdentity]::GetCurrent()
 $administrator=([Security.Principal.WindowsPrincipal]::new($identity)).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
 if($Action -eq 'Inspect'){$caller=(Get-CimInstance Win32_Process -Filter ('ProcessId='+$PID)).ParentProcessId;@{ownerSid=$identity.User.Value;elevated=$administrator;callerPid=$caller;callerCreatedAt=(Get-Process -Id $caller).StartTime.ToFileTimeUtc().ToString()}|ConvertTo-Json -Compress;exit 0}
 $stage='REQUEST';$expected=[IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'AIWorkRouter/codex-shared'))
 if([IO.Path]::GetFullPath($Directory) -ine $expected -or $JobId -notmatch '^[a-f0-9-]{36}$'){throw 'SHARED_PRIVILEGE_REQUEST_INVALID'}
 $requestPath=Join-Path $expected ('launch-jobs/'+$JobId+'/request.json')
 $request=Get-Content -LiteralPath $requestPath -Raw -Encoding UTF8 | ConvertFrom-Json
 if($request.ownerSid -cne $identity.User.Value -or $request.id -cne $JobId -or $request.mode -cne $Mode){throw 'SHARED_PRIVILEGE_REQUEST_INVALID'}
 $stage='CALLER';$parentId=if($Action -eq 'Child'){[int]$request.callerPid}else{(Get-CimInstance Win32_Process -Filter ('ProcessId='+$PID)).ParentProcessId}
 $parent=& (Join-Path $PSScriptRoot 'peer-service.ps1') -QueryPid $parentId | ConvertFrom-Json
 if(!$parent -or $parent.pid -ne $request.callerPid -or $parent.ownerSid -cne $identity.User.Value -or $parent.imagePath -ine $request.node -or $parent.createdAt -cne $request.callerCreatedAt){throw 'SHARED_PRIVILEGE_CALLER_CHANGED'}
 function Quote-Argument([string]$Value){if($Value.Contains('"') -or $Value.Contains([char]0)){throw 'SHARED_PRIVILEGE_REQUEST_INVALID'};return '"'+$Value+'"'}
 if($Action -eq 'Child'){
  if($administrator -ne ($Mode -eq 'administrator')){throw 'SHARED_PRIVILEGE_TOKEN_MISMATCH'}
  if($request.kind -eq 'probe'){
   & $request.node (Join-Path $PSScriptRoot 'privilege.mjs') --child $requestPath
   if($LASTEXITCODE){throw 'SHARED_PRIVILEGE_UNCONFIRMED'}
   $completed=[IO.File]::Open((Join-Path (Split-Path -Parent $requestPath) 'released'),[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None);$completed.Dispose()
   exit 0
  }
  $sha=[Security.Cryptography.SHA256]::Create();try{$key=([BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($expected)))).Replace('-','')}finally{$sha.Dispose()}
  $mutex=[Threading.Mutex]::new($false,('Local\Agbrio.SharedCodex.Privilege.'+$key));$held=$false
  try{try{$held=$mutex.WaitOne(0)}catch [Threading.AbandonedMutexException]{$held=$true};if(!$held){throw 'SHARED_START_UNAVAILABLE'}
   & $request.node (Join-Path $PSScriptRoot 'privilege.mjs') --child $requestPath
   if($LASTEXITCODE){throw 'SHARED_PRIVILEGE_UNCONFIRMED'}
  }finally{if($held){$mutex.ReleaseMutex()};$mutex.Dispose()}
  $completed=[IO.File]::Open((Join-Path (Split-Path -Parent $requestPath) 'released'),[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None);$completed.Dispose()
  exit 0
 }
 $powershell=Join-Path $env:WINDIR 'System32/WindowsPowerShell/v1.0/powershell.exe'
 $programRoot=if($env:ProgramW6432){$env:ProgramW6432}else{$env:ProgramFiles}
 if($programRoot){$core=Join-Path $programRoot 'PowerShell/7/pwsh.exe';if(Test-Path -LiteralPath $core -PathType Leaf){$powershell=$core}}
 $arguments='-NoProfile -NonInteractive -ExecutionPolicy Bypass -File '+(Quote-Argument (Join-Path $PSScriptRoot 'privilege.ps1'))+' -Action Child -Directory '+(Quote-Argument $expected)+' -JobId '+$JobId+' -Mode '+$Mode
 $stage='LAUNCH';if($Mode -eq 'administrator'){
  # Retain the native UAC cancellation code rather than Start-Process's wrapped exception.
  Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class AgbrioPrivilegeShell {
 [StructLayout(LayoutKind.Sequential,CharSet=CharSet.Unicode)] struct ExecuteInfo {
  public int cbSize;public uint fMask;public IntPtr hwnd;
  [MarshalAs(UnmanagedType.LPWStr)]public string verb;
  [MarshalAs(UnmanagedType.LPWStr)]public string file;
  [MarshalAs(UnmanagedType.LPWStr)]public string parameters;
  [MarshalAs(UnmanagedType.LPWStr)]public string directory;
  public int show;public IntPtr instance;public IntPtr idList;
  [MarshalAs(UnmanagedType.LPWStr)]public string className;
  public IntPtr classKey;public uint hotKey;public IntPtr icon;public IntPtr process;
 }
 [DllImport("shell32.dll",EntryPoint="ShellExecuteExW",CharSet=CharSet.Unicode,ExactSpelling=true,SetLastError=true)]static extern bool Execute(ref ExecuteInfo info);
 [DllImport("kernel32.dll")]static extern uint WaitForSingleObject(IntPtr handle,uint milliseconds);
 [DllImport("kernel32.dll")]static extern bool GetExitCodeProcess(IntPtr handle,out uint code);
 [DllImport("kernel32.dll")]static extern bool CloseHandle(IntPtr handle);
 public static int Run(string file,string arguments,out uint exitCode){
  exitCode=259;var info=new ExecuteInfo{cbSize=Marshal.SizeOf(typeof(ExecuteInfo)),fMask=0x40|0x100,verb="runas",file=file,parameters=arguments,show=0};
  if(!Execute(ref info))return Marshal.GetLastWin32Error();
  if(info.process==IntPtr.Zero)return 0;
  try{if(WaitForSingleObject(info.process,600000)!=0)return 258;if(!GetExitCodeProcess(info.process,out exitCode))return 1;return 0;}finally{CloseHandle(info.process);}
 }
}
'@
  $childExit=[uint32]259;$launchNativeError=[AgbrioPrivilegeShell]::Run($powershell,$arguments,[ref]$childExit)
  if($launchNativeError -eq 1223){throw 'SHARED_UAC_CANCELLED'}
  if($launchNativeError -eq 258){throw 'SHARED_PRIVILEGE_UNCONFIRMED'}
  if($launchNativeError -ne 0){throw 'SHARED_PRIVILEGE_LAUNCH_FAILED'}
  if($childExit -ne 0 -and !(Test-Path -LiteralPath (Join-Path (Split-Path -Parent $requestPath) 'result.json'))){throw 'SHARED_PRIVILEGE_UNCONFIRMED'}
 }elseif(!$administrator){
  Start-Process -FilePath $powershell -ArgumentList $arguments -WindowStyle Hidden | Out-Null
 }else{
  # Use the actual desktop Explorer automation object, not a newly elevated shell object.
  $windows=(New-Object -ComObject Shell.Application).Windows()
  $location=[object]0;$root=[object]0;$handle=[int]0
  $desktop=$windows.FindWindowSW([ref]$location,[ref]$root,8,[ref]$handle,1)
  if(!$desktop -or !$desktop.Document.Application){throw 'SHARED_PRIVILEGE_STANDARD_LAUNCH_UNAVAILABLE'}
  $desktop.Document.Application.ShellExecute($powershell,$arguments,$PSScriptRoot,'open',0)
 }
 @{requested=$true}|ConvertTo-Json -Compress
}catch{
 $failure=$_;$native=$_.Exception.GetBaseException();$errorCode=if($launchNativeError){$launchNativeError}else{$native.NativeErrorCode}
 if($Action -eq 'Start' -and $request -and $request.ownerSid -ceq $identity.User.Value){
  @{stage=$stage;exceptionType=$_.Exception.GetType().FullName;baseType=$native.GetType().FullName;nativeErrorCode=$errorCode;hResult=$native.HResult;errorId=$failure.FullyQualifiedErrorId} | ConvertTo-Json -Compress | Set-Content -LiteralPath (Join-Path (Split-Path -Parent $requestPath) 'launch-error.json') -Encoding UTF8
 }
 $message=if($errorCode -eq 1223 -or $_.Exception.InnerException.NativeErrorCode -eq 1223){'SHARED_UAC_CANCELLED'}elseif($_.Exception.Message -match '^SHARED_[A-Z_]+$'){$_.Exception.Message}else{'SHARED_PRIVILEGE_LAUNCH_FAILED'}
 if($Action -eq 'Child' -and $request -and $request.ownerSid -ceq $identity.User.Value){
  $receipt=Join-Path (Split-Path -Parent $requestPath) 'result.json'
  if(!(Test-Path -LiteralPath $receipt)){try{$temporary=$receipt+'.error.tmp';@{id=$JobId;ownerSid=$identity.User.Value;mode=$Mode;ok=$false;errorCode=$message}|ConvertTo-Json -Compress|Set-Content -LiteralPath $temporary -Encoding UTF8;Move-Item -LiteralPath $temporary -Destination $receipt}catch{}}
  try{$completed=[IO.File]::Open((Join-Path (Split-Path -Parent $requestPath) 'released'),[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None);$completed.Dispose()}catch{}
 }
 [Console]::Error.Write($message);exit 1
}
