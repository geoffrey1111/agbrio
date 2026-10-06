param([int]$PeerPort,[int]$ServerPort,[string]$OwnerSid,[int]$ExpectedServerPid=0)
$ErrorActionPreference='Stop'
try {
 if($ExpectedServerPid){$localPort=$ServerPort;$remotePort=$PeerPort}else{$localPort=$PeerPort;$remotePort=$ServerPort}
 $connections=@(Get-NetTCPConnection -LocalPort $localPort -RemotePort $remotePort -State Established -ErrorAction Stop | Where-Object {$_.LocalAddress -eq '127.0.0.1' -and $_.RemoteAddress -eq '127.0.0.1'})
 if($connections.Count -ne 1){throw 'Ambiguous peer'}
 if($ExpectedServerPid -and $connections[0].OwningProcess -ne $ExpectedServerPid){throw 'Different server'}
 $peer=Get-CimInstance Win32_Process -Filter ('ProcessId='+$connections[0].OwningProcess)
 $sid=(Invoke-CimMethod -InputObject $peer -MethodName GetOwnerSid -ErrorAction Stop).Sid
 if($sid -ne $OwnerSid){throw 'Different owner'}
 @{owner=$true;desktop=($peer.Name -eq 'ChatGPT.exe' -and $peer.ExecutablePath -match '^C:\\Program Files\\WindowsApps\\OpenAI\.Codex_')}|ConvertTo-Json -Compress
} catch {Write-Output '{"owner":false,"desktop":false}';exit 1}
