param([string]$Probe='src-tauri/target/debug/examples/independent-launch-probe.exe')
$ErrorActionPreference='Stop'
$priorPort=$env:AI_WORK_ROUTER_MOBILE_PORT;$env:AI_WORK_ROUTER_MOBILE_PORT='59816'
Add-Type @"
using System;using System.Runtime.InteropServices;
public static class AgbrioJobProbe {
 [StructLayout(LayoutKind.Sequential,CharSet=CharSet.Unicode)]public struct SI {public int cb;public string reserved,desktop,title;public int x,y,w,h,cx,cy,flags;public short show,res;public IntPtr reserved2,input,output,error;}
 [StructLayout(LayoutKind.Sequential)]public struct PI{public IntPtr process,thread;public int pid,tid;}
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)]public static extern IntPtr CreateJobObject(IntPtr a,string n);
 [DllImport("kernel32.dll",SetLastError=true)]public static extern bool SetInformationJobObject(IntPtr j,int c,IntPtr p,uint n);
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)]public static extern bool CreateProcess(string app,string args,IntPtr p,IntPtr t,bool inherit,uint flags,IntPtr env,string cwd,ref SI si,out PI pi);
 [DllImport("kernel32.dll",SetLastError=true)]public static extern bool AssignProcessToJobObject(IntPtr j,IntPtr p);
 [DllImport("kernel32.dll")]public static extern uint ResumeThread(IntPtr t);
 [DllImport("kernel32.dll")]public static extern bool CloseHandle(IntPtr h);
}
"@
$root=Join-Path (Get-Location).Path ('runtime/independent-job-'+[guid]::NewGuid().ToString('N'));New-Item -ItemType Directory -Path $root|Out-Null;[IO.File]::WriteAllText((Join-Path $root '.owned-probe'),'isolated');$out=Join-Path $root 'proof.json';$exe=(Resolve-Path -LiteralPath $Probe).Path
$job=[AgbrioJobProbe]::CreateJobObject([IntPtr]::Zero,$null);$mem=[Runtime.InteropServices.Marshal]::AllocHGlobal(144);$pi=[AgbrioJobProbe+PI]::new()
try {
 for($i=0;$i -lt 144;$i++){[Runtime.InteropServices.Marshal]::WriteByte($mem,$i,0)};[Runtime.InteropServices.Marshal]::WriteInt32($mem,16,0x2000)
 if(![AgbrioJobProbe]::SetInformationJobObject($job,9,$mem,144)){throw 'JOB_SETUP_FAILED'}
 $si=[AgbrioJobProbe+SI]::new();$si.cb=[Runtime.InteropServices.Marshal]::SizeOf($si)
 if(![AgbrioJobProbe]::CreateProcess($exe,('"'+$exe+'" "'+$out+'"'),[IntPtr]::Zero,[IntPtr]::Zero,$false,0x08000004,[IntPtr]::Zero,$root,[ref]$si,[ref]$pi)){throw 'PROBE_CREATE_FAILED'}
 if(![AgbrioJobProbe]::AssignProcessToJobObject($job,$pi.process)){throw 'PROBE_ASSIGN_FAILED'};[void][AgbrioJobProbe]::ResumeThread($pi.thread)
 $until=[DateTime]::UtcNow.AddSeconds(40);while(!(Test-Path -LiteralPath $out) -and [DateTime]::UtcNow -lt $until){Start-Sleep -Milliseconds 100}
 if(!(Test-Path -LiteralPath $out)){throw 'DETACHED_PROBE_NOT_OBSERVED'};$result=Get-Content -LiteralPath $out -Raw|ConvertFrom-Json
 if($result.port -ne '59816'){throw 'LAUNCH_ENVIRONMENT_NOT_PRESERVED'}
 # Another Windows job is permitted; prove survival of this exact parent job.
 [void][AgbrioJobProbe]::CloseHandle($job);$job=[IntPtr]::Zero
 [IO.File]::WriteAllText([IO.Path]::ChangeExtension($out,'release'),'parent job closed')
 $until=[DateTime]::UtcNow.AddSeconds(5);while(!(Test-Path -LiteralPath ([IO.Path]::ChangeExtension($out,'alive'))) -and [DateTime]::UtcNow -lt $until){Start-Sleep -Milliseconds 100}
 if(!(Test-Path -LiteralPath ([IO.Path]::ChangeExtension($out,'alive')))){throw 'CHILD_DID_NOT_SURVIVE_JOB_CLOSE'}
 @{independentLaunch='PASS';restrictedKillJob=$true;survivedJobClose=$true;realAppStarts=0;root=$root}|ConvertTo-Json -Compress
}finally{$env:AI_WORK_ROUTER_MOBILE_PORT=$priorPort;if($job -ne [IntPtr]::Zero){[void][AgbrioJobProbe]::CloseHandle($job)};if($pi.thread -ne [IntPtr]::Zero){[void][AgbrioJobProbe]::CloseHandle($pi.thread)};if($pi.process -ne [IntPtr]::Zero){[void][AgbrioJobProbe]::CloseHandle($pi.process)};[Runtime.InteropServices.Marshal]::FreeHGlobal($mem)}
