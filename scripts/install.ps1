param([string]$SourceDirectory = (Split-Path $PSScriptRoot -Parent), [switch]$StartNow)
$ErrorActionPreference = 'Stop'
$sourceExe = Join-Path $SourceDirectory 'window-placer.exe'
$sourceConfig = Join-Path $SourceDirectory 'windows.conf'
if (-not (Test-Path -LiteralPath $sourceExe) -or -not (Test-Path -LiteralPath $sourceConfig)) {
    throw 'Extract the release ZIP first; the executable and windows.conf must be together.'
}
# AppData can be virtualized when an installer is launched from a packaged app.
# Keep the executable and configuration outside that virtualized namespace.
$install = Join-Path $env:USERPROFILE 'Applications\WindowPlacer'
$legacyInstall = Join-Path $env:LOCALAPPDATA 'WindowPlacer'
$exe = Join-Path $install 'window-placer.exe'
$config = Join-Path $install 'windows.conf'
$legacyConfig = Join-Path $legacyInstall 'windows.conf'
$candidateConfig = if (Test-Path -LiteralPath $config) { $config } elseif (Test-Path -LiteralPath $legacyConfig) { $legacyConfig } else { $sourceConfig }
# Validate configuration and startup before changing the installed copy.
$check = Start-Process $sourceExe -ArgumentList ('--duration 1 --config "' + $candidateConfig + '"') -Wait -PassThru -WindowStyle Hidden
if ($check.ExitCode -ne 0) { throw "Configuration/startup validation failed (exit $($check.ExitCode))." }
New-Item -ItemType Directory -Path $install -Force | Out-Null
Get-CimInstance Win32_Process -Filter "Name = 'window-placer.exe'" | Where-Object {
    $_.ExecutablePath -eq $exe -or $_.ExecutablePath -eq (Join-Path $legacyInstall 'window-placer.exe')
} | ForEach-Object {
    $previous = Get-Process -Id $_.ProcessId
    Stop-Process -Id $_.ProcessId -ErrorAction Stop
    if (-not $previous.WaitForExit(5000)) { throw 'Previous installed process did not exit.' }
}
if ((Resolve-Path -LiteralPath $sourceExe).Path -ne $exe) { Copy-Item -LiteralPath $sourceExe -Destination $exe -Force }
if (-not (Test-Path -LiteralPath $config)) { Copy-Item -LiteralPath $candidateConfig -Destination $config }
$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut((Join-Path ([Environment]::GetFolderPath('Startup')) 'Window Placer.lnk'))
$shortcut.TargetPath = $exe
$shortcut.Arguments = '--apply --config "' + $config + '"'
$shortcut.WorkingDirectory = $install
$shortcut.Description = 'Place app windows once using windows.conf'
$shortcut.Save()
if ($StartNow) { Start-Process $exe -ArgumentList $shortcut.Arguments -WorkingDirectory $install -WindowStyle Hidden }
Write-Output "Installed: $exe"
Write-Output "Configuration: $config"
