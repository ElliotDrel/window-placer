param([Parameter(Mandatory=$true)][string]$BinaryPath)
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot -Parent
$already=@(Get-CimInstance Win32_Process -Filter "Name='window-placer.exe'"|Where-Object {$_.CommandLine -like '*--apply*'})
if($already.Count) { throw 'Close active Window Placer instances before the isolated installer lifecycle test.' }
$testRoot=Join-Path ([IO.Path]::GetTempPath()) ('Window Placer Startup Test '+[Guid]::NewGuid())
$source=Join-Path $testRoot 'release files'
$install=Join-Path $testRoot 'installed app'
$startup=Join-Path $testRoot 'test startup'
New-Item -ItemType Directory -Path $source -Force|Out-Null
Copy-Item -LiteralPath $BinaryPath -Destination (Join-Path $source 'window-placer.exe')
Copy-Item -LiteralPath (Join-Path $repo 'scripts') -Destination $source -Recurse
Copy-Item -LiteralPath (Join-Path $repo 'Setup.cmd'),(Join-Path $repo 'Status.cmd') -Destination $source
# No real app can match this fixture configuration: this test moves no windows.
'window-placer-startup-test-never.exe | 50 | 50 | 800 | 600'|Set-Content (Join-Path $source 'windows.conf') -Encoding UTF8
function Literal([string]$value) { return "'"+$value.Replace("'","''")+"'" }
Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;
public static class WindowPlacerJobTest {
 [StructLayout(LayoutKind.Sequential,CharSet=CharSet.Unicode)]
 struct SI { public uint cb; public string reserved,desktop,title; public uint x,y,w,h,xc,yc,fill,flags; public ushort show,extra; public IntPtr ptr,input,output,error; }
 [StructLayout(LayoutKind.Sequential)] struct PI { public IntPtr process,thread; public uint pid,tid; }
 [StructLayout(LayoutKind.Sequential)] struct Basic { public long processTime,jobTime; public uint flags; public UIntPtr min,max; public uint active; public UIntPtr affinity; public uint priority,scheduling; }
 [StructLayout(LayoutKind.Sequential)] struct IO { public ulong a,b,c,d,e,f; }
 [StructLayout(LayoutKind.Sequential)] struct Limits { public Basic basic; public IO io; public UIntPtr processMemory,jobMemory,peakProcess,peakJob; }
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern IntPtr CreateJobObject(IntPtr attr,string name);
 [DllImport("kernel32.dll",SetLastError=true)] static extern bool SetInformationJobObject(IntPtr job,int type,ref Limits data,uint size);
 [DllImport("kernel32.dll",SetLastError=true)] static extern bool AssignProcessToJobObject(IntPtr job,IntPtr process);
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern bool CreateProcess(string app,StringBuilder cmd,IntPtr pa,IntPtr ta,bool inherit,uint flags,IntPtr env,string cwd,ref SI si,out PI pi);
 [DllImport("kernel32.dll",SetLastError=true)] static extern uint ResumeThread(IntPtr thread);
 [DllImport("kernel32.dll")] static extern uint WaitForSingleObject(IntPtr process,uint ms);
 [DllImport("kernel32.dll",SetLastError=true)] static extern bool GetExitCodeProcess(IntPtr process,out uint code);
 [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
 public static void Run(string app,string args) {
  IntPtr job=CreateJobObject(IntPtr.Zero,null); if(job==IntPtr.Zero) throw new Win32Exception();
  var pi=new PI();
  try {
   var limits=new Limits(); limits.basic.flags=0x2000|0x800; // kill-on-close + allow breakaway
   if(!SetInformationJobObject(job,9,ref limits,(uint)Marshal.SizeOf(typeof(Limits)))) throw new Win32Exception();
   var si=new SI();si.cb=(uint)Marshal.SizeOf(typeof(SI));si.flags=1;si.show=0;
   if(!CreateProcess(app,new StringBuilder("\""+app+"\" "+args),IntPtr.Zero,IntPtr.Zero,false,0x00000014,IntPtr.Zero,null,ref si,out pi)) throw new Win32Exception();
   if(!AssignProcessToJobObject(job,pi.process)) throw new Win32Exception();
   if(ResumeThread(pi.thread)==0xffffffff) throw new Win32Exception();
   if(WaitForSingleObject(pi.process,30000)!=0) throw new Exception("Installer timed out.");
   uint code;if(!GetExitCodeProcess(pi.process,out code)||code!=0) throw new Exception("Installer failed inside the test job; exit "+code);
  } finally {
   // Closing this job kills ordinary child launches. The app must have escaped.
   CloseHandle(job);if(pi.thread!=IntPtr.Zero)CloseHandle(pi.thread);if(pi.process!=IntPtr.Zero)CloseHandle(pi.process);
  }
 }
}
'@
try {
 $script='$ErrorActionPreference=''Stop''; & '+(Literal (Join-Path $source 'scripts\install.ps1'))+' -SourceDirectory '+(Literal $source)+' -InstallDirectory '+(Literal $install)+' -StartupDirectory '+(Literal $startup)+' -StartNow'
 $encoded=[Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($script))
 [WindowPlacerJobTest]::Run((Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'),'-NoProfile -NonInteractive -WindowStyle Hidden -EncodedCommand '+$encoded)
 Start-Sleep -Seconds 2
 $health=& (Join-Path $repo 'scripts\status.ps1') -InstallDirectory $install -StartupDirectory $startup -AsObject
 if(-not $health.Running -or -not $health.StartupTargetCorrect -or -not $health.StartupArgumentsCorrect) { throw 'Shortcut-launched app did not survive parent job cleanup.' }
 $appPid=$health.Pids[0]
 $installedConfig=Join-Path $install 'windows.conf'
 $custom='# keep my settings'+[Environment]::NewLine+'window-placer-startup-test-never.exe | 70 | 80 | 900 | 650'
 [IO.File]::WriteAllText($installedConfig,$custom)
 & (Join-Path $source 'scripts\install.ps1') -SourceDirectory $source -InstallDirectory $install -StartupDirectory $startup
 if([IO.File]::ReadAllText($installedConfig) -ne $custom) { throw 'Reinstall overwrote user configuration.' }
 # Setup without StartNow stops this test copy deliberately; start it again.
 & (Join-Path $repo 'scripts\start-shortcut.ps1') -ShortcutPath (Join-Path $startup 'Window Placer.lnk')
 Start-Sleep -Seconds 2
 $before=& (Join-Path $repo 'scripts\status.ps1') -InstallDirectory $install -StartupDirectory $startup -AsObject
 if(-not $before.Running){throw 'Reinstalled test app did not start.'}
 [IO.File]::WriteAllText($installedConfig,'not valid config')
 $rejected=$false
 try { & (Join-Path $source 'scripts\install.ps1') -SourceDirectory $source -InstallDirectory $install -StartupDirectory $startup -StartNow } catch { $rejected=$true }
 if(-not $rejected) { throw 'Invalid configuration was accepted.' }
 $after=& (Join-Path $repo 'scripts\status.ps1') -InstallDirectory $install -StartupDirectory $startup -AsObject
 if(-not $after.Running -or $before.Pids[0] -ne $after.Pids[0]) { throw 'Failed preflight interrupted the healthy listener.' }
 Write-Output 'PASS: actual shortcut, paths with spaces, parent job cleanup, configuration preservation, failed-preflight isolation.'
} finally {
 $testExe=Join-Path $install 'window-placer.exe'
 Get-CimInstance Win32_Process -Filter "Name='window-placer.exe'"|Where-Object ExecutablePath -eq $testExe|ForEach-Object { $p=Get-Process -Id $_.ProcessId; Stop-Process -Id $_.ProcessId; $p.WaitForExit(5000)|Out-Null }
 $resolved=[IO.Path]::GetFullPath($testRoot)
 $temp=[IO.Path]::GetFullPath([IO.Path]::GetTempPath())
 if(-not $resolved.StartsWith($temp,[StringComparison]::OrdinalIgnoreCase)) { throw 'Refusing cleanup outside the test temp directory.' }
 Remove-Item -LiteralPath $resolved -Recurse -Force
}
