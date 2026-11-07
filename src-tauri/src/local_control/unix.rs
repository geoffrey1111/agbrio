//! macOS local socket: private directory, lifetime lock and kernel peer UID.
use super::*;
use std::{
    fs::{self, File, OpenOptions},
    os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
};

extern "C" {
    fn geteuid() -> u32;
}
fn uid() -> u32 {
    unsafe { geteuid() }
}
pub(super) fn endpoint() -> PathBuf {
    PathBuf::from("/private/tmp")
        .join(format!("agbrio-control-v1-{}", uid()))
        .join("control.sock")
}

struct Owner {
    path: PathBuf,
    device: u64,
    inode: u64,
    _lock: File,
}
impl Drop for Owner {
    fn drop(&mut self) {
        if fs::symlink_metadata(&self.path).is_ok_and(|m| {
            m.file_type().is_socket() && m.dev() == self.device && m.ino() == self.inode
        }) {
            let _ = fs::remove_file(&self.path);
        }
        // The lock file must stay at the same inode across owners. Unlinking it
        // would let a third process take a different lock while a successor runs.
    }
}
fn bind(path: &Path) -> Result<(UnixListener, Owner), String> {
    let directory = path.parent().ok_or("APP_CONTROL_ENDPOINT_INVALID")?;
    match fs::DirBuilder::new().mode(0o700).create(directory) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err("APP_CONTROL_DIRECTORY_UNAVAILABLE".into()),
    }
    let m = fs::symlink_metadata(directory).map_err(|_| "APP_CONTROL_DIRECTORY_UNAVAILABLE")?;
    if !m.is_dir() || m.file_type().is_symlink() || m.uid() != uid() || m.mode() & 0o777 != 0o700 {
        return Err("APP_CONTROL_DIRECTORY_UNTRUSTED".into());
    }
    // O_NOFOLLOW is Darwin's native value. This module is never compiled for
    // another Unix target. Files are private even with a permissive umask.
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(0x100)
        .open(directory.join("listener.lock"))
        .map_err(|_| "APP_CONTROL_LOCK_UNAVAILABLE")?;
    let m = lock
        .metadata()
        .map_err(|_| "APP_CONTROL_LOCK_UNAVAILABLE")?;
    if !m.is_file() || m.uid() != uid() || m.mode() & 0o777 != 0o600 || m.nlink() != 1 {
        return Err("APP_CONTROL_LOCK_UNTRUSTED".into());
    }
    lock.try_lock().map_err(|_| "APP_CONTROL_ALREADY_OWNED")?;
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_socket() && m.uid() == uid() && m.mode() & 0o777 == 0o600 => {
            // Holding the lifetime lock proves no cooperating Host owns this
            // stale socket; never remove an active foreign listener.
            match std::os::unix::net::UnixStream::connect(path) {
                Ok(_) => return Err("APP_CONTROL_ALREADY_OWNED".into()),
                Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => {}
                Err(_) => return Err("APP_CONTROL_ENDPOINT_UNCONFIRMED".into()),
            }
            fs::remove_file(path).map_err(|_| "APP_CONTROL_STALE_ENDPOINT_UNAVAILABLE")?;
        }
        Ok(_) => return Err("APP_CONTROL_ENDPOINT_UNTRUSTED".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("APP_CONTROL_ENDPOINT_UNAVAILABLE".into()),
    }
    let listener = UnixListener::bind(path).map_err(|_| "APP_CONTROL_LISTENER_UNAVAILABLE")?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|_| "APP_CONTROL_SECURITY_UNAVAILABLE")?;
    let m = fs::symlink_metadata(path).map_err(|_| "APP_CONTROL_ENDPOINT_UNAVAILABLE")?;
    Ok((
        listener,
        Owner {
            path: path.into(),
            device: m.dev(),
            inode: m.ino(),
            _lock: lock,
        },
    ))
}
async fn exchange(
    mut stream: UnixStream,
    core: &RouterCore,
    host: &HostRuntime,
    seen: &mut ReplayStore,
) -> Result<bool, ()> {
    if stream.peer_cred().map_err(|_| ())?.uid() != uid() {
        return Err(());
    }
    let mut bytes = Vec::new();
    loop {
        let byte = stream.read_u8().await.map_err(|_| ())?;
        if byte == b'\n' {
            break;
        }
        bytes.push(byte);
        if bytes.len() > 8192 {
            break;
        }
    }
    let (reply, shutdown) = dispatch(core, host, &bytes, seen);
    let mut output = serde_json::to_vec(&reply).map_err(|_| ())?;
    output.push(b'\n');
    stream.write_all(&output).await.map_err(|_| ())?;
    stream.flush().await.map_err(|_| ())?;
    let mut ack = [0; 4];
    stream.read_exact(&mut ack).await.map_err(|_| ())?;
    if &ack != b"ACK\n" {
        return Err(());
    }
    Ok(shutdown)
}
async fn serve_at(
    core: RouterCore,
    host: HostRuntime,
    path: PathBuf,
    on_shutdown: impl FnOnce() + Send + 'static,
) -> Result<(), String> {
    let (listener, _owner) = bind(&path)?;
    let mut seen = ReplayStore::new()?;
    loop {
        let (stream, _) = listener
            .accept()
            .await
            .map_err(|_| "APP_CONTROL_CONNECT_FAILED")?;
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            exchange(stream, &core, &host, &mut seen),
        )
        .await;
        if matches!(result, Ok(Ok(true))) {
            on_shutdown();
            return Ok(());
        }
    }
}
pub(super) async fn serve(
    core: RouterCore,
    host: HostRuntime,
    on_shutdown: impl FnOnce() + Send + 'static,
) -> Result<(), String> {
    serve_at(core, host, endpoint(), on_shutdown).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_control::tests::{fixture, request};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use tokio::io::{AsyncBufReadExt, BufReader};
    fn directory() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("agbrio-ipc-")
            .tempdir_in("/private/tmp")
            .unwrap()
    }
    async fn call(path: &Path, request: &[u8], ack: bool) -> Value {
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(3);
        let mut stream = loop {
            if let Ok(s) = UnixStream::connect(path).await {
                break BufReader::new(s);
            }
            assert!(std::time::Instant::now()<deadline,"owned socket did not become ready");
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        };
        stream.get_mut().write_all(request).await.unwrap();
        stream.get_mut().write_all(b"\n").await.unwrap();
        let mut line = String::new();
        stream.read_line(&mut line).await.unwrap();
        if ack {
            stream.get_mut().write_all(b"ACK\n").await.unwrap();
            let mut end = String::new();
            stream.read_line(&mut end).await.unwrap();
        }
        serde_json::from_str(&line).unwrap()
    }
    #[test]
    fn ownership_rejects_unsafe_directories_symlinks_and_second_writers() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let _enter = rt.enter();
        let temp = directory();
        let path = temp.path().join("private/control.sock");
        let (_listener, owner) = bind(&path).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
        assert_eq!(bind(&path).err().unwrap(), "APP_CONTROL_ALREADY_OWNED");
        drop(owner);
        assert!(!path.exists());
        fs::write(&path, b"foreign file").unwrap();
        assert_eq!(bind(&path).err().unwrap(), "APP_CONTROL_ENDPOINT_UNTRUSTED");
        fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(temp.path(), &path).unwrap();
        assert_eq!(bind(&path).err().unwrap(), "APP_CONTROL_ENDPOINT_UNTRUSTED");
        fs::set_permissions(path.parent().unwrap(), fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            bind(&path).err().unwrap(),
            "APP_CONTROL_DIRECTORY_UNTRUSTED"
        );
    }
    #[test]
    fn only_a_refused_owned_stale_socket_can_be_replaced() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let _enter = rt.enter();
        let temp = directory();
        let path = temp.path().join("private/control.sock");
        fs::DirBuilder::new()
            .mode(0o700)
            .create(path.parent().unwrap())
            .unwrap();
        let stale = std::os::unix::net::UnixListener::bind(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        drop(stale);
        let (_listener, owner) = bind(&path).unwrap();
        assert!(path.exists());
        drop(owner);
        assert!(!path.exists());
        let lock = path.parent().unwrap().join("listener.lock");
        fs::remove_file(&lock).unwrap();
        std::os::unix::fs::symlink(temp.path().join("foreign"), &lock).unwrap();
        assert_eq!(bind(&path).err().unwrap(), "APP_CONTROL_LOCK_UNAVAILABLE");
        assert!(!temp.path().join("foreign").exists());
    }
    #[test]
    fn bundled_node_client_exchanges_with_the_real_rust_socket() {
        let (_data, core, host) = fixture();
        let temp = directory();
        let path = temp.path().join("private/control.sock");
        tokio::runtime::Runtime::new().unwrap().block_on(async{
            // The production Tauri state keeps blocking HTTP clients outside
            // Tokio. Retain the fixture's last Arc in that same outer scope.
            let address=path.clone();let instance=host.instance_id().to_string();let server=tokio::spawn(serve_at(core.clone(),host.clone(),address,||{}));
            for _ in 0..100{if path.exists(){break}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}
            let module=Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("tools/agbrio-control/control.mjs");let module=url::Url::from_file_path(module).unwrap().to_string();
            let result=tokio::task::spawn_blocking(move||std::process::Command::new("node").args(["--input-type=module","-e",r#"const {exchange}=await import(process.argv[1]);const [socket,instance]=process.argv.slice(2);const s=await exchange(socket,'STATUS');if(s.instanceId!==instance)throw Error('wrong host');await exchange(socket,'SHUTDOWN',{expectedInstance:instance});console.log('NATIVE_SOCKET_CLIENT_PASS');"#,&module,path.to_str().unwrap(),&instance]).output().unwrap()).await.unwrap();
            assert!(result.status.success(),"{}",String::from_utf8_lossy(&result.stderr));assert!(String::from_utf8_lossy(&result.stdout).contains("NATIVE_SOCKET_CLIENT_PASS"));
            tokio::time::timeout(std::time::Duration::from_secs(5),server).await.unwrap().unwrap().unwrap();
        });
    }
    #[test]
    fn kernel_socket_transport_preserves_ack_replay_and_epoch_fences() {
        let (_data, core, host) = fixture();
        let temp = directory();
        let path = temp.path().join("private/control.sock");
        let calls = Arc::new(AtomicUsize::new(0));
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let count = calls.clone();
            let address = path.clone();
            let owned = host.clone();
            let server = tokio::spawn(serve_at(core, owned, address, move || {
                count.fetch_add(1, Ordering::SeqCst);
            }));

            for _ in 0..16 {
                assert_eq!(
                    call(&path, &request("STATUS", None), true).await["data"]["instanceId"],
                    host.instance_id()
                );
            }
            assert_eq!(
                call(&path, &request("SHUTDOWN", Some("old")), true).await["errorCode"],
                "APP_CONTROL_INSTANCE_CHANGED"
            );
            let abandoned = request("SHUTDOWN", Some(host.instance_id()));
            let _ = call(&path, &abandoned, false).await;
            let _ = call(&path, &abandoned, true).await;
            assert_eq!(calls.load(Ordering::SeqCst), 0);
            let _ = call(&path, &request("SHUTDOWN", Some(host.instance_id())), true).await;
            tokio::time::timeout(std::time::Duration::from_secs(5), server)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            assert!(!path.exists());
        });
    }
    #[test]
    fn more_than_128_real_socket_receipts_preserve_unacknowledged_shutdown_fences() {
        let (_data, core, host) = fixture();
        let temp = directory();
        let path = temp.path().join("private/control.sock");
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let count=calls.clone();let address=path.clone();
            let server=tokio::spawn(serve_at(core.clone(),host.clone(),address,move||{count.fetch_add(1,std::sync::atomic::Ordering::SeqCst);}));
            for _ in 0..100 {if path.exists(){break}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}
            let first=request("SHUTDOWN",Some(host.instance_id()));
            for n in 0..512 {
                let bytes=if n==0{first.clone()}else{request("SHUTDOWN",Some(host.instance_id()))};
                let reply=call(&path,&bytes,false).await;
                assert!(reply.get("errorCode").is_none(),"receipt {n}: {reply}");
            }
            assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst),0);
            // ACKing an old result must not admit its shutdown again.
            let replay=call(&path,&first,true).await;assert!(replay.get("errorCode").is_none());
            assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst),0);
            call(&path,&request("SHUTDOWN",Some(host.instance_id())),true).await;
            tokio::time::timeout(std::time::Duration::from_secs(5),server).await.unwrap().unwrap().unwrap();
            assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst),1);
            assert!(!path.exists());
        });
    }

}
