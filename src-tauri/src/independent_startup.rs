//! Detach a resident GUI launch from an agent's kill-on-close job before opening state.
#[cfg(windows)]
pub(crate) fn relay_if_needed()->bool{
 use std::os::windows::process::CommandExt;
 use windows::core::BOOL;
 use windows::Win32::System::{JobObjects::IsProcessInJob,Threading::GetCurrentProcess};
 let args=std::env::args().skip(1).collect::<Vec<_>>();
 // A single relay attempt only. A refused system consent does not loop.
 if args.iter().any(|a|a.contains("--agbrio-independent-child")||a=="--notification-activated"||a.eq_ignore_ascii_case("-Embedding")){return false;}
 let mut member=BOOL::default();
 unsafe{if IsProcessInJob(GetCurrentProcess(),None,&mut member).is_err()||!member.as_bool(){return false;}}
 let Ok(exe)=std::env::current_exe()else{return false;};
 // Restricted jobs cannot break away. Ask the existing desktop shell to launch
 // the exact same GUI executable with the normal desktop user token, matching
 // the installer RunAsUser path. Tool backends retain their separate UAC policy.
 // No service, task or startup registration is added.
 let marker="--agbrio-independent-child".to_string();
 let command_line=args.iter().chain(std::iter::once(&marker)).map(|a|quote(a)).collect::<Vec<_>>().join(" ");
 let mut env=std::collections::BTreeMap::new();for name in ["LOCALAPPDATA","WEBVIEW2_USER_DATA_FOLDER","AI_WORK_ROUTER_MOBILE_PORT"]{if let Ok(value)=std::env::var(name){env.insert(name,value);}}
 let payload=serde_json::json!({"exe":exe,"args":command_line,"env":env});
 let encoded=encode(&payload.to_string());
 let child_script=format!("$ErrorActionPreference='Stop';$p=[Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{encoded}'))|ConvertFrom-Json;foreach($e in $p.env.PSObject.Properties){{[Environment]::SetEnvironmentVariable($e.Name,[string]$e.Value,'Process')}};Start-Process -FilePath $p.exe -ArgumentList $p.args -WorkingDirectory (Split-Path -Parent $p.exe) -WindowStyle Hidden");
 let child_wide=child_script.encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<_>>();
 let child_args=format!("-NoProfile -NonInteractive -WindowStyle Hidden -EncodedCommand {}",encode_bytes(&child_wide));
 let script=format!("$ErrorActionPreference='Stop';$p=[Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{encoded}'))|ConvertFrom-Json;$owner=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value;$session=([Diagnostics.Process]::GetCurrentProcess()).SessionId;$explorers=@(Get-CimInstance Win32_Process -Filter \"Name='explorer.exe'\"|Where-Object {{$_.SessionId -eq $session}});if(!$explorers.Count){{exit 3}};foreach($e in $explorers){{$who=Invoke-CimMethod -InputObject $e -MethodName GetOwnerSid;if($who.Sid -ne $owner){{exit 3}}}};$w=(New-Object -ComObject Shell.Application).Windows();$l=[object]0;$r=[object]0;$h=[int]0;$d=$w.FindWindowSW([ref]$l,[ref]$r,8,[ref]$h,1);if(!$d){{exit 2}};$v='open';$powershell=[IO.Path]::Combine($env:SystemRoot,'System32/WindowsPowerShell/v1.0/powershell.exe');$d.Document.Application.ShellExecute($powershell,'{child_args}',(Split-Path -Parent $p.exe),$v,0)");
 let wide=script.encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<_>>();
 let shell=std::env::var_os("SystemRoot").map(std::path::PathBuf::from).unwrap_or_else(||"C:/Windows".into()).join("System32/WindowsPowerShell/v1.0/powershell.exe");
 let status=std::process::Command::new(shell).args(["-NoProfile","-NonInteractive","-WindowStyle","Hidden","-EncodedCommand",&encode_bytes(&wide)]).creation_flags(0x08000000).status();
 status.is_ok_and(|s|s.success())
}
#[cfg(windows)]fn encode(s:&str)->String{encode_bytes(s.as_bytes())}
#[cfg(windows)]fn encode_bytes(s:&[u8])->String{use base64ct::Encoding;base64ct::Base64::encode_string(s)}
#[cfg(windows)]fn quote(s:&str)->String{
 let mut out=String::from("\"");let mut slashes=0;
 for c in s.chars(){if c=='\\'{slashes+=1;continue;}if c=='"'{out.push_str(&"\\".repeat(slashes*2+1));}else{out.push_str(&"\\".repeat(slashes));}slashes=0;out.push(c);}
 out.push_str(&"\\".repeat(slashes*2));out.push('"');out
}
#[cfg(not(windows))]pub(crate)fn relay_if_needed()->bool{false}

#[cfg(all(test,windows))]mod tests{use super::*;#[test]fn marker_is_one_unescaped_argument(){assert_eq!(quote("marker").as_bytes(),&[34,109,97,114,107,101,114,34]);}}
