param([ValidateSet('Snapshot','Restore','Protect','Stop','CleanCertificate','DesktopWindow','ShowDesktop')][string]$Action,[string]$Directory,[string]$SnapshotPath,[string]$Url,[string]$TestRegistryPath,[uint32]$DesktopPid)
$ErrorActionPreference='Stop'
$names=@('CODEX_APP_SERVER_WS_URL','NODE_EXTRA_CA_CERTS','NODE_USE_SYSTEM_CA')
if($TestRegistryPath -and $TestRegistryPath -notmatch '^Software\\AIWorkRouter\\Tests\\[a-f0-9-]{36}$'){throw 'SHARED_TEST_REGISTRY_INVALID'}
$key=[Microsoft.Win32.Registry]::CurrentUser.CreateSubKey($(if($TestRegistryPath){$TestRegistryPath}else{'Environment'}))
function Broadcast {
 if($TestRegistryPath){return}
 Add-Type -TypeDefinition @'
using System;using System.Runtime.InteropServices;
public static class AIWREnvironmentBroadcast {
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern IntPtr SendMessageTimeout(IntPtr h,uint m,IntPtr w,string l,uint f,uint t,out IntPtr r);
}
'@
 $result=[IntPtr]::Zero
 [void][AIWREnvironmentBroadcast]::SendMessageTimeout([IntPtr]0xffff,0x1a,[IntPtr]::Zero,'Environment',2,2000,[ref]$result)
}
switch($Action){
 'ShowDesktop' {
  Add-Type -TypeDefinition @'
using System;using System.Runtime.InteropServices;
public static class AIWRDesktopRestore {
 [DllImport("user32.dll")] public static extern bool ShowWindowAsync(IntPtr h,int action);
 [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
 [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
}
'@
  $root=(Get-AppxPackage -Name 'OpenAI.Codex').InstallLocation
  $desktop=Join-Path $root 'app/ChatGPT.exe'
  $owner=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
  $visible=$false
  foreach($entry in @(Get-CimInstance Win32_Process -Filter "Name='ChatGPT.exe'")){
   if(![string]::Equals($entry.ExecutablePath,$desktop,[StringComparison]::OrdinalIgnoreCase)){continue}
   if((Invoke-CimMethod -InputObject $entry -MethodName GetOwnerSid).Sid -cne $owner){continue}
   $target=Get-Process -Id $entry.ProcessId -ErrorAction SilentlyContinue
   if(!$target -or $target.MainWindowHandle -eq [IntPtr]::Zero){continue}
   [void][AIWRDesktopRestore]::ShowWindowAsync($target.MainWindowHandle,9)
   [void][AIWRDesktopRestore]::SetForegroundWindow($target.MainWindowHandle)
   $visible=[AIWRDesktopRestore]::IsWindowVisible($target.MainWindowHandle)
   if($visible){break}
  }
  @{visible=$visible}|ConvertTo-Json -Compress
 }
 'DesktopWindow' {
  Add-Type -TypeDefinition @'
using System;using System.Runtime.InteropServices;
public static class AIWRDesktopVisibility { [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window); }
'@
  $target=Get-Process -Id $DesktopPid -ErrorAction SilentlyContinue
  @{visible=($null -ne $target -and $target.MainWindowHandle -ne [IntPtr]::Zero -and [AIWRDesktopVisibility]::IsWindowVisible($target.MainWindowHandle))}|ConvertTo-Json -Compress
 }
 'Snapshot' {
  $values=@{};foreach($name in $names){$values[$name]=$key.GetValue($name,$null,[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)}
  $values|ConvertTo-Json -Compress
 }
 'Restore' {
  $snapshot=Get-Content -LiteralPath $SnapshotPath -Raw|ConvertFrom-Json
  foreach($change in $snapshot.changes){
   $current=$key.GetValue($change.name,$null,[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
   # Compare-and-restore: later owner/unrelated configuration always wins.
   if($current -ceq $change.installed){
    if($null -eq $change.previous){$key.DeleteValue($change.name,$false)}else{$key.SetValue($change.name,[string]$change.previous,[Microsoft.Win32.RegistryValueKind]::String)}
   }
  }
  Broadcast;'{"ok":true}'
 }
 'Protect' {
  $resolved=[IO.Path]::GetFullPath($Directory)
  [IO.Directory]::CreateDirectory($resolved)|Out-Null
  $acl=[Security.AccessControl.DirectorySecurity]::new();$acl.SetAccessRuleProtection($true,$false)
  foreach($sid in @([Security.Principal.WindowsIdentity]::GetCurrent().User.Value,'S-1-5-18','S-1-5-32-544')){
   $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new($sid),[Security.AccessControl.FileSystemRights]::FullControl,([Security.AccessControl.InheritanceFlags]::ContainerInherit -bor [Security.AccessControl.InheritanceFlags]::ObjectInherit),[Security.AccessControl.PropagationFlags]::None,[Security.AccessControl.AccessControlType]::Allow))
  }
  [IO.Directory]::SetAccessControl($resolved,$acl);'{"ok":true}'
 }
 'Stop' {
  $readyPath=Join-Path $Directory 'ready.json';if(!(Test-Path -LiteralPath $readyPath)){Write-Output '{"stopped":true,"absent":true}';break}
  $ready=Get-Content -LiteralPath $readyPath -Raw|ConvertFrom-Json
  $saved=$ready.supervisorIdentity
  if(!$saved){throw 'SHARED_PROCESS_IDENTITY_MISSING'}
  $liveJson=& (Join-Path $Directory 'runtime/peer-service.ps1') -QueryPid ([uint32]$saved.pid)
  $live=$liveJson|ConvertFrom-Json
  if(!$live){Write-Output '{"stopped":true,"absent":true}';break}
  $expected=Join-Path $Directory 'runtime/node.exe'
  if($live.pid -ne $saved.pid -or $live.createdAt -cne $saved.createdAt -or $live.ownerSid -cne $saved.ownerSid -or $live.ownerSid -cne [Security.Principal.WindowsIdentity]::GetCurrent().User.Value -or ![string]::Equals($live.imagePath,$expected,[StringComparison]::OrdinalIgnoreCase)){
   # The saved process has exited and its PID can be recycled. Do not kill the
   # new occupant, or turn a proven-absent old supervisor into a recovery loop.
   Write-Output '{"stopped":true,"absent":true,"identityRecycled":true}';break
  }
  # Capture children and their start-time identities before stopping this proven root.
  $children=@(Get-CimInstance Win32_Process -Filter ('ParentProcessId='+$saved.pid))
  $identities=@()
  foreach($child in $children){
   if(($child.Name -eq 'codex.exe' -and $child.CommandLine -match '\bapp-server\b') -or ($child.Name -eq 'powershell.exe' -and $child.CommandLine.Contains((Join-Path $Directory 'runtime/peer-service.ps1')))){
    $identity=(& (Join-Path $Directory 'runtime/peer-service.ps1') -QueryPid ([uint32]$child.ProcessId))|ConvertFrom-Json
    if($identity.ownerSid -ceq $saved.ownerSid){$identities+=@{pid=$child.ProcessId;createdAt=$identity.createdAt;imagePath=$identity.imagePath}}
   }
  }
  Stop-Process -Id ([int]$saved.pid) -ErrorAction Stop
  foreach($identity in $identities){
   $current=(& (Join-Path $Directory 'runtime/peer-service.ps1') -QueryPid ([uint32]$identity.pid))|ConvertFrom-Json
   if($current -and $current.createdAt -ceq $identity.createdAt -and $current.imagePath -ceq $identity.imagePath){Stop-Process -Id ([int]$identity.pid) -ErrorAction Stop}
  }
  '{"stopped":true}'
 }
 'CleanCertificate' {
  $pem=Join-Path $Directory 'server.pem'
  if(Test-Path -LiteralPath $pem){
   $text=[IO.File]::ReadAllText($pem)
   $der=[Convert]::FromBase64String(($text -replace '-----BEGIN CERTIFICATE-----|-----END CERTIFICATE-----|\s',''))
   $leaf=[Security.Cryptography.X509Certificates.X509Certificate2]::new($der)
   # Exact saved DER thumbprint only, never delete all certificates by subject.
   if($leaf.Subject -ceq 'CN=AI Work Router local Codex'){
    $store=[Security.Cryptography.X509Certificates.X509Store]::new('Root','CurrentUser')
    try{$store.Open([Security.Cryptography.X509Certificates.OpenFlags]::ReadWrite);foreach($cert in $store.Certificates.Find([Security.Cryptography.X509Certificates.X509FindType]::FindByThumbprint,$leaf.Thumbprint,$false)){$store.Remove($cert)}}finally{$store.Close()}
   }
   $leaf.Dispose()
  }
  '{"ok":true}'
 }
}
