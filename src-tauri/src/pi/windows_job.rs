//! One owned Job per Pi process; closing it collects that task's descendants.
#[cfg(windows)]
pub(super) struct OwnedJob(windows::Win32::Foundation::HANDLE);
#[cfg(windows)]
unsafe impl Send for OwnedJob {}
#[cfg(windows)]
unsafe impl Sync for OwnedJob {}
#[cfg(windows)]
impl OwnedJob {
    pub fn terminate_and_wait(&self,timeout:std::time::Duration)->Result<(),String>{
        use windows::Win32::System::JobObjects::*;
        unsafe{TerminateJobObject(self.0,0).map_err(|_|"PI_CLOSE_UNCONFIRMED")?;}
        let until=std::time::Instant::now()+timeout;
        loop{
            let mut info=JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
            unsafe{QueryInformationJobObject(Some(self.0),JobObjectBasicAccountingInformation,(&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),std::mem::size_of_val(&info)as u32,None).map_err(|_|"PI_CLOSE_UNCONFIRMED")?;}
            if info.ActiveProcesses==0{return Ok(());}if std::time::Instant::now()>=until{return Err("PI_CLOSE_UNCONFIRMED".into());}std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
    pub fn attach(child: &std::process::Child) -> Result<Self, String> {
        use std::os::windows::io::AsRawHandle;
        use windows::{
            core::PCWSTR,
            Win32::{
                Foundation::{CloseHandle, HANDLE},
                System::JobObjects::*,
            },
        };
        unsafe {
            let job = CreateJobObjectW(None, PCWSTR::null()).map_err(|_| "PI_JOB_CREATE_FAILED")?;
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of_val(&info) as u32,
            )
            .is_err()
                || AssignProcessToJobObject(job, HANDLE(child.as_raw_handle())).is_err()
            {
                let _ = CloseHandle(job);
                return Err("PI_JOB_ASSIGN_FAILED".into());
            }
            Ok(Self(job))
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::{
        io::Write,
        os::windows::{io::AsRawHandle, process::CommandExt},
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    struct Handle(windows::Win32::Foundation::HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                let _ = windows::Win32::Foundation::CloseHandle(self.0);
            }
        }
    }
    #[test]
    #[ignore = "owned real Windows process-tree lifecycle; no model, no foreign termination"]
    fn closing_job_reaps_owned_descendant_and_preserves_same_image_outside_job() {
        use windows::{
            core::BOOL,
            Win32::{
                Foundation::{HANDLE, WAIT_OBJECT_0},
                System::{
                    JobObjects::IsProcessInJob,
                    Threading::{
                        OpenProcess, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION,
                        PROCESS_SYNCHRONIZE,
                    },
                },
            },
        };
        let root =
            std::path::PathBuf::from(std::env::var_os("AGBRIO_PI_QA_ROOT").expect("owned QA root"));
        assert!(root.is_absolute() && root.is_dir());
        let marker = root.join(format!("owned-job-{}.json", uuid::Uuid::new_v4()));
        let node = super::super::installation().unwrap().node;
        let mut protected = Child(
            Command::new(&node)
                .args(["-e", "setInterval(()=>{},1000)"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(0x08000000)
                .spawn()
                .unwrap(),
        );
        let script="const fs=require('node:fs'),cp=require('node:child_process');process.stdin.once('data',()=>{const child=cp.spawn(process.execPath,['-e','setInterval(()=>{},1000)'],{stdio:'ignore',windowsHide:true});fs.writeFileSync(process.argv[1],JSON.stringify({child:child.pid}));setInterval(()=>{},1000);})";
        let mut parent = Child(
            Command::new(&node)
                .args(["-e", script])
                .arg(&marker)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(0x08000000)
                .spawn()
                .unwrap(),
        );
        let job = OwnedJob::attach(&parent.0).unwrap();
        parent
            .0
            .stdin
            .as_mut()
            .unwrap()
            .write_all(b"start\n")
            .unwrap();
        let until = Instant::now() + Duration::from_secs(5);
        while !marker.is_file() {
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(20));
        }
        let record: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&marker).unwrap()).unwrap();
        let child = Handle(unsafe {
            OpenProcess(
                PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
                false,
                record["child"].as_u64().unwrap() as u32,
            )
            .unwrap()
        });
        let mut belongs = BOOL::default();
        unsafe {
            IsProcessInJob(child.0, Some(job.0), &mut belongs).unwrap();
        }
        assert!(belongs.as_bool());
        let mut outside = BOOL::default();
        unsafe {
            IsProcessInJob(
                HANDLE(protected.0.as_raw_handle()),
                Some(job.0),
                &mut outside,
            )
            .unwrap();
        }
        assert!(!outside.as_bool());
        drop(job);
        assert_eq!(unsafe { WaitForSingleObject(child.0, 3000) }, WAIT_OBJECT_0);
        let until = Instant::now() + Duration::from_secs(3);
        while parent.0.try_wait().unwrap().is_none() {
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            protected.0.try_wait().unwrap().is_none(),
            "unrelated same-image process must remain alive"
        );
        assert!(marker.starts_with(&root));
        std::fs::remove_file(marker).unwrap();
    }
}
#[cfg(windows)]
impl Drop for OwnedJob {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.0);
        }
    }
}
#[cfg(not(windows))]
pub(super) struct OwnedJob;
#[cfg(not(windows))]
impl OwnedJob {
    pub fn terminate_and_wait(&self,_:std::time::Duration)->Result<(),String>{Err("PI_PLATFORM_UNVERIFIED".into())}
    pub fn attach(_: &std::process::Child) -> Result<Self, String> {
        Err("PI_PLATFORM_UNVERIFIED".into())
    }
}
