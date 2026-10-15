use super::{
    archive::StagedUpdate,
    transaction::{InstallLayout, Transaction, recover},
};
use anyhow::{Context, Result, bail, ensure};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

static LIFETIME_PIPES: OnceLock<Mutex<Vec<ChildStdin>>> = OnceLock::new();
#[derive(Serialize, Deserialize)]
struct UpdateJob {
    layout: InstallLayout,
    staged: Option<StagedUpdate>,
    parent: u32,
    ready_address: String,
    nonce: String,
}
pub struct ReadyHelper {
    _child: Child,
}
fn nonce() -> String {
    hex::encode(rand::random::<[u8; 32]>())
}
fn listener() -> Result<TcpListener> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    Ok(listener)
}
fn receive(
    listener: &TcpListener,
    expected: &str,
    timeout: Duration,
    alive: impl FnMut() -> Result<bool>,
) -> Result<()> {
    receive_with(listener, expected, timeout, alive, || Ok(()))
}
fn receive_with(
    listener: &TcpListener,
    expected: &str,
    timeout: Duration,
    mut alive: impl FnMut() -> Result<bool>,
    mut verified: impl FnMut() -> Result<()>,
) -> Result<()> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        ensure!(alive()?, "Updater process exited before acknowledgement");
        match listener.accept() {
            Ok((mut stream, address)) => {
                ensure!(
                    address.ip().is_loopback(),
                    "Unexpected acknowledgement peer"
                );
                stream.set_read_timeout(Some(Duration::from_millis(200)))?;
                let mut data = vec![0; expected.len()];
                if stream.read_exact(&mut data).is_ok() && data == expected.as_bytes() {
                    verified()?;
                    stream.write_all(b"ok")?;
                    return Ok(());
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(25))
            }
            Err(e) => return Err(e.into()),
        }
    }
    bail!("Updater acknowledgement timed out")
}
fn acknowledge(address: &str, token: &str) -> Result<()> {
    let address: std::net::SocketAddr = address.parse()?;
    ensure!(
        address.ip().is_loopback() && token.len() == 64 && hex::decode(token).is_ok(),
        "Invalid update acknowledgement"
    );
    let mut socket = TcpStream::connect_timeout(&address, Duration::from_secs(5))?;
    socket.set_read_timeout(Some(Duration::from_secs(70)))?;
    socket.set_write_timeout(Some(Duration::from_secs(5)))?;
    socket.write_all(token.as_bytes())?;
    let mut ok = [0; 2];
    socket.read_exact(&mut ok)?;
    ensure!(&ok == b"ok", "Acknowledgement rejected");
    Ok(())
}
pub fn acknowledge_health() -> Result<()> {
    if let (Ok(address), Ok(token)) = (
        std::env::var("PICORA_UPDATE_HEALTH_ADDRESS"),
        std::env::var("PICORA_UPDATE_HEALTH_TOKEN"),
    ) {
        acknowledge(&address, &token)?;
    }
    Ok(())
}
pub fn is_health_launch() -> bool {
    std::env::var_os("PICORA_UPDATE_HEALTH_TOKEN").is_some()
}

