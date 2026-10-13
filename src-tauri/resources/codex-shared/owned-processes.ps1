# Capture before graceful shutdown: after the supervisor exits, PPID traversal
# cannot recover its orphaned children. Never act on a process name alone.
function Test-SharedProcessIdentity($Expected,$Actual) {
 if(!$Expected -or !$Actual -or !$Expected.createdAt -or !$Expected.ownerSid -or !$Expected.imagePath){return $false}
 return $Expected.pid -gt 0 -and $Expected.pid -eq $Actual.pid -and $Expected.createdAt -ceq $Actual.createdAt -and $Expected.ownerSid -ceq $Actual.ownerSid -and [string]::Equals($Expected.imagePath,$Actual.imagePath,[StringComparison]::OrdinalIgnoreCase)
}
function Get-SharedProcessIdentity([uint32]$ProcessId,[string]$Directory) {
 return (& (Join-Path $Directory 'runtime/peer-service.ps1') -QueryPid $ProcessId)|ConvertFrom-Json
}
function Get-SharedProcessTable {return @(Get-CimInstance Win32_Process)}
function Stop-SharedProcess([uint32]$ProcessId) {Stop-Process -Id ([int]$ProcessId) -ErrorAction Stop}
function Assert-SharedStopRoot($Saved,[string]$Directory,[string]$OwnerSid) {
 if(!$Saved -or !$Saved.createdAt -or $Saved.pid -le 0 -or $Saved.ownerSid -cne $OwnerSid -or ![string]::Equals($Saved.imagePath,(Join-Path $Directory 'runtime/node.exe'),[StringComparison]::OrdinalIgnoreCase)){throw 'SHARED_PROCESS_IDENTITY_MISSING'}
}
function Get-SharedStopSnapshot($Saved,[string]$Directory,[string]$OwnerSid) {
 Assert-SharedStopRoot $Saved $Directory $OwnerSid
 $live=Get-SharedProcessIdentity ([uint32]$Saved.pid) $Directory
 $snapshot=@{root=$Saved;children=@();rootAbsent=(!(Test-SharedProcessIdentity $Saved $live))}
 if($snapshot.rootAbsent){return $snapshot}
 $table=Get-SharedProcessTable
 $known=@{};$known[[uint32]$Saved.pid]=$live
 do{
  $added=$false
  foreach($entry in $table){
   $id=[uint32]$entry.ProcessId;$parent=$known[[uint32]$entry.ParentProcessId]
   if(!$parent -or $known.ContainsKey($id) -or $entry.Name -ieq 'ChatGPT.exe'){continue}
   $identity=Get-SharedProcessIdentity $id $Directory
   # Reject a recycled PPID, a foreign owner, or an identity read from a later
   # process than the one in this table. Descendants of rejected entries stay out.
   if(!$identity -or $identity.ownerSid -cne $OwnerSid -or [long]$identity.createdAt -lt [long]$parent.createdAt){continue}
   # CIM DATETIME carries microseconds; GetProcessTimes carries 100ns ticks.
   # Only the table correlation permits sub-microsecond rounding. The captured
   # native identity and every stop-time identity comparison remain exact.
   if($entry.CreationDate -and [Math]::Abs(([datetime]$entry.CreationDate).ToFileTimeUtc()-[long]$identity.createdAt) -ge 10){continue}
   $known[$id]=$identity;$snapshot.children+=@($identity);$added=$true
  }
 }while($added)
 return $snapshot
}
function Invoke-SharedStopSnapshot($Snapshot,$Saved,[string]$Directory,[string]$OwnerSid) {
 Assert-SharedStopRoot $Saved $Directory $OwnerSid
 if(!(Test-SharedProcessIdentity $Saved $Snapshot.root)){throw 'SHARED_PROCESS_SNAPSHOT_INVALID'}
 # Stop the root first so it cannot spawn replacements, then the captured children.
 foreach($identity in @($Snapshot.root)+@($Snapshot.children)){
  if($identity.ownerSid -cne $OwnerSid -or [IO.Path]::GetFileName($identity.imagePath) -ieq 'ChatGPT.exe'){continue}
  $current=Get-SharedProcessIdentity ([uint32]$identity.pid) $Directory
  if(!(Test-SharedProcessIdentity $identity $current)){continue}
  try{Stop-SharedProcess ([uint32]$identity.pid)}catch{
   if(Test-SharedProcessIdentity $identity (Get-SharedProcessIdentity ([uint32]$identity.pid) $Directory)){throw 'SHARED_STOP_FAILED'}
  }
 }
 foreach($identity in @($Snapshot.root)+@($Snapshot.children)){
  if(Test-SharedProcessIdentity $identity (Get-SharedProcessIdentity ([uint32]$identity.pid) $Directory)){throw 'SHARED_STOP_FAILED'}
 }
 return @{stopped=$true}
}
