//! Headless native process replacement tests, also run by `cargo test`.
use anyhow::{Context, Result, ensure};
use picora::update::{
    archive::StagedUpdate,
    helper,
    release::Target,
    transaction::{InstallLayout, Transaction},
};
use std::{
    fs,
    io::{BufRead, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
fn main() {
    if let Err(e) = run() {
        eprintln!("{e:#}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|x| x == "--test-recovery") {
        return interrupted_recovery();
    }
    if args.get(1).is_some_and(|x| x == "--update-helper") {
        return helper::run_helper(Path::new(&args[2]));
    }
    if args.get(1).is_some_and(|x| x == "--initiate") {
        let layout: InstallLayout = serde_json::from_slice(&fs::read(&args[2])?)?;
        let dir = tempfile::Builder::new()
            .prefix("picora-update-test-")
            .tempdir()?;
        let root = dir.path().join("staged");
        fs::create_dir(&root)?;
        let target = layout.target;
        let rel = match target {
            Target::WindowsX86_64 => "picora.exe",
            Target::LinuxX86_64 => "picora",
            Target::MacosAarch64 => "Picora.app/Contents/MacOS/picora",
        };
        let exe = root.join(rel);
        fs::create_dir_all(exe.parent().unwrap())?;
        fs::copy(std::env::current_exe()?, &exe)?;
        let mut files = vec![PathBuf::from(rel)];
        let version = if target == Target::MacosAarch64 {
            "Picora.app/version.txt"
        } else {
            "version.txt"
        };
        fs::write(root.join(version), "new")?;
        files.push(version.into());
        let staged = StagedUpdate {
            root,
            files,
            target,
        };
        let _ready = helper::launch_helper(layout, staged, dir.path())?;
        let _ = dir.keep();
        println!("ready");
        std::io::stdout().flush()?;
        let mut release = [0];
        std::io::stdin().read_exact(&mut release)?;
        return Ok(());
    }
    if args.get(1).is_some_and(|x| x == "--library") {
        let startup = helper::startup_recovery()?;
        if startup.restarting {
            return Ok(());
        }
        drop(startup);
        let exe = std::env::current_exe()?;
        let layout = InstallLayout::detect(&exe)?;
        let version = fs::read_to_string(layout.root.join("version.txt"))?;
        if version == "new" && std::env::var_os("PICORA_TEST_CRASH").is_some() {
            std::process::exit(9);
        }
        if version == "new" && std::env::var_os("PICORA_TEST_HANG").is_some() {
            loop {
                std::thread::sleep(Duration::from_secs(1));
            }
        }
        helper::acknowledge_health()?;
        publish_readiness(&layout.root, &version)?;
        return Ok(());
    }
    for mode in ["healthy", "crash", "hang"] {
        scenario(mode)?;
    }
    interrupted_recovery()?;
    println!(
        "native updater process tests passed (replacement, parent wait, health acknowledgement, crash/hang rollback, external recovery)"
    );
    Ok(())
}
fn scenario(mode: &str) -> Result<()> {
    let temp = tempfile::tempdir()?;
    let target = Target::current().context("Unsupported CI target")?;
    let root = temp.path().join(if target == Target::MacosAarch64 {
        "space 한글/Picora.app"
    } else {
        "space 한글/Picora"
    });
    fs::create_dir_all(&root)?;
    let layout = InstallLayout::for_root(&root, target)?;
    fs::create_dir_all(layout.executable().parent().unwrap())?;
    fs::copy(std::env::current_exe()?, layout.executable())?;
    fs::write(layout.root.join("version.txt"), "old")?;
    let job = temp.path().join("layout.json");
    fs::write(&job, serde_json::to_vec(&layout)?)?;
    let mut cmd = Command::new(std::env::current_exe()?);
    cmd.arg("--initiate")
        .arg(job)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    if mode == "crash" {
        cmd.env("PICORA_TEST_CRASH", "1");
    }
    if mode == "hang" {
        cmd.env("PICORA_TEST_HANG", "1");
    }
    let mut parent = cmd.spawn()?;
    let stdout = parent.stdout.take().context("Missing parent stdout")?;
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let result = std::io::BufReader::new(stdout)
            .read_line(&mut line)
            .map(|_| line);
        let _ = sender.send(result);
    });
    let readiness = receiver
        .recv_timeout(Duration::from_secs(20))
        .context("Parent readiness timed out")??;
    ensure!(
        readiness.trim() == "ready",
        "Parent did not complete handoff"
    );
    ensure!(
        fs::read_to_string(layout.root.join("version.txt"))? == "old",
        "Replaced before parent exit"
    );
    parent
        .stdin
        .take()
        .context("Missing parent stdin")?
        .write_all(b"1")?;
    ensure!(parent.wait()?.success(), "Updater handoff failed");
    let ready = wait_readiness(&layout.root, Duration::from_secs(80))?;
    let expected = if mode == "healthy" { "new" } else { "old" };
    ensure!(
        ready.version == expected,
        "{mode}: expected {expected}, restarted {:?}; updater: {}",
        ready.version,
        fs::read_to_string(layout.state_dir()?.join("last-error.txt")).unwrap_or_default()
    );
    wait_exit(ready.pid)?;
    // Wait for helper commit and fixture child exit before removing its files (Windows).
    let start = Instant::now();
    while layout.state_dir()?.join("journal.json").exists() {
        ensure!(
            start.elapsed() < Duration::from_secs(10),
            "Transaction not finalized"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(())
}

fn interrupted_recovery() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let target = Target::current().context("Unsupported target")?;
    let root = temp.path().join(if target == Target::MacosAarch64 {
        "Picora.app"
    } else {
        "Picora"
    });
    fs::create_dir_all(&root)?;
    let layout = InstallLayout::for_root(&root, target)?;
    fs::create_dir_all(layout.executable().parent().unwrap())?;
    fs::copy(std::env::current_exe()?, layout.executable())?;
    fs::write(layout.root.join("version.txt"), "old")?;
    let stage = temp.path().join("staged");
    let rel = match target {
        Target::WindowsX86_64 => "picora.exe",
        Target::LinuxX86_64 => "picora",
        Target::MacosAarch64 => "Picora.app/Contents/MacOS/picora",
    };
    fs::create_dir_all(stage.join(rel).parent().unwrap())?;
    fs::copy(std::env::current_exe()?, stage.join(rel))?;
    let version = if target == Target::MacosAarch64 {
        "Picora.app/version.txt"
    } else {
        "version.txt"
    };
    fs::write(stage.join(version), "new")?;
    let staged = StagedUpdate {
        root: stage,
        files: vec![rel.into(), version.into()],
        target,
    };
    {
        let mut transaction = Transaction::prepare(&layout, &staged)?;
        transaction.apply()?;
    }
    ensure!(
        fs::read_to_string(layout.root.join("version.txt"))? == "new",
        "Fixture not updated"
    );
    let mut first = Command::new(layout.executable()).arg("--library").spawn()?;
    let first_pid = first.id();
    ensure!(first.wait()?.success(), "Recovery handoff failed");
    let ready = wait_readiness(&layout.root, Duration::from_secs(20))?;
    ensure!(
        ready.version == "old",
        "Recovery continued running new executable"
    );
    ensure!(
        ready.pid != first_pid,
        "Recovery must restart in a separate process"
    );
    wait_exit(ready.pid)?;
    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Readiness {
    version: String,
    pid: u32,
}
fn publish_readiness(root: &Path, version: &str) -> Result<()> {
    let mut file = tempfile::NamedTempFile::new_in(root)?;
    serde_json::to_writer(
        &mut file,
        &Readiness {
            version: version.into(),
            pid: std::process::id(),
        },
    )?;
    file.flush()?;
    file.persist(root.join("ready.json"))?;
    Ok(())
}
fn wait_readiness(root: &Path, timeout: Duration) -> Result<Readiness> {
    let start = Instant::now();
    loop {
        match fs::read(root.join("ready.json")) {
            Ok(bytes) => return Ok(serde_json::from_slice(&bytes)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
        ensure!(start.elapsed() < timeout, "Child did not publish readiness");
        std::thread::sleep(Duration::from_millis(25));
    }
}
#[cfg(unix)]
fn wait_exit(pid: u32) -> Result<()> {
    let start = Instant::now();
    loop {
        // SAFETY: signal zero probes the process without sending a signal.
        if unsafe { libc::kill(i32::try_from(pid)?, 0) } != 0 {
            let error = std::io::Error::last_os_error();
            ensure!(
                error.raw_os_error() == Some(libc::ESRCH),
                "Could not observe child exit: {error}"
            );
            return Ok(());
        }
        #[cfg(target_os = "linux")]
        if fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|stat| {
            stat.rsplit_once(") ")
                .is_some_and(|(_, state)| state.starts_with('Z'))
        }) {
            return Ok(());
        }
        ensure!(
            start.elapsed() < Duration::from_secs(10),
            "Child {pid} did not exit"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}
#[cfg(windows)]
fn wait_exit(pid: u32) -> Result<()> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, WAIT_OBJECT_0},
        System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    };
    // SAFETY: request a synchronize-only handle; it is closed after waiting.
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    if handle.is_null() {
        let error = std::io::Error::last_os_error();
        ensure!(
            error.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32),
            "Could not observe child exit: {error}"
        );
        return Ok(());
    }
    // SAFETY: the owned handle is valid throughout the bounded wait.
    let result = unsafe { WaitForSingleObject(handle, 10_000) };
    unsafe {
        CloseHandle(handle);
    }
    ensure!(result == WAIT_OBJECT_0, "Child {pid} did not exit");
    Ok(())
}
