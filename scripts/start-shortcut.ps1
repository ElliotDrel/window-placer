param([Parameter(Mandatory=$true)][string]$ShortcutPath)
$ErrorActionPreference = 'Stop'
$ShortcutPath = (Resolve-Path -LiteralPath $ShortcutPath).Path
if ([IO.Path]::GetExtension($ShortcutPath) -ne '.lnk') { throw 'Expected a Windows shortcut.' }
# Give the one-shot PowerShell launcher its own hidden console and break it
# out of an automation host's job. The GUI app itself does not attach to it.
if (-not ('WindowPlacer.ShortcutLauncher' -as [type])) {
Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;
namespace WindowPlacer {
public static class ShortcutLauncher {
 [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)]
 struct StartupInfo {
  public uint cb; public string reserved, desktop, title;
  public uint x,y,xSize,ySize,xChars,yChars,fill,flags;
  public ushort show,reservedSize; public IntPtr reservedPtr,input,output,error;
 }
 [StructLayout(LayoutKind.Sequential)]
 struct ProcessInfo { public IntPtr process,thread; public uint pid,tid; }
 [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
 static extern bool CreateProcess(string app, StringBuilder cmd, IntPtr pa, IntPtr ta,
  bool inherit, uint flags, IntPtr env, string cwd, ref StartupInfo si, out ProcessInfo pi);
 [DllImport("kernel32.dll")] static extern uint WaitForSingleObject(IntPtr h,uint ms);
 [DllImport("kernel32.dll", SetLastError=true)] static extern bool GetExitCodeProcess(IntPtr h,out uint code);
 [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
 public static void Run(string app,string args) {
  var si=new StartupInfo(); si.cb=(uint)Marshal.SizeOf(typeof(StartupInfo)); si.flags=1; si.show=0;
  ProcessInfo pi;
  if(!CreateProcess(app,new StringBuilder("\""+app+"\" "+args),IntPtr.Zero,IntPtr.Zero,
    false,0x01000010,IntPtr.Zero,null,ref si,out pi)) throw new Win32Exception(Marshal.GetLastWin32Error());
  try {
   if(WaitForSingleObject(pi.process,15000)!=0) throw new Exception("Shortcut launcher did not finish within 15 seconds.");
   uint code; if(!GetExitCodeProcess(pi.process,out code) || code!=0) throw new Exception("Shortcut launcher failed. Check the shortcut target and configuration.");
  } finally { CloseHandle(pi.thread); CloseHandle(pi.process); }
 }
}}
'@
}
$literal = $ShortcutPath.Replace("'", "''")
$command = "`$ErrorActionPreference = 'Stop'; Start-Process -FilePath '$literal' -WindowStyle Hidden"
$encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($command))
$powershell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
[WindowPlacer.ShortcutLauncher]::Run($powershell, '-NoProfile -NonInteractive -WindowStyle Hidden -EncodedCommand ' + $encoded)
