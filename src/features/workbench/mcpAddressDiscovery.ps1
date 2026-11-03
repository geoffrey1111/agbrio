$ErrorActionPreference = 'Stop'
$configPath = Join-Path $env:LOCALAPPDATA 'AIWorkRouter\data\mobile-runtime.json'
if (!(Test-Path -LiteralPath $configPath -PathType Leaf)) {
  throw 'AGBRIO_MCP_ADDRESS_MISSING: Open Agbrio Settings > Devices and check this computer connection.'
}
$connection = Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json
$origin = $null
if (![Uri]::TryCreate([string]$connection.allowed_origin, [UriKind]::Absolute, [ref]$origin) -or
    $origin.Scheme -ne 'https' -or !$origin.Host -or $origin.UserInfo -or
    $origin.AbsolutePath -ne '/' -or $origin.Query -or $origin.Fragment) {
  throw 'AGBRIO_MCP_HTTPS_REQUIRED: Keep the existing phone connection; check its HTTPS entrance in Agbrio Settings > Devices.'
}
# Public address only. No grants, credentials, plugins or settings are changed.
$endpoint = $origin.GetLeftPart([UriPartial]::Authority) + '/mcp'
[pscustomobject]@{ name = 'agbrio'; url = $endpoint; transport = 'Streamable HTTP'; authentication = 'OAuth' } | ConvertTo-Json
