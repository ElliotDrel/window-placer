param(
 [string]$InstallDirectory = (Join-Path $env:USERPROFILE 'Applications\WindowPlacer'),
 [string]$StartupDirectory = ([Environment]::GetFolderPath('Startup')),
 [switch]$AsObject
)
$ErrorActionPreference='Stop'
$exe=Join-Path $InstallDirectory 'window-placer.exe'
$config=Join-Path $InstallDirectory 'windows.conf'
$link=Join-Path $StartupDirectory 'Window Placer.lnk'
$running=@(Get-CimInstance Win32_Process -Filter "Name='window-placer.exe'" | Where-Object { $_.ExecutablePath -eq $exe -and $_.CommandLine -like '*--apply*' })
$target=$null; $arguments=$null
if(Test-Path -LiteralPath $link) { $s=(New-Object -ComObject WScript.Shell).CreateShortcut($link); $target=$s.TargetPath; $arguments=$s.Arguments }
$result=[pscustomobject]@{
 Running=($running.Count -eq 1); Pids=@($running.ProcessId); Executable=$exe;
 Configuration=$config; ConfigurationExists=(Test-Path -LiteralPath $config);
 StartupShortcut=$link; StartupTargetCorrect=($target -eq $exe);
 StartupArgumentsCorrect=($arguments -eq ('--apply --config "'+$config+'"') -or $arguments -eq ('--apply --config "'+$config+'" --trace'));
 TraceEnabled=($arguments -like '* --trace');
 Diagnostics=(Join-Path $InstallDirectory 'window-placer.log')
}
if($AsObject) { return $result }
$result|Format-List
if(Test-Path -LiteralPath $result.Diagnostics) { 'Recent background events:'; Get-Content -LiteralPath $result.Diagnostics -Tail 8 }
'Windows Settings > Apps > Startup can disable a login entry. This check does not override that preference.'
