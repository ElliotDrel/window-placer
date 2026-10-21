param(
 [string]$SourceDirectory = (Split-Path $PSScriptRoot -Parent),
 [string]$InstallDirectory = (Join-Path $env:USERPROFILE 'Applications\WindowPlacer'),
 [string]$StartupDirectory = ([Environment]::GetFolderPath('Startup')),
 [switch]$StartNow
)

$ErrorActionPreference = 'Stop'
$sourceExe = Join-Path $SourceDirectory 'window-placer.exe'
$sourceConfig = Join-Path $SourceDirectory 'windows.conf'
if (-not (Test-Path -LiteralPath $sourceExe) -or -not (Test-Path -LiteralPath $sourceConfig)) {
    throw 'Extract the release ZIP first; the executable and windows.conf must be together.'
}
# AppData can be virtualized when an installer is launched from a packaged app.
# Keep the executable and configuration outside that virtualized namespace.
$install = [IO.Path]::GetFullPath($InstallDirectory)
$StartupDirectory = [IO.Path]::GetFullPath($StartupDirectory)
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
New-Item -ItemType Directory -Path $StartupDirectory -Force | Out-Null
$shortcutPath = Join-Path $StartupDirectory 'Window Placer.lnk'
$shortcut = $shell.CreateShortcut($shortcutPath)
$shortcut.TargetPath = $exe
$shortcut.Arguments = '--apply --config "' + $config + '"'
$shortcut.WorkingDirectory = $install
$shortcut.Description = 'Place app windows once using windows.conf'
$shortcut.Save()
# Keep setup/status available after the downloaded ZIP is discarded.
$sourceScripts=Join-Path $SourceDirectory 'scripts'
$installedScripts=Join-Path $install 'scripts'
if((Resolve-Path -LiteralPath $sourceScripts).Path -ne $installedScripts) {
 New-Item -ItemType Directory -Path $installedScripts -Force | Out-Null
 foreach($name in @('install.ps1','start-shortcut.ps1','status.ps1')) { Copy-Item -LiteralPath (Join-Path $sourceScripts $name) -Destination (Join-Path $installedScripts $name) -Force }
}
foreach($name in @('Setup.cmd','Status.cmd')) {
 $source=Join-Path $SourceDirectory $name; $destination=Join-Path $install $name
 if((Resolve-Path -LiteralPath $source).Path -ne $destination) { Copy-Item -LiteralPath $source -Destination $destination -Force }
}
if ($StartNow) {
 & (Join-Path $installedScripts 'start-shortcut.ps1') -ShortcutPath $shortcutPath
 Start-Sleep -Seconds 3
 $health=& (Join-Path $installedScripts 'status.ps1') -InstallDirectory $install -StartupDirectory $StartupDirectory -AsObject
 if(-not $health.Running -or -not $health.StartupTargetCorrect -or -not $health.StartupArgumentsCorrect) { throw 'Startup verification failed. Run Status.cmd and inspect window-placer.log.' }
 Write-Output ('Verified startup shortcut launch; running PID: '+($health.Pids -join ', '))
}
Write-Output "Installed: $exe"
Write-Output "Configuration: $config"
