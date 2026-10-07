param([uint32]$DesktopPid,[string]$Directory)
$ErrorActionPreference='Stop'
# Reuse registered Codex branding without modifying/restarting the application.
$package=Get-AppxPackage -Name OpenAI.Codex
if(!$package){throw 'SHARED_DESKTOP_BINARY_UNRESOLVED'}
$target=Get-Process -Id $DesktopPid
if($target.Path -notmatch '^C:\\Program Files\\WindowsApps\\OpenAI\.Codex_' -or $target.MainWindowHandle -eq [IntPtr]::Zero){throw 'SHARED_DESKTOP_WINDOW_UNRESOLVED'}
$light=(Get-ItemProperty -LiteralPath 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize' -Name SystemUsesLightTheme -ErrorAction SilentlyContinue).SystemUsesLightTheme -eq 1
$variant=if($light){'lightunplated'}else{'unplated'}
$png=[IO.File]::ReadAllBytes((Join-Path $package.InstallLocation ('Assets/Square44x44Logo.targetsize-256_altform-'+$variant+'.png')))
$cache=Join-Path $Directory 'icons'
[void][IO.Directory]::CreateDirectory($cache)
$icon=Join-Path $cache ('codex-'+$package.Version+'-'+$variant+'.ico')
# Lossless ICO container around the official packaged PNG; no copied executable.
$stream=New-Object IO.MemoryStream
$writer=New-Object IO.BinaryWriter($stream)
try {
 $writer.Write([uint16]0);$writer.Write([uint16]1);$writer.Write([uint16]1)
 $writer.Write([byte]0);$writer.Write([byte]0);$writer.Write([byte]0);$writer.Write([byte]0)
 $writer.Write([uint16]1);$writer.Write([uint16]32);$writer.Write([uint32]$png.Length);$writer.Write([uint32]22);$writer.Write($png)
 [IO.File]::WriteAllBytes($icon,$stream.ToArray())
}finally{$writer.Dispose();$stream.Dispose()}
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class AgbrioCodexIdentity {
 [StructLayout(LayoutKind.Sequential)] public struct Key { public Guid format; public uint id; public Key(uint n){format=new Guid("9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3");id=n;} }
 [StructLayout(LayoutKind.Explicit,Size=24)] public struct Value { [FieldOffset(0)] public ushort type; [FieldOffset(8)] public IntPtr pointer; }
 [ComImport,Guid("886D8EEB-8CF2-4446-8D02-CDBA1DBDCF99"),InterfaceType(ComInterfaceType.InterfaceIsIUnknown)] public interface Store {
  [PreserveSig]int GetCount(out uint count); [PreserveSig]int GetAt(uint index,out Key key);
  [PreserveSig]int GetValue(ref Key key,out Value value); [PreserveSig]int SetValue(ref Key key,ref Value value); [PreserveSig]int Commit();
 }
 [DllImport("shell32.dll")] static extern int SHGetPropertyStoreForWindow(IntPtr hwnd,ref Guid iid,out Store store);
 [DllImport("ole32.dll")] static extern int PropVariantClear(ref Value value);
 static string Read(Store store,uint id){var key=new Key(id);Value v;Marshal.ThrowExceptionForHR(store.GetValue(ref key,out v));try{return v.type==31?Marshal.PtrToStringUni(v.pointer):"";}finally{PropVariantClear(ref v);}}
 static void Write(Store store,uint id,string text){var key=new Key(id);var v=new Value{type=31,pointer=Marshal.StringToCoTaskMemUni(text)};try{Marshal.ThrowExceptionForHR(store.SetValue(ref key,ref v));}finally{PropVariantClear(ref v);}}
 public static bool Apply(IntPtr hwnd,string icon,string appId){
  var iid=typeof(Store).GUID;Store s;Marshal.ThrowExceptionForHR(SHGetPropertyStoreForWindow(hwnd,ref iid,out s));
  try{Write(s,3,icon+",0");if(String.IsNullOrEmpty(Read(s,5)))Write(s,5,appId);return Read(s,3)==icon+",0";}finally{Marshal.ReleaseComObject(s);}
 }
}
'@
$applied=[AgbrioCodexIdentity]::Apply($target.MainWindowHandle,$icon,($package.PackageFamilyName+'!App'))
@{applied=$applied;packageVersion=$package.Version.ToString()} | ConvertTo-Json -Compress