/// The caller must keep `directory` until the helper has acknowledged readiness.
/// On success ownership of staging passes to the helper.
pub fn launch_helper(
    layout: InstallLayout,
    staged: StagedUpdate,
    directory: &Path,
) -> Result<ReadyHelper> {
    launch_job(layout, Some(staged), directory)
}
fn launch_job(
    layout: InstallLayout,
    staged: Option<StagedUpdate>,
    directory: &Path,
) -> Result<ReadyHelper> {
    let listener = listener()?;
    let token = nonce();
    let helper = directory.join(if cfg!(windows) {
        "updater.exe"
    } else {
        "updater"
    });
    fs::copy(std::env::current_exe()?, &helper)?;
    let job = UpdateJob {
        layout,
        staged,
        parent: std::process::id(),
        ready_address: listener.local_addr()?.to_string(),
        nonce: token.clone(),
    };
    let job_path = directory.join("job.json");
    fs::write(&job_path, serde_json::to_vec(&job)?)?;
    let mut child = Command::new(helper)
        .arg("--update-helper")
        .arg(job_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    if let Err(e) = receive(&listener, &token, Duration::from_secs(15), || {
        Ok(child.try_wait()?.is_none())
    }) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(e);
    }
    let pipe = child
        .stdin
        .take()
        .context("Missing updater lifetime pipe")?;
    LIFETIME_PIPES
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| anyhow::anyhow!("Updater lifetime lock poisoned"))?
        .push(pipe);
    Ok(ReadyHelper { _child: child })
}
#[cfg(windows)]
struct Parent(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
impl Parent {
    fn open(pid: u32) -> Result<Self> {
        use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE};
        // SAFETY: obtains an owned synchronize-only handle for the exact process.
        let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        ensure!(!handle.is_null(), "Cannot wait for Picora to exit");
        Ok(Self(handle))
    }
    fn wait(&self) -> Result<()> {
        // SAFETY: handle remains valid until Drop; finite timeout.
        let result =
            unsafe { windows_sys::Win32::System::Threading::WaitForSingleObject(self.0, 30_000) };
        ensure!(result == 0, "Picora did not exit within 30 seconds");
        Ok(())
    }
}
#[cfg(windows)]
impl Drop for Parent {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}
#[cfg(not(windows))]
fn wait_pipe() -> Result<()> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut byte = [0];
        let _ = tx.send(std::io::stdin().read(&mut byte));
    });
    ensure!(
        rx.recv_timeout(Duration::from_secs(30))?? == 0,
        "Invalid updater lifetime signal"
    );
    Ok(())
}
pub fn run_helper(job_path: &Path) -> Result<()> {
    ensure!(
        fs::metadata(job_path)?.len() <= 1024 * 1024,
        "Update job too large"
    );
    let directory = job_path
        .parent()
        .context("Invalid update job location")?
        .canonicalize()?;
    let lease = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(directory.join("lease"))?;
    lease.try_lock_exclusive()?;
    let job: UpdateJob = serde_json::from_slice(&fs::read(job_path)?)?;
    if let Some(staged) = &job.staged {
        ensure!(
            staged.root.canonicalize()? == directory.join("staged")
                && staged.target == job.layout.target,
            "Invalid staging location"
        );
    }
    ensure!(
        InstallLayout::for_root(job.layout.root.clone(), job.layout.target)?.root
            == job.layout.root,
        "Invalid installation location"
    );
    let _gate = job.layout.startup_gate()?;
    // Validate writable destination before asking the desktop to exit.
    let state = job.layout.state_dir()?;
    fs::create_dir_all(&state)?;
    let probe = tempfile::NamedTempFile::new_in(&state)?;
    drop(probe);
    #[cfg(windows)]
    let parent = Parent::open(job.parent)?;
    acknowledge(&job.ready_address, &job.nonce)?;
    #[cfg(windows)]
    parent.wait()?;
    #[cfg(not(windows))]
    wait_pipe()?;
    let outcome = if let Some(staged) = &job.staged {
        apply_and_restart(&job.layout, staged)
    } else {
        recover(&job.layout).and_then(|()| {
            // Recovery is finished; the restored child must be able to acquire its startup gate.
            fs2::FileExt::unlock(&_gate)?;
            Command::new(job.layout.executable())
                .arg("--library")
                .env_remove("PICORA_UPDATE_HEALTH_ADDRESS")
                .env_remove("PICORA_UPDATE_HEALTH_TOKEN")
                .spawn()?;
            Ok(())
        })
    };
    if let Err(e) = &outcome {
        let _ = fs::write(
            state.join("last-error.txt"),
            format!("Update failed: {e:#}"),
        );
        // A failed rollback is not hidden by starting a potentially mixed install.
        if recover(&job.layout).is_ok() {
            let _ = fs2::FileExt::unlock(&_gate);
            let _ = Command::new(job.layout.executable())
                .env_remove("PICORA_UPDATE_HEALTH_ADDRESS")
                .env_remove("PICORA_UPDATE_HEALTH_TOKEN")
                .arg("--library")
                .spawn();
        }
    }
    // The running helper cannot delete itself on Windows. Its private directory
    // is left for cleanup on the next normal app launch.
    #[cfg(not(windows))]
    let _ = fs::remove_dir_all(&directory);
    outcome
}
fn apply_and_restart(layout: &InstallLayout, staged: &StagedUpdate) -> Result<()> {
    recover(layout)?;
    let mut transaction = Transaction::prepare(layout, staged)?;
    if let Err(e) = transaction.apply() {
        transaction.rollback()?;
        return Err(e);
    }
    let listener = listener()?;
    let token = nonce();
    let child = Command::new(layout.executable())
        .arg("--library")
        .env(
            "PICORA_UPDATE_HEALTH_ADDRESS",
            listener.local_addr()?.to_string(),
        )
        .env("PICORA_UPDATE_HEALTH_TOKEN", &token)
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            transaction.rollback()?;
            return Err(e.into());
        }
    };
    let result = receive_with(
        &listener,
        &token,
        Duration::from_secs(60),
        || Ok(child.try_wait()?.is_none()),
        || {
            // Commit before telling the replacement it may stay running. If the
            // helper dies earlier, the replacement's failed handshake closes it.
            match transaction.commit() {
                Ok(()) => Ok(()),
                Err(e) if layout.state_dir()?.join("committed").exists() => {
                    let _ = fs::write(
                        layout.state_dir()?.join("last-error.txt"),
                        format!("Update installed; cleanup will retry: {e:#}"),
                    );
                    Ok(())
                }
                Err(e) => Err(e),
            }
        },
    );
    if let Err(e) = result {
        let _ = child.kill();
        let _ = child.wait();
        transaction.rollback()?;
        return Err(e);
    }
    Ok(())
}

