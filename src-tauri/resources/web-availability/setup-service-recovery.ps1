param([Parameter(Mandatory=$true)][string]$OwnerSid,[Parameter(Mandatory=$true)][string]$EvidenceDirectory)
$ErrorActionPreference='Stop'
$identity=[Security.Principal.WindowsIdentity]::GetCurrent()
if(!([Security.Principal.WindowsPrincipal]::new($identity)).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){throw 'Windows administrator consent required'}
$userSid=[Security.Principal.SecurityIdentifier]::new($OwnerSid)
if(!$userSid.IsAccountSid()){throw 'Invalid owner identity'}
New-Item -ItemType Directory -Path $EvidenceDirectory -Force | Out-Null
$service=Get-CimInstance Win32_Service -Filter "Name='Cloudflared'"
if(!$service -or $service.PathName -notmatch '--token-file'){throw 'Expected installed credential-file Cloudflared service'}
$image=[string]$service.PathName
if($image -match '--token(?:=|\s)'){throw 'Inline credential command is outside this setup'}
$backup=Join-Path $EvidenceDirectory 'service-before.json'
$sddlOutput=& sc.exe sdshow Cloudflared
$oldSddl=($sddlOutput | Where-Object {$_ -match '^[DOGS]:'} | Select-Object -First 1).Trim()
if(!$oldSddl){throw 'Cannot preserve service security descriptor'}
if(!(Test-Path -LiteralPath $backup)){@{imagePath=$image;securityDescriptor=$oldSddl;ownerSid=$OwnerSid}|ConvertTo-Json | Set-Content -LiteralPath $backup -Encoding utf8}
# Preserve the installed binary, credential path and all tunnel routes. Only
# use Cloudflare's supported TCP transport and stable localhost readiness port.
$newImage=[regex]::Replace($image,'(?i)\s+--protocol(?:=|\s+)(auto|quic|http2)\b','')
$newImage=[regex]::Replace($newImage,'(?i)\s+--metrics(?:=|\s+)(?:"[^"]+"|\S+)','')
if($newImage -notmatch '(?i)\btunnel\s+run\b'){throw 'Unexpected service argument topology'}
$newImage=[regex]::Replace($newImage,'(?i)\btunnel\s+run\b','tunnel --protocol http2 --metrics 127.0.0.1:20241 run')
Set-ItemProperty -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Services\Cloudflared' -Name ImagePath -Value $newImage
# Grant this owner only QueryStatus(4), Start(16), Stop(32). No token file access
# or service/config/DACL editing rights are delegated to the app.
$sd=[Security.AccessControl.RawSecurityDescriptor]::new($oldSddl)
$exists=$false
foreach($ace in $sd.DiscretionaryAcl){if($ace -is [Security.AccessControl.CommonAce] -and $ace.SecurityIdentifier -eq $userSid -and ($ace.AccessMask -band 0x34) -eq 0x34){$exists=$true}}
if(!$exists){$ace=[Security.AccessControl.CommonAce]::new([Security.AccessControl.AceFlags]::None,[Security.AccessControl.AceQualifier]::AccessAllowed,0x34,$userSid,$false,$null);$sd.DiscretionaryAcl.InsertAce($sd.DiscretionaryAcl.Count,$ace);& sc.exe sdset Cloudflared $sd.GetSddlForm([Security.AccessControl.AccessControlSections]::All) | Out-Null;if($LASTEXITCODE -ne 0){throw 'Scoped recovery permission failed'}}
Restart-Service -Name Cloudflared
@{status='ADMIN_SETUP_COMPLETE';transport='http2';grantedRights=@('QueryStatus','Start','Stop');tokenContentRead=$false;routesChanged=$false}|ConvertTo-Json | Set-Content -LiteralPath (Join-Path $EvidenceDirectory 'setup-result.json') -Encoding utf8
