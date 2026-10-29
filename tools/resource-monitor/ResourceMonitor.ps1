#requires -Version 5.1
[CmdletBinding()]
param([ValidateSet('Read','Apply','Capture','SelfTest')][string]$Mode='Read',[string]$RequestFile='',[string]$DataRoot='')
$ErrorActionPreference='Stop'
$desktop=[string](Get-ItemProperty -LiteralPath 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Shell Folders' -Name Desktop).Desktop
$expected=[IO.Path]::GetFullPath((Join-Path $desktop '工具\内存保护'))
if(!$DataRoot){$DataRoot=$expected}
$DataRoot=[IO.Path]::GetFullPath($DataRoot)
if($DataRoot.TrimEnd('\') -ine $expected.TrimEnd('\')){throw 'RESOURCE_MONITOR_ROOT_INVALID'}
$sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$collector='LocalResourceAlert';$taskName='LocalResourceAlert-Collect'
$marker='AgbrioResourceMonitor/v2 observe-only; '+$DataRoot+'; '+$sid
$legacyMarker='LocalResourceAlert/v1 observe-only; '+(Join-Path $DataRoot 'MemoryAlert.ps1')
$utf8=New-Object Text.UTF8Encoding($false)
[Console]::OutputEncoding=$utf8
function NoLinks([string]$Path){$cursor=[IO.Path]::GetFullPath($Path);while($cursor){if(Test-Path -LiteralPath $cursor){$item=Get-Item -LiteralPath $cursor -Force;if($item.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'RESOURCE_MONITOR_LINK_NOT_ALLOWED'}};$parent=[IO.Path]::GetDirectoryName($cursor);if($parent -eq $cursor){break};$cursor=$parent}}
NoLinks $DataRoot
function Atomic($Path,$Value){$temp=$Path+'.'+[guid]::NewGuid().ToString('N')+'.tmp';try{[IO.File]::WriteAllText($temp,($Value|ConvertTo-Json -Depth 12),$utf8);if(Test-Path -LiteralPath $Path){[IO.File]::Replace($temp,$Path,[System.Management.Automation.Language.NullString]::Value)}else{[IO.File]::Move($temp,$Path)}}finally{if(Test-Path -LiteralPath $temp){Remove-Item -LiteralPath $temp}}}
function Logman([string[]]$Arguments){$text=& "$env:SystemRoot\System32\logman.exe" @Arguments 2>&1;if($LASTEXITCODE -ne 0){throw 'RESOURCE_MONITOR_COUNTER_CONFIGURATION_FAILED'};$null=$text}
function Existing {
 $set=$null;try{$set=New-Object -ComObject Pla.DataCollectorSet;$set.Query($collector,$null)}catch{$set=$null}
 if($set -and $set.Description -cne $marker -and $set.Description -cne $legacyMarker){throw 'RESOURCE_MONITOR_FOREIGN_COLLECTOR'}
 $task=Get-ScheduledTask -TaskName $taskName -TaskPath '\' -ErrorAction SilentlyContinue
 if($task -and $task.Description -cne $marker -and $task.Description -cne $legacyMarker){throw 'RESOURCE_MONITOR_FOREIGN_TASK'}
 if($task){$principal=$task.Principal.UserId;try{$principal=([Security.Principal.NTAccount]$principal).Translate([Security.Principal.SecurityIdentifier]).Value}catch{};if($principal -ne $sid){throw 'RESOURCE_MONITOR_WRONG_USER'}}
 return @{set=$set;task=$task}
}
function Volumes {
 @(Get-CimInstance Win32_LogicalDisk -Filter 'DriveType=3'|Where-Object{$_.DeviceID -match '^[A-Z]:$' -and $_.Size -gt 0}|Sort-Object DeviceID|ForEach-Object{[pscustomobject]@{drive=$_.DeviceID;label=[string]$_.VolumeName;totalBytes=[double]$_.Size;freeBytes=[double]$_.FreeSpace;freePercent=[math]::Round(100*[double]$_.FreeSpace/[double]$_.Size,2)}})
}
function Read-MonitorConfig($Drives){
 $path=Join-Path $DataRoot 'settings.json';$old=if(Test-Path -LiteralPath $path){Get-Content -LiteralPath $path -Raw|ConvertFrom-Json}else{$null}
 if($old -and $old.SchemaVersion -eq 2){$known=@($old.Disks|ForEach-Object{$_.Drive});$old.Disks=@($old.Disks)+@($Drives|Where-Object{$_.drive -notin $known}|ForEach-Object{[pscustomobject]@{Drive=$_.drive;Enabled=$false;FreePercent=10.0}});return $old}
 $disks=@($Drives|ForEach-Object{$pct=10.0;$enabled=(!$old);if($old -and $_.drive -eq 'D:'){$enabled=$true;$pct=[math]::Max(0.1,[math]::Min(95,[math]::Round(100*[double]$old.DiskFreeGiB*1GB/$_.totalBytes,1)))};[pscustomobject]@{Drive=$_.drive;Enabled=$enabled;FreePercent=$pct}})
 [pscustomobject]@{SchemaVersion=2;Enabled=$true;SampleSeconds=if($old){[int]$old.SampleSeconds}else{120};AvailableMemoryPercent=if($old){[double]$old.AvailableMemoryPercent}else{10};CommitPercent=if($old){[double]$old.CommitPercent}else{90};RepeatMinutes=if($old){[int]$old.RepeatMinutes}else{30};Disks=$disks;LegacyDiskFreeGiB=if($old){[double]$old.DiskFreeGiB}else{$null}}
}
function Validate($c,$Drives){
 if($c.SchemaVersion -ne 2 -or $c.Enabled -isnot [bool] -or $c.AvailableMemoryPercent -lt 1 -or $c.AvailableMemoryPercent -gt 95 -or $c.CommitPercent -le 0 -or $c.CommitPercent -ge 100 -or $c.SampleSeconds -lt 60 -or $c.SampleSeconds -gt 3600 -or $c.RepeatMinutes -lt 5 -or $c.RepeatMinutes -gt 1440 -or @($c.Disks).Count -gt 26){throw 'RESOURCE_MONITOR_SETTINGS_INVALID'}
 $seen=@{};foreach($d in @($c.Disks)){if($d.Drive -notmatch '^[A-Z]:$' -or $d.Enabled -isnot [bool] -or $d.FreePercent -lt 0.1 -or $d.FreePercent -gt 95 -or $seen.ContainsKey($d.Drive)){throw 'RESOURCE_MONITOR_SETTINGS_INVALID'};$seen[$d.Drive]=$true;if($d.Enabled -and !($Drives|Where-Object drive -CEQ $d.Drive)){throw 'RESOURCE_MONITOR_DRIVE_UNAVAILABLE'}}
}
function Metrics($c){
 $m=Get-CimInstance Win32_PerfFormattedData_PerfOS_Memory;$system=Get-CimInstance Win32_ComputerSystem
 $disks=@(Volumes|Where-Object{$drive=$_;@($c.Disks|Where-Object{$_.Drive -ceq $drive.drive -and $_.Enabled}).Count -gt 0}|ForEach-Object{$d=$_;$threshold=@($c.Disks|Where-Object Drive -CEQ $d.drive)[0];[ordered]@{drive=$d.drive;totalBytes=$d.totalBytes;freeBytes=$d.freeBytes;freePercent=$d.freePercent;thresholdPercent=[double]$threshold.FreePercent}})
 $d=@($disks|Where-Object drive -CEQ 'D:')
 [ordered]@{availableBytes=[double]$m.AvailableBytes;physicalBytes=[double]$system.TotalPhysicalMemory;committedBytes=[double]$m.CommittedBytes;commitLimitBytes=[double]$m.CommitLimit;commitPercent=[math]::Round(100*[double]$m.CommittedBytes/[double]$m.CommitLimit,3);commitHeadroomBytes=[double]$m.CommitLimit-[double]$m.CommittedBytes;diskDFreeBytes=if($d.Count){$d[0].freeBytes}else{$null};disks=$disks}
}
function Reasons($m,$c){$result=@();if($m.availableBytes -lt $m.physicalBytes*$c.AvailableMemoryPercent/100){$result+='physical_memory_low'};if($m.commitPercent -gt $c.CommitPercent){$result+='commit_high'};foreach($d in @($m.disks)){if($d.freePercent -lt $d.thresholdPercent){$result+=('disk_low:'+$d.drive)}};return $result}
function Capture {
 try{$lock=[IO.File]::Open((Join-Path $DataRoot 'capture.lock'),'OpenOrCreate','ReadWrite','None')}catch [IO.IOException]{return}
 try{$c=Read-MonitorConfig @();if(!$c.Enabled){return};$m=Metrics $c;$conditions=@(Reasons $m $c);if(!$conditions.Count){return}
  $critical=$m.commitPercent -ge 95 -or $m.availableBytes -le $m.physicalBytes*[math]::Min(5,$c.AvailableMemoryPercent/2)/100
  foreach($disk in @($m.disks)){if($disk.freePercent -le [math]::Min(2,$disk.thresholdPercent/2)){$critical=$true}}
  $severity=if($critical){'critical'}else{'warning'};$now=[DateTimeOffset]::UtcNow;$statePath=Join-Path $DataRoot 'state.json';$state=if(Test-Path -LiteralPath $statePath){Get-Content -LiteralPath $statePath -Raw|ConvertFrom-Json}else{$null};$fingerprint=$severity+':'+(($conditions|Sort-Object)-join ',')
  if($state -and $state.fingerprint -ceq $fingerprint -and $now.ToUnixTimeSeconds()-[double]$state.lastCreatedEpochSeconds -ge 0 -and $now.ToUnixTimeSeconds()-[double]$state.lastCreatedEpochSeconds -lt $c.RepeatMinutes*60){return}
  $top=@(Get-CimInstance Win32_PerfFormattedData_PerfProc_Process|Where-Object{$_.Name -notin @('_Total','Idle') -and $_.IDProcess -gt 0}|Sort-Object PrivateBytes -Descending|Select-Object -First 10|ForEach-Object{[ordered]@{name=$_.Name;pid=[int]$_.IDProcess;privateBytes=[double]$_.PrivateBytes;workingSetBytes=[double]$_.WorkingSet;creationUtc=$null;parentPid=$null;identityNote='Snapshot only; no process action authorized.'}})
  $id=[guid]::NewGuid().ToString();$event=[ordered]@{schemaVersion=2;eventId=$id;eventType='local_resource_pressure';observedUtc=$now.ToString('o');host=$env:COMPUTERNAME;isTest=$false;severity=$severity;conditions=$conditions;metrics=$m;topProcesses=$top;thresholds=$c;policy=@{observeOnly=$true;processActionsAllowed=$false;targetConversationId=$null;deliveryStatus='pending_agbrio_integration'}}
  $outbox=Join-Path $DataRoot 'outbox';$null=New-Item -ItemType Directory -Path $outbox -Force
  if(@(Get-ChildItem -LiteralPath $outbox -Filter '*.json' -File).Count -ge 64){Atomic (Join-Path $DataRoot 'overflow-latest.json') $event}else{Atomic (Join-Path $outbox ($now.ToString('yyyyMMddTHHmmssfffZ')+'_'+$id+'.json')) $event}
  Atomic $statePath @{lastCreatedEpochSeconds=$now.ToUnixTimeSeconds();fingerprint=$fingerprint;lastEventId=$id}
 }finally{$lock.Dispose()}
}
function Read {
 $drives=@(Volumes);$config=Read-MonitorConfig $drives;$current=Existing;$running=$current.set -and $current.set.Status -eq 1;$m=Metrics $config
 $parent=Get-CimInstance Win32_Process -Filter ("ProcessId="+(Get-CimInstance Win32_Process -Filter "ProcessId=$PID").ParentProcessId)
 [ordered]@{config=$config;drives=$drives;availableBytes=$m.availableBytes;physicalBytes=$m.physicalBytes;running=[bool]$running;installed=[bool]$current.set;legacy=($current.set -and $current.set.Description -ceq $legacyMarker);sampleSeconds=$config.SampleSeconds;startupEnabled=$false;ownerSid=$sid;callerPid=$parent.ProcessId;callerImage=$parent.ExecutablePath;callerCreatedUtc=$parent.CreationDate.ToUniversalTime().ToString('o')}|ConvertTo-Json -Depth 12
}
function Apply {
 $identity=[Security.Principal.WindowsIdentity]::GetCurrent();$principal=New-Object Security.Principal.WindowsPrincipal($identity);if(!$principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){throw 'RESOURCE_MONITOR_ADMIN_REQUIRED'}
 if(!$RequestFile -or !(Test-Path -LiteralPath $RequestFile)){throw 'RESOURCE_MONITOR_REQUEST_REQUIRED'}
 $requests=[IO.Path]::GetFullPath((Join-Path $DataRoot 'requests'))+'\';$requestPath=[IO.Path]::GetFullPath($RequestFile);if(!$requestPath.StartsWith($requests,[StringComparison]::OrdinalIgnoreCase) -or (Get-Item -LiteralPath $requestPath).Length -gt 16384){throw 'RESOURCE_MONITOR_REQUEST_INVALID'}
 NoLinks $requestPath;NoLinks (Join-Path $DataRoot 'ResourceCapture.ps1');NoLinks (Join-Path $DataRoot 'settings.json');NoLinks (Join-Path $DataRoot 'capture.lock')
 $request=Get-Content -LiteralPath $requestPath -Raw|ConvertFrom-Json;if($request.ownerSid -cne $sid){throw 'RESOURCE_MONITOR_WRONG_USER'}
 $requestGuid=[guid]::Empty;if([IO.Path]::GetFileNameWithoutExtension($requestPath) -cne $request.requestId -or ![guid]::TryParse($request.requestId,[ref]$requestGuid)){throw 'RESOURCE_MONITOR_REQUEST_INVALID'}
 if([math]::Abs(([DateTimeOffset]::UtcNow-([DateTimeOffset]$request.createdUtc)).TotalSeconds) -gt 120){throw 'RESOURCE_MONITOR_REQUEST_EXPIRED'}
 $caller=Get-CimInstance Win32_Process -Filter ("ProcessId="+[int]$request.callerPid)
 if(!$caller -or $caller.ExecutablePath -ine $request.callerImage -or $caller.CreationDate.ToUniversalTime().Ticks -ne ([DateTime]$request.callerCreatedUtc).ToUniversalTime().Ticks){throw 'RESOURCE_MONITOR_CALLER_CHANGED'}
 $c=$request.config;$drives=@(Volumes);Validate $c $drives;$current=Existing;if($current.task -and $current.task.State -eq 'Running'){throw 'RESOURCE_MONITOR_CAPTURE_BUSY'}
 $null=New-Item -ItemType Directory -Path $DataRoot -Force;$lock=[IO.File]::Open((Join-Path $DataRoot 'capture.lock'),'OpenOrCreate','ReadWrite','None')
 $settingsPath=Join-Path $DataRoot 'settings.json';$oldBytes=if(Test-Path -LiteralPath $settingsPath){[IO.File]::ReadAllBytes($settingsPath)}else{$null};$oldSet=if($current.set){$current.set.Xml}else{$null};$oldTask=if($current.task){Export-ScheduledTask -TaskName $taskName -TaskPath '\'}else{$null};$wasRunning=$current.set -and $current.set.Status -eq 1
 $mirror=Join-Path $DataRoot 'ResourceCapture.ps1';$oldMirror=if(Test-Path -LiteralPath $mirror){[IO.File]::ReadAllBytes($mirror)}else{$null}
 $createdNewSet=$false
 try{
  if($wasRunning){Logman @('stop',$collector)}
  if($current.set){Logman @('delete',$collector)}
  $mirror=Join-Path $DataRoot 'ResourceCapture.ps1';$temp=$mirror+'.new';if([IO.Path]::GetFullPath($PSCommandPath) -ine [IO.Path]::GetFullPath($mirror)){[IO.File]::Copy($PSCommandPath,$temp,$true);if(Test-Path -LiteralPath $mirror){[IO.File]::Replace($temp,$mirror,[System.Management.Automation.Language.NullString]::Value)}else{[IO.File]::Move($temp,$mirror)}}
  Atomic $settingsPath $c
  $exe=Join-Path $env:ProgramFiles 'PowerShell\7\pwsh.exe';if(!(Test-Path -LiteralPath $exe)){$exe="$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe"};$action=New-ScheduledTaskAction -Execute $exe -Argument ('-NoProfile -NonInteractive -ExecutionPolicy RemoteSigned -WindowStyle Hidden -File "'+$mirror+'" -Mode Capture') -WorkingDirectory $DataRoot
  $taskPrincipal=New-ScheduledTaskPrincipal -UserId $identity.Name -LogonType Interactive -RunLevel Limited;$taskSettings=New-ScheduledTaskSettingsSet -MultipleInstances IgnoreNew -ExecutionTimeLimit (New-TimeSpan -Seconds 60) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
  $null=Register-ScheduledTask -TaskName $taskName -TaskPath '\' -Action $action -Principal $taskPrincipal -Settings $taskSettings -Description $marker -Force
  $physical=[double](Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory;$free=[math]::Floor($physical*$c.AvailableMemoryPercent/100);$interval=[TimeSpan]::FromSeconds($c.SampleSeconds).ToString('hh\:mm\:ss');$thresholds=@("\Memory\Available Bytes<$free",("\Memory\% Committed Bytes In Use>"+$c.CommitPercent))
  foreach($disk in @($c.Disks|Where-Object Enabled)){ $percent=([double]$disk.FreePercent).ToString('0.0',[Globalization.CultureInfo]::InvariantCulture);$thresholds+=('\LogicalDisk('+$disk.Drive+')\% Free Space<'+$percent) }
  $arguments=@('create','alert',$collector,'-si',$interval,'-th')+$thresholds+@('-tn',('\'+$taskName),'-o',(Join-Path $DataRoot 'native'),'--el');Logman $arguments;$createdNewSet=$true
  $set=New-Object -ComObject Pla.DataCollectorSet;$set.Query($collector,$null);$set.Description=$marker;$null=$set.Commit($collector,$null,3)
  if($c.Enabled){Logman @('start',$collector)}
  $readback=Existing;if(!$readback.set -or ($c.Enabled -and $readback.set.Status -ne 1)){throw 'RESOURCE_MONITOR_APPLY_UNCONFIRMED'}
  Atomic (Join-Path $requests ([IO.Path]::GetFileNameWithoutExtension($requestPath)+'.result.json')) @{applied=$true;ownerSid=$sid;requestId=$request.requestId;config=$c}
 }catch{
  try{if($createdNewSet){$broken=New-Object -ComObject Pla.DataCollectorSet;$broken.Query($collector,$null);if($broken.Description -ceq $marker -or !$broken.Description){if($broken.Status -eq 1){Logman @('stop',$collector)};Logman @('delete',$collector)}}}catch{}
  if($null -ne $oldBytes){[IO.File]::WriteAllBytes($settingsPath,$oldBytes)}elseif(Test-Path -LiteralPath $settingsPath){Remove-Item -LiteralPath $settingsPath}
  if($null -ne $oldMirror){[IO.File]::WriteAllBytes($mirror,$oldMirror)}elseif(Test-Path -LiteralPath $mirror){Remove-Item -LiteralPath $mirror}
  if($oldTask){$null=Register-ScheduledTask -TaskName $taskName -TaskPath '\' -Xml $oldTask -Force}elseif(Get-ScheduledTask -TaskName $taskName -TaskPath '\' -ErrorAction SilentlyContinue){Unregister-ScheduledTask -TaskName $taskName -TaskPath '\' -Confirm:$false}
  if($oldSet){$restore=New-Object -ComObject Pla.DataCollectorSet;$restore.SetXml($oldSet);$null=$restore.Commit($collector,$null,3);if($wasRunning){Logman @('start',$collector)}}
  throw
 }finally{$lock.Dispose()}
}
switch($Mode){'Read'{Read};'Apply'{Apply};'Capture'{Capture};'SelfTest'{
 $c=[pscustomobject]@{AvailableMemoryPercent=10;CommitPercent=90};$m=@{availableBytes=20;physicalBytes=100;commitPercent=50;disks=@(@{drive='C:';freePercent=8;thresholdPercent=10},@{drive='D:';freePercent=60;thresholdPercent=10})};if((@(Reasons $m $c)-join ',') -ne 'disk_low:C:'){throw 'Disk percent test failed'};$m.availableBytes=5;if((@(Reasons $m $c)-join ',') -ne 'physical_memory_low,disk_low:C:'){throw 'RAM percent test failed'};'PASS: selected disk and memory percentage predicates, no process actions'
}}