pub struct Startup {
    pub restarting: bool,
    _gate: Option<fs::File>,
    _helper: Option<ReadyHelper>,
}
pub fn startup_recovery() -> Result<Startup> {
    let empty = || Startup {
        restarting: false,
        _gate: None,
        _helper: None,
    };
    if is_health_launch() {
        return Ok(empty());
    }
    let Ok(layout) = InstallLayout::detect(&std::env::current_exe()?) else {
        return Ok(empty());
    };
    if !layout.state_dir()?.exists() {
        return Ok(empty());
    }
    let gate = layout.startup_gate()?;
    if layout.state_dir()?.join("journal.json").exists() {
        drop(gate);
        let directory = tempfile::Builder::new()
            .prefix("picora-update-recover-")
            .tempdir()?;
        let helper = launch_job(layout, None, directory.path())?;
        let _ = directory.keep();
        return Ok(Startup {
            restarting: true,
            _gate: None,
            _helper: Some(helper),
        });
    }
    cleanup_staging(&layout);
    Ok(Startup {
        restarting: false,
        _gate: Some(gate),
        _helper: None,
    })
}

fn cleanup_staging(layout: &InstallLayout) {
    let Ok(entries) = fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with("picora-update-")
            || !entry.file_type().is_ok_and(|t| t.is_dir())
        {
            continue;
        }
        let path = entry.path();
        let Ok(bytes) = fs::read(path.join("job.json")) else {
            continue;
        };
        if bytes.len() > 1024 * 1024 {
            continue;
        }
        let Ok(job) = serde_json::from_slice::<UpdateJob>(&bytes) else {
            continue;
        };
        if job.layout.root != layout.root {
            continue;
        }
        let Ok(lease) = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path.join("lease"))
        else {
            continue;
        };
        if lease.try_lock_exclusive().is_ok() {
            let _ = fs::remove_dir_all(path);
        }
    }
}
pub fn last_failure() -> Option<String> {
    let layout = InstallLayout::detect(&std::env::current_exe().ok()?).ok()?;
    let path = layout.state_dir().ok()?.join("last-error.txt");
    if fs::metadata(&path).ok()?.len() > 64 * 1024 {
        return None;
    }
    fs::read_to_string(path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authenticated_handshake_and_timeout() {
        let socket = listener().unwrap();
        let address = socket.local_addr().unwrap().to_string();
        let token = nonce();
        let other = token.clone();
        let thread = std::thread::spawn(move || acknowledge(&address, &other));
        receive(&socket, &token, Duration::from_secs(1), || Ok(true)).unwrap();
        thread.join().unwrap().unwrap();
        assert!(receive(&socket, &token, Duration::from_millis(30), || Ok(true)).is_err());
    }
}
