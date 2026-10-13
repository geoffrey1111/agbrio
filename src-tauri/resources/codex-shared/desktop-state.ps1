# Read-only registered Desktop discovery; never launch, close or select a chat.
$ErrorActionPreference='Stop'
$packages=@(Get-AppxPackage -Name 'OpenAI.Codex')
if($packages.Count -ne 1 -or !$packages[0].InstallLocation){throw 'SHARED_DESKTOP_BINARY_UNRESOLVED'}
$desktop=Join-Path $packages[0].InstallLocation 'app/ChatGPT.exe'
if(!(Test-Path -LiteralPath $desktop -PathType Leaf)){throw 'SHARED_DESKTOP_BINARY_UNRESOLVED'}
$owner=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$count=0
$candidates=@(Get-CimInstance Win32_Process -Filter "Name='ChatGPT.exe'" | Where-Object {[string]::Equals($_.ExecutablePath,$desktop,[StringComparison]::OrdinalIgnoreCase)})
$ids=[Collections.Generic.HashSet[uint32]]::new()
foreach($entry in $candidates){[void]$ids.Add([uint32]$entry.ProcessId)}
foreach($entry in $candidates){
 # Electron render/GPU helpers are part of the same Desktop, not additional
 # instances. Query ownership only for root instances to keep discovery bounded.
 if($ids.Contains([uint32]$entry.ParentProcessId)){continue}
 if((Invoke-CimMethod -InputObject $entry -MethodName GetOwnerSid).Sid -ceq $owner){$count++}
}
@{desktop=$desktop;count=$count}|ConvertTo-Json -Compress
