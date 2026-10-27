# Run in ordinary PowerShell on the computer running Claude and Agbrio.
# Installs the bundled plugin only. Connection credentials stay in Agbrio.
$ErrorActionPreference = 'Stop'
function Invoke-AgbrioPluginCli {
  param([string]$Executable, [string[]]$CliArguments)
  $result = & $Executable @CliArguments
  if ($LASTEXITCODE -ne 0) { throw "Claude plugin command failed (exit $LASTEXITCODE). Keep the original error above; do not change permission settings." }
  return $result
}
function Install-AgbrioClaudePlugin {
  $packages = @(
    Get-CimInstance Win32_Process -Filter "Name='ai-work-router.exe'" |
      Where-Object ExecutablePath |
      ForEach-Object { Join-Path ([IO.Path]::GetDirectoryName($_.ExecutablePath)) 'claude-code-plugin' } |
      Where-Object { Test-Path -LiteralPath (Join-Path $_ '.claude-plugin/marketplace.json') } |
      Sort-Object -Unique
  )
  if ($packages.Count -ne 1) { throw 'Keep one installed Windows Agbrio client running, then retry.' }
  $package = $packages[0]
  $manifest = Get-Content -LiteralPath (Join-Path $package '.claude-plugin/plugin.json') -Raw | ConvertFrom-Json
  $marketplace = Get-Content -LiteralPath (Join-Path $package '.claude-plugin/marketplace.json') -Raw | ConvertFrom-Json
  if ($manifest.name -ne 'agbrio-bridge' -or $marketplace.name -ne 'agbrio-observation') { throw 'Unexpected bundled plugin. Stop and check the Agbrio installation.' }
  $roots = @("$env:LOCALAPPDATA/Claude-Data/claude_pzs8sxrjxfjjc", "$env:APPDATA/Claude", "$env:LOCALAPPDATA/Packages/Claude_pzs8sxrjxfjjc/LocalCache/Roaming/Claude")
  $engines = @(foreach ($root in $roots) {
    foreach ($version in @(Get-ChildItem -LiteralPath (Join-Path $root 'claude-code') -Directory -ErrorAction SilentlyContinue)) {
      if ($version.Attributes -band [IO.FileAttributes]::ReparsePoint) { continue }
      $number = $null
      if (![version]::TryParse($version.Name, [ref]$number) -or $number -lt [version]'2.1.286') { continue }
      foreach ($build in @(Get-ChildItem -LiteralPath $version.FullName -Directory -ErrorAction SilentlyContinue)) {
        if ($build.Attributes -band [IO.FileAttributes]::ReparsePoint) { continue }
        $path = Join-Path $build.FullName 'claude.exe'
        if ((Test-Path -LiteralPath $path -PathType Leaf) -and !((Get-Item -LiteralPath $path).Attributes -band [IO.FileAttributes]::ReparsePoint)) { [pscustomobject]@{Version=$number; Path=$path} }
      }
    }
  })
  $engine = $engines | Sort-Object Version -Descending | Select-Object -First 1
  if (!$engine) { throw 'Open a local Code session in Claude Desktop. If its /status version is below 2.1.286, update Claude Desktop.' }
  $id = 'agbrio-bridge@agbrio-observation'
  Invoke-AgbrioPluginCli $engine.Path @('plugin','marketplace','add',$package,'--json')
  $list = @((Invoke-AgbrioPluginCli $engine.Path @('plugin','list','--json') | Out-String) | ConvertFrom-Json)
  $installed = $list | Where-Object { $_.id -eq $id -and $_.scope -eq 'user' }
  $operation = if ($installed) { 'update' } else { 'install' }
  Invoke-AgbrioPluginCli $engine.Path @('plugin',$operation,$id,'--scope','user','--json')
  $list = @((Invoke-AgbrioPluginCli $engine.Path @('plugin','list','--json') | Out-String) | ConvertFrom-Json)
  $installed = @($list | Where-Object { $_.id -eq $id -and $_.scope -eq 'user' })
  if ($installed.Count -ne 1) { throw 'User-scope installation was not confirmed. Keep the command output.' }
  if (!$installed[0].enabled) { Invoke-AgbrioPluginCli $engine.Path @('plugin','enable',$id,'--scope','user','--json') }
  $final = @((Invoke-AgbrioPluginCli $engine.Path @('plugin','list','--json') | Out-String) | ConvertFrom-Json)
  $confirmed = @($final | Where-Object { $_.id -eq $id -and $_.scope -eq 'user' -and $_.enabled })
  if ($confirmed.Count -ne 1) { throw 'Plugin enabled state was not confirmed. Keep the command output.' }
  $final | ConvertTo-Json -Depth 6
  Write-Host 'Installation finished. Return to Agbrio and click Install and connect Claude full features to configure the connection. When the Claude session is idle, run /reload-plugins and check the connection. If still not loaded, save your work and fully quit/reopen Claude Desktop yourself.'
}
Install-AgbrioClaudePlugin
