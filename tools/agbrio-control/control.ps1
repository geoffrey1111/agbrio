[CmdletBinding()]
param(
 [ValidateSet('Status','Capabilities','Diagnostics','Start','Stop','Restart','PublishBoard')][string]$Command='Status',
 [string]$Folder='',
 [string]$Executable=(Join-Path $PSScriptRoot '..\ai-work-router.exe'),
 [string]$ExpectedInstance='',
 [guid]$RequestId=[guid]::NewGuid()
)
$ErrorActionPreference='Stop'
$sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$hash=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($sid))).ToLowerInvariant()
$pipeName='agbrio-control-v1-'+$hash
function Invoke-Control([string]$Operation,[string]$Expected='',[guid]$Identity=[guid]::NewGuid()) {
 $pipe=[IO.Pipes.NamedPipeClientStream]::new('.', $pipeName, [IO.Pipes.PipeDirection]::InOut,[IO.Pipes.PipeOptions]::Asynchronous,[Security.Principal.TokenImpersonationLevel]::Identification)
 try {
  $pipe.Connect(2000)
  $payload=@{apiVersion=1;requestId=$Identity.ToString();command=$Operation;expectedInstance=if($Expected){$Expected}else{$null}}
  if($Operation -eq 'PUBLISH_BOARD'){$payload.folder=$Folder}
  $bytes=[Text.Encoding]::UTF8.GetBytes(($payload | ConvertTo-Json -Compress)+"`n")
  $write=$pipe.WriteAsync($bytes,0,$bytes.Length);if(!$write.Wait(3000)){throw 'APP_CONTROL_WRITE_UNCONFIRMED'};$write.GetAwaiter().GetResult()
  $buffer=[byte[]]::new(8192);$output=[IO.MemoryStream]::new();$deadline=[DateTime]::UtcNow.AddSeconds(5)
  try {
   while($true){
    $remaining=[int][Math]::Max(1,($deadline-[DateTime]::UtcNow).TotalMilliseconds)
    $read=$pipe.ReadAsync($buffer,0,$buffer.Length);if(!$read.Wait($remaining)){throw 'APP_CONTROL_RESPONSE_UNCONFIRMED'};$count=$read.GetAwaiter().GetResult()
    if($count -eq 0){throw 'APP_CONTROL_RESPONSE_UNCONFIRMED'}
    $end=[Array]::IndexOf($buffer,[byte]10,0,$count);$length=if($end -ge 0){$end}else{$count}
    if($output.Length+$length -gt 131072){throw 'APP_CONTROL_RESPONSE_TOO_LARGE'};$output.Write($buffer,0,$length)
    if($end -ge 0){break};if([DateTime]::UtcNow -ge $deadline){throw 'APP_CONTROL_RESPONSE_UNCONFIRMED'}
   }
   $reply=[Text.Encoding]::UTF8.GetString($output.ToArray()) | ConvertFrom-Json
   if($reply.errorCode){throw [string]$reply.errorCode}
   if($reply.apiVersion -ne 1 -or $reply.requestId -ne $Identity.ToString()){throw 'APP_CONTROL_RESPONSE_INVALID'}
   $ack=[Text.Encoding]::ASCII.GetBytes("ACK`n");$sent=$pipe.WriteAsync($ack,0,$ack.Length)
   if(!$sent.Wait(1000)){throw 'APP_CONTROL_RESPONSE_UNCONFIRMED'};$sent.GetAwaiter().GetResult()
   # Wait for this pipe instance to close before opening another. No command is
   # retried if completion of this exchange is uncertain.
   $closed=$pipe.ReadAsync($buffer,0,1)
   if(!$closed.Wait(1000)){throw 'APP_CONTROL_RESPONSE_UNCONFIRMED'}
   if($closed.GetAwaiter().GetResult() -ne 0){throw 'APP_CONTROL_RESPONSE_INVALID'}
   return $reply.data
  } finally {$output.Dispose()}
 } finally {$pipe.Dispose()}
}
function Exact-Executable {
 $resolved=(Resolve-Path -LiteralPath $Executable).Path
 if([IO.Path]::GetFileName($resolved) -ne 'ai-work-router.exe'){throw 'APP_CONTROL_EXECUTABLE_INVALID'}
 $item=Get-Item -LiteralPath $resolved
 if($item.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'APP_CONTROL_EXECUTABLE_INVALID'}
 return $resolved
}
function Exact-Owner($Status,[string]$Path) {
 $process=Get-CimInstance Win32_Process -Filter ('ProcessId='+[int]$Status.pid)
 if(!$process -or $process.ExecutablePath -ine $Path){throw 'APP_CONTROL_EXECUTABLE_MISMATCH'}
 return $process
}
function Start-Host([string]$Path) {
 $existing=@(Get-CimInstance Win32_Process -Filter "Name='ai-work-router.exe'" | Where-Object {$_.ExecutablePath -ieq $Path})
 if($existing.Count){$state=Invoke-Control 'STATUS';$null=Exact-Owner $state $Path;return $state}
 $launched=Start-Process -FilePath $Path -WindowStyle Hidden -PassThru
 $deadline=[DateTime]::UtcNow.AddSeconds(30)
 while([DateTime]::UtcNow -lt $deadline){
  try {$state=Invoke-Control 'STATUS';$null=Exact-Owner $state $Path;return $state} catch {
   if($_.Exception.Message -eq 'APP_CONTROL_EXECUTABLE_MISMATCH'){throw}
   if($launched.HasExited){throw 'APP_CONTROL_START_FAILED'}
   Start-Sleep -Milliseconds 250
  }
 }
 throw 'APP_CONTROL_START_UNCONFIRMED'
}
function Stop-Host([string]$Path) {
 $instanceValue=[guid]::Empty
 if(!$ExpectedInstance -or ![guid]::TryParse($ExpectedInstance,[ref]$instanceValue)){throw 'APP_CONTROL_EXPECTED_INSTANCE_REQUIRED'}
 $state=Invoke-Control 'STATUS'
 if($state.instanceId -ne $ExpectedInstance){throw 'APP_CONTROL_INSTANCE_CHANGED'}
 $owner=Exact-Owner $state $Path
 # One write only. Even if its response is lost, inspect the old process; never
 # resend or kill it, and never continue restart until its exit is proven.
 $uncertain=$false
 try {$null=Invoke-Control 'SHUTDOWN' $ExpectedInstance $RequestId} catch {$uncertain=$true}
 $deadline=[DateTime]::UtcNow.AddSeconds(30)
 while([DateTime]::UtcNow -lt $deadline){
  $current=Get-CimInstance Win32_Process -Filter ('ProcessId='+$owner.ProcessId)
  if(!$current -or $current.CreationDate -ne $owner.CreationDate){return @{state='STOPPED';instanceId=$ExpectedInstance;pid=$owner.ProcessId;receiptUncertain=$uncertain}}
  Start-Sleep -Milliseconds 250
 }
 throw 'APP_CONTROL_SHUTDOWN_UNCONFIRMED'
}
try {
 $result=switch($Command){
  'Status' {Invoke-Control 'STATUS' $ExpectedInstance $RequestId}
  'Capabilities' {Invoke-Control 'CAPABILITIES' $ExpectedInstance $RequestId}
  'Diagnostics' {Invoke-Control 'DIAGNOSTICS' $ExpectedInstance $RequestId}
  'PublishBoard' {if(!$Folder){throw 'PUBLISHED_BOARD_FOLDER_REQUIRED'};$owner=Invoke-Control 'STATUS';Invoke-Control 'PUBLISH_BOARD' $owner.instanceId $RequestId}
  'Start' {Start-Host (Exact-Executable)}
  'Stop' {Stop-Host (Exact-Executable)}
  'Restart' {$path=Exact-Executable;$null=Stop-Host $path;Start-Host $path}
 }
 $result | ConvertTo-Json -Depth 30 -Compress
} catch {
 $code=$_.Exception.Message
 if($code -notmatch '^(APP_|PUBLISHED_BOARD_)[A-Z_]+$'){$code='APP_CONTROL_UNREACHABLE'}
 @{errorCode=$code;state='UNCONFIRMED';automaticWriteRetry=$false} | ConvertTo-Json -Compress
 exit 1
}
