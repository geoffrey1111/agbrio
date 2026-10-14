# Owner-invoked recovery for an unreachable, identity-proven shared supervisor.
# No process-name kill, global endpoint change, Desktop stop or automatic replay.
function Get-AgbrioUnavailableSnapshot([string]$Directory,[string]$Scripts) {
 if((Get-Item -LiteralPath $Directory).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'SHARED_RECOVERY_LINK_REJECTED'}
 $config=Get-Content -LiteralPath (Join-Path $Directory 'runtime-config.json') -Raw|ConvertFrom-Json
 $ready=Get-Content -LiteralPath (Join-Path $Directory 'ready.json') -Raw|ConvertFrom-Json
 $owner=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
 if($config.format -ne 2 -or !$config.instance -or $ready.instance -cne $config.instance -or $config.ownerSid -cne $owner -or $config.port -lt 1 -or $config.port -gt 65535){throw 'SHARED_RECOVERY_IDENTITY_REJECTED'}
 $healthy=$null
 try{$healthy=Invoke-RestMethod -Uri ('http://127.0.0.1:'+([int]$config.port)+'/health') -TimeoutSec 2 -ErrorAction Stop}catch{}
 # If service state is available, keep the normal idle/busy stop boundary.
 if($healthy -and $healthy.instance -ceq $config.instance){throw 'SHARED_RECOVERY_USE_NORMAL_STOP'}
 . (Join-Path $Scripts 'owned-processes.ps1')
 $snapshot=Get-SharedStopSnapshot $ready.supervisorIdentity $Directory $owner
 if($snapshot.rootAbsent){throw 'SHARED_RECOVERY_ROOT_ABSENT'}
 return @{instance=$config.instance;snapshot=$snapshot}
}
function Stop-AgbrioUnavailableOwner([string]$Directory,[string]$Scripts,$Preview,[switch]$OwnerConfirmed) {
 if(!$OwnerConfirmed){throw 'SHARED_RECOVERY_OWNER_CONFIRMATION_REQUIRED'}
 # Recheck health and identity after the owner responds; a recovered/busy runtime
 # or replacement supervisor must never borrow an earlier confirmation.
 $fresh=Get-AgbrioUnavailableSnapshot $Directory $Scripts
 . (Join-Path $Scripts 'owned-processes.ps1')
 if($fresh.instance -cne $Preview.instance -or !(Test-SharedProcessIdentity $Preview.snapshot.root $fresh.snapshot.root)){throw 'SHARED_RECOVERY_IDENTITY_CHANGED'}
 $owner=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
 $snapshots=@($Preview.snapshot,$fresh.snapshot)
 $path=Join-Path $Directory 'stop-processes.json'
 foreach($target in @($path,$path+'.pending')){if((Test-Path -LiteralPath $target) -and ((Get-Item -LiteralPath $target).PSIsContainer -or ((Get-Item -LiteralPath $target).Attributes -band [IO.FileAttributes]::ReparsePoint))){throw 'SHARED_RECOVERY_LINK_REJECTED'}}
 if(Test-Path -LiteralPath $path){
  $previous=Get-Content -LiteralPath $path -Raw|ConvertFrom-Json
  if(Test-SharedProcessIdentity $fresh.snapshot.root $previous.root){$snapshots+=@($previous)}
 }
 $children=@{};foreach($record in $snapshots){foreach($child in $record.children){$key=[string]$child.pid+':'+$child.createdAt+':'+$child.ownerSid+':'+$child.imagePath;$children[$key]=$child}}
 $snapshot=@{root=$fresh.snapshot.root;children=@($children.Values);rootAbsent=$false}
 # Keep the exact capture across an interrupted recovery. Never mark disabled
 # here: the normal lifecycle must verify cleanup and commit its own state.
 $pending=$path+'.pending'
 [IO.File]::WriteAllText($pending,($snapshot|ConvertTo-Json -Depth 8),[Text.UTF8Encoding]::new($false))
 Move-Item -LiteralPath $pending -Destination $path -Force -ErrorAction Stop
 Invoke-SharedStopSnapshot $snapshot $snapshot.root $Directory $owner
}
