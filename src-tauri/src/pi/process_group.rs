//! Standard-user macOS inherited process-group ownership. Not an OS sandbox.
//! Keep the leader unreaped until descendants settle, so its PID/PGID cannot
//! be recycled into an unrelated process group during cleanup.
use std::{
    process::{Child, Command},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
pub(super) fn configure(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}
fn info(pid: i32) -> Option<libc::proc_bsdinfo> {
    let mut value: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let count = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut value as *mut libc::proc_bsdinfo).cast(),
            std::mem::size_of_val(&value) as i32,
        )
    };
    (count as usize == std::mem::size_of_val(&value)).then_some(value)
}
pub(super) struct OwnedGroup {
    leader: i32,
    uid: u32,
    born: (u64, u64),
    active: AtomicBool,
}
impl OwnedGroup {
    pub fn attach(child: &Child) -> Result<Self, String> {
        let pid = i32::try_from(child.id()).map_err(|_| "PI_GROUP_IDENTITY_UNAVAILABLE")?;
        let value = info(pid).ok_or("PI_GROUP_IDENTITY_UNAVAILABLE")?;
        if value.pbi_uid != unsafe { libc::geteuid() }
            || value.pbi_ppid != std::process::id()
            || value.pbi_pgid != pid as u32
        {
            return Err("PI_GROUP_IDENTITY_MISMATCH".into());
        }
        Ok(Self {
            leader: pid,
            uid: value.pbi_uid,
            born: (value.pbi_start_tvsec, value.pbi_start_tvusec),
            active: AtomicBool::new(true),
        })
    }
    fn leader(&self) -> Result<libc::proc_bsdinfo, String> {
        let value = info(self.leader).ok_or("PI_CLOSE_UNCONFIRMED")?;
        if value.pbi_uid != self.uid || (value.pbi_start_tvsec, value.pbi_start_tvusec) != self.born
        {
            return Err("PI_GROUP_IDENTITY_CHANGED".into());
        }
        Ok(value)
    }
    pub fn leader_exited(&self) -> Result<bool, String> {
        let mut result: libc::siginfo_t = unsafe { std::mem::zeroed() };
        if unsafe {
            libc::waitid(
                libc::P_PID,
                self.leader as libc::id_t,
                &mut result,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        } != 0
        {
            return Err("PI_CLOSE_UNCONFIRMED".into());
        }
        if result.si_pid == self.leader {
            return Ok(true);
        }
        let _ = self.leader()?;
        Ok(false)
    }
    fn live_members(&self) -> Result<usize, String> {
        unsafe {
            *libc::__error() = 0;
        }
        let count = unsafe { libc::proc_listpgrppids(self.leader, std::ptr::null_mut(), 0) };
        if count == 0 {
            let error = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
            return if error == 0 || error == libc::ESRCH {
                Ok(0)
            } else {
                Err("PI_CLOSE_UNCONFIRMED".into())
            };
        }
        if count < 0 || count > 100000 {
            return Err("PI_CLOSE_UNCONFIRMED".into());
        }
        let mut pids = vec![0_i32; count as usize + 128];
        unsafe {
            *libc::__error() = 0;
        }
        let count = unsafe {
            libc::proc_listpgrppids(
                self.leader,
                pids.as_mut_ptr().cast(),
                (pids.len() * std::mem::size_of::<i32>()) as i32,
            )
        };
        if count < 0 || count as usize >= pids.len() {
            return Err("PI_CLOSE_UNCONFIRMED".into());
        }
        if count == 0 {
            let error = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
            if error != 0 && error != libc::ESRCH {
                return Err("PI_CLOSE_UNCONFIRMED".into());
            }
        }
        let mut live = 0;
        for pid in pids.into_iter().take(count as usize) {
            if let Some(value) = info(pid) {
                if value.pbi_pgid == self.leader as u32 {
                    if value.pbi_uid != self.uid {
                        return Err("PI_GROUP_OWNER_CHANGED".into());
                    }
                    if value.pbi_status != libc::SZOMB {
                        live += 1;
                    }
                }
            } else {
                let group = unsafe { libc::getpgid(pid) };
                if group == self.leader
                    || group < 0
                        && std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
                {
                    return Err("PI_CLOSE_UNCONFIRMED".into());
                }
            }
        }
        Ok(live)
    }
    pub fn terminate_and_wait(&self, timeout: Duration) -> Result<(), String> {
        if !self.active.load(Ordering::Acquire) {
            return Ok(());
        }
        if !self.leader_exited()? {
            let _ = self.leader()?;
        }
        let _ = self.live_members()?;
        if unsafe { libc::kill(-self.leader, libc::SIGKILL) } != 0
            && std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
        {
            return Err("PI_CLOSE_UNCONFIRMED".into());
        }
        let deadline = Instant::now() + timeout;
        loop {
            if self.live_members()? == 0 {
                self.active.store(false, Ordering::Release);
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err("PI_CLOSE_UNCONFIRMED".into());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
impl Drop for OwnedGroup {
    fn drop(&mut self) {
        if self.active.load(Ordering::Acquire) {
            let _ = self.terminate_and_wait(Duration::from_secs(3));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader, Write},
        process::Stdio,
    };
    struct Process(Child);
    impl Drop for Process {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    #[test]
    fn inherited_descendants_are_reaped_without_touching_same_image_outside_the_group() {
        let mut protected = Process(Command::new("/bin/sleep").arg("60").spawn().unwrap());
        let mut command = Command::new("/bin/sh");
        command
            .args([
                "-c",
                "read start; /bin/sleep 60 & printf '%s\\n' \"$!\"; wait",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        configure(&mut command);
        let mut parent = Process(command.spawn().unwrap());
        let group = OwnedGroup::attach(&parent.0).unwrap();
        parent
            .0
            .stdin
            .as_mut()
            .unwrap()
            .write_all(b"start\n")
            .unwrap();
        let mut pid = String::new();
        BufReader::new(parent.0.stdout.take().unwrap())
            .read_line(&mut pid)
            .unwrap();
        let descendant: i32 = pid.trim().parse().unwrap();
        assert_eq!(info(descendant).unwrap().pbi_pgid, parent.0.id());
        assert_ne!(
            unsafe { libc::getpgid(protected.0.id() as i32) },
            parent.0.id() as i32
        );
        group.terminate_and_wait(Duration::from_secs(3)).unwrap();
        parent.0.wait().unwrap();
        assert!(info(descendant).is_none_or(|p| p.pbi_status == libc::SZOMB));
        assert!(protected.0.try_wait().unwrap().is_none());
    }
    #[test]
    fn a_naturally_exited_leader_stays_unreaped_until_its_group_settles() {
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", "read start; /bin/sleep 60 & exit 0"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        configure(&mut command);
        let mut child = Process(command.spawn().unwrap());
        let group = OwnedGroup::attach(&child.0).unwrap();
        child
            .0
            .stdin
            .as_mut()
            .unwrap()
            .write_all(b"start\n")
            .unwrap();
        let until = Instant::now() + Duration::from_secs(3);
        while !group.leader_exited().unwrap() {
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(10));
        }
        group.terminate_and_wait(Duration::from_secs(3)).unwrap();
        child.0.wait().unwrap();
    }
}
