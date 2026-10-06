param(
 [Parameter(Mandatory=$true)][ValidateSet('CLOUDFLARE','TAILSCALE_FUNNEL','TAILSCALE_SERVE','CUSTOM_HTTPS')][string]$Method,
 [Parameter(Mandatory=$true)][string]$Origin,
 [string]$ExecutablePath,
 [string]$DataDirectory=(Join-Path $env:LOCALAPPDATA 'AIWorkRouter\data'),
 [switch]$NoWait,
 [int]$TimeoutSeconds=45
)
$ErrorActionPreference='Stop'
$agbrioUri=[Uri]$Origin
if(!$agbrioUri.IsAbsoluteUri -or $agbrioUri.Scheme -ne 'https' -or $agbrioUri.UserInfo -or $agbrioUri.AbsolutePath -ne '/' -or $agbrioUri.Query -or $agbrioUri.Fragment){throw 'Use an HTTPS origin without a path, query, fragment or credentials.'}
if($Method.StartsWith('TAILSCALE_') -and !$agbrioUri.Host.EndsWith('.ts.net')){throw 'Use the actual HTTPS .ts.net address provided by Tailscale.'}
$agbrioExe=$null
if($ExecutablePath){
 $agbrioExe=(Resolve-Path -LiteralPath $ExecutablePath).Path
 if([IO.Path]::GetFileName($agbrioExe) -ne 'ai-work-router.exe'){throw 'ExecutablePath must point to the installed Agbrio executable.'}
}
$agbrioData=[IO.Path]::GetFullPath($DataDirectory)
$agbrioRequest=Join-Path $agbrioData 'mobile-setup-request.json'
$agbrioId=[Guid]::NewGuid().ToString()
$agbrioTemporary=Join-Path $agbrioData ('mobile-setup-'+$agbrioId+'.pending')
foreach($agbrioTarget in @($agbrioRequest,$agbrioTemporary)){
 if([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($agbrioTarget)) -ne $agbrioData){throw 'Setup file is outside the chosen data directory.'}
}
New-Item -ItemType Directory -Path $agbrioData -Force|Out-Null
if(Test-Path -LiteralPath $agbrioRequest){throw 'A setup request is already pending. Wait for it to finish.'}
$agbrioJson=@{requestId=$agbrioId;method=$Method;origin=$agbrioUri.GetLeftPart([UriPartial]::Authority)}|ConvertTo-Json -Compress
[IO.File]::WriteAllText($agbrioTemporary,$agbrioJson,[Text.UTF8Encoding]::new($false))
Move-Item -LiteralPath $agbrioTemporary -Destination $agbrioRequest
if($agbrioExe){
 $agbrioRunning=Get-Process -Name ai-work-router -ErrorAction SilentlyContinue
 if(!$agbrioRunning){Start-Process -FilePath $agbrioExe -WindowStyle Hidden}
}
if($NoWait){@{requestId=$agbrioId;status='SUBMITTED'}|ConvertTo-Json;exit 0}
$agbrioResult=Join-Path $agbrioData 'mobile-setup-result.json'
$agbrioDeadline=(Get-Date).AddSeconds([Math]::Max(10,[Math]::Min(120,$TimeoutSeconds)))
while((Get-Date) -lt $agbrioDeadline){
 if(Test-Path -LiteralPath $agbrioResult){
  try{$agbrioReply=Get-Content -LiteralPath $agbrioResult -Raw|ConvertFrom-Json}catch{$agbrioReply=$null}
  if($agbrioReply -and $agbrioReply.requestId -eq $agbrioId){
   if($agbrioReply.status -eq 'READY'){@{status='READY';url=($agbrioUri.GetLeftPart([UriPartial]::Authority)+'/mobile')}|ConvertTo-Json;exit 0}
   if($agbrioReply.status -eq 'FAILED'){throw ('Setup failed: '+$agbrioReply.errorCode+'. Check the existing configuration and Devices status.')}
  }
 }
 Start-Sleep -Milliseconds 400
}
throw 'Setup has not completed. Start the current Agbrio release and check Devices. No agent or existing tunnel was stopped.'
