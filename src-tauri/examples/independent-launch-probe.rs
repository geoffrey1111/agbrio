#[cfg(windows)]
#[path="../src/independent_startup.rs"]mod independent_startup;
#[cfg(windows)]
fn main(){if independent_startup::relay_if_needed(){return;}let args=std::env::args().collect::<Vec<_>>();let path=std::path::PathBuf::from(args.get(1).expect("owned output"));assert!(path.parent().unwrap().join(".owned-probe").is_file());let mut member=windows::core::BOOL::default();unsafe{windows::Win32::System::JobObjects::IsProcessInJob(windows::Win32::System::Threading::GetCurrentProcess(),None,&mut member).unwrap();}std::fs::write(&path,serde_json::json!({"pid":std::process::id(),"inJob":member.as_bool(),"port":std::env::var("AI_WORK_ROUTER_MOBILE_PORT").unwrap_or_default()}).to_string()).unwrap();let start=std::time::Instant::now();while start.elapsed().as_secs()<20{if path.with_extension("release").exists(){std::fs::write(path.with_extension("alive"),"survived parent job close").unwrap();return;}std::thread::sleep(std::time::Duration::from_millis(100));}}

#[cfg(not(windows))]
fn main(){eprintln!("Windows-only independent launch fixture");std::process::exit(2);}
