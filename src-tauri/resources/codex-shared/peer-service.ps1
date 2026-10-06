param([string]$OwnerSid,[uint32]$QueryPid=0)
$ErrorActionPreference='Stop'
Add-Type -TypeDefinition @"
using System;using System.Net;using System.Runtime.InteropServices;using System.Security.Principal;using System.Text;
public static class RouterPeerOwner {
 [StructLayout(LayoutKind.Sequential)] struct FileInfo {public uint Attributes,CreatedLow,CreatedHigh,AccessLow,AccessHigh,WrittenLow,WrittenHigh,Volume,SizeHigh,SizeLow,Links,IndexHigh,IndexLow;}
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode)] static extern IntPtr CreateFile(string name,uint access,uint share,IntPtr security,uint disposition,uint flags,IntPtr template);
 [DllImport("kernel32.dll")] static extern bool GetFileInformationByHandle(IntPtr handle,out FileInfo information);
 static string desktopFile;
 static string FileIdentity(string path){var handle=CreateFile(path,0,7,IntPtr.Zero,3,0,IntPtr.Zero);if(handle==new IntPtr(-1))return null;try{FileInfo file;if(!GetFileInformationByHandle(handle,out file))return null;return file.Volume+":"+file.IndexHigh+":"+file.IndexLow;}finally{CloseHandle(handle);}}
 public static void ConfigureDesktop(string path){desktopFile=FileIdentity(path);}
 [DllImport("iphlpapi.dll")] static extern uint GetExtendedTcpTable(IntPtr table,ref uint size,bool order,int family,int cls,uint reserved);
 [DllImport("kernel32.dll")] static extern IntPtr OpenProcess(uint access,bool inherit,uint pid);
 [DllImport("advapi32.dll")] static extern bool OpenProcessToken(IntPtr process,uint access,out IntPtr token);
 [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode)] static extern bool QueryFullProcessImageName(IntPtr process,uint flags,StringBuilder path,ref uint size);
 [DllImport("kernel32.dll")] static extern bool GetProcessTimes(IntPtr process,out long created,out long exited,out long kernel,out long user);
 [DllImport("kernel32.dll")] static extern bool GetExitCodeProcess(IntPtr process,out uint code);
 static bool DesktopLineage(uint pid,string owner) {
  var identity=Identity(pid);if(identity==null||identity[1]!=owner||desktopFile==null)return false;
  // File object identity handles Store junctions, without accepting arbitrary
  // descendants (our shell/test clients are also children of Codex Desktop).
  return FileIdentity(identity[0])==desktopFile;
 }
 public static string[] Identity(uint pid){var process=OpenProcess(0x1000,false,pid);if(process==IntPtr.Zero)return null;try{uint code;long created,exited,kernel,user;if(!GetExitCodeProcess(process,out code)||code!=259||!GetProcessTimes(process,out created,out exited,out kernel,out user))return null;var path=new StringBuilder(4096);uint length=4096;if(!QueryFullProcessImageName(process,0,path,ref length))return null;IntPtr token;if(!OpenProcessToken(process,8,out token))return null;try{using(var identity=new WindowsIdentity(token)){return new string[]{path.ToString(),identity.User.Value,created.ToString(System.Globalization.CultureInfo.InvariantCulture)};}}finally{CloseHandle(token);}}finally{CloseHandle(process);}}
 public static int Verify(int peerPort,int serverPort,string owner,uint expectedPid) {
  uint size=0;GetExtendedTcpTable(IntPtr.Zero,ref size,false,2,5,0);IntPtr buffer=Marshal.AllocHGlobal((int)size);
  try {if(GetExtendedTcpTable(buffer,ref size,false,2,5,0)!=0)return 0;int rows=Marshal.ReadInt32(buffer);uint pid=0;int hits=0;
   for(int i=0;i<rows;i++){IntPtr row=IntPtr.Add(buffer,4+i*24);if(Marshal.ReadInt32(row)!=5||Marshal.ReadInt32(row,4)!=0x0100007f||Marshal.ReadInt32(row,12)!=0x0100007f)continue;int lp=Marshal.ReadInt32(row,8),rp=Marshal.ReadInt32(row,16);lp=((lp&255)<<8)|((lp>>8)&255);rp=((rp&255)<<8)|((rp>>8)&255);
    if(lp==(expectedPid==0?peerPort:serverPort)&&rp==(expectedPid==0?serverPort:peerPort)){hits++;pid=(uint)Marshal.ReadInt32(row,20);}}
   if(hits!=1||(expectedPid!=0&&pid!=expectedPid))return 0;IntPtr process=OpenProcess(0x1000,false,pid);if(process==IntPtr.Zero)return 0;
   try{IntPtr token;if(!OpenProcessToken(process,8,out token))return 0;try{using(var identity=new WindowsIdentity(token)){if(identity.User.Value!=owner)return 0;}}finally{CloseHandle(token);}return DesktopLineage(pid,owner)?2:1;}finally{CloseHandle(process);}
  }finally{Marshal.FreeHGlobal(buffer);}
 }
}
"@
if($QueryPid -eq 0){
 $package=Get-AppxPackage -Name 'OpenAI.Codex'
 if($package.InstallLocation){[RouterPeerOwner]::ConfigureDesktop((Join-Path $package.InstallLocation 'app/ChatGPT.exe'))}
}
function Identity([uint32]$processId){$v=[RouterPeerOwner]::Identity($processId);if($v -and $v.Length -eq 3){return @{pid=$processId;imagePath=$v[0];ownerSid=$v[1];createdAt=$v[2]}};return $null}
if($QueryPid -gt 0){Identity $QueryPid|ConvertTo-Json -Compress;exit}
$parent=(Get-CimInstance Win32_Process -Filter ('ProcessId='+$PID)).ParentProcessId
@{ready=$true;supervisorIdentity=(Identity $parent)}|ConvertTo-Json -Depth 4 -Compress|ForEach-Object {[Console]::WriteLine($_)}
while(($line=[Console]::ReadLine()) -ne $null){
 try{$request=$line|ConvertFrom-Json;$result=[RouterPeerOwner]::Verify([int]$request.peerPort,[int]$request.serverPort,$OwnerSid,[uint32]$request.expectedPid);$reply=@{id=$request.id;owner=($result -gt 0);desktop=($result -eq 2)}|ConvertTo-Json -Compress;[Console]::WriteLine($reply)}catch{[Console]::WriteLine('{"id":0,"owner":false,"desktop":false}')}
}
