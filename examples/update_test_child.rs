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
    path::{Path, PathBuf},
    process::Command,
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
        if std::env::var_os("PICORA_TEST_PARENT_DELAY").is_some() {
            std::thread::sleep(Duration::from_millis(800));
        }
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
        fs::write(layout.root.join("started.txt"), &version)?;
        fs::write(
            layout.root.join("started-pid.txt"),
            std::process::id().to_string(),
        )?;
        std::thread::sleep(Duration::from_secs(1));
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
    let layout = InstallLayout::for_root(root, target)?;
    fs::create_dir_all(layout.executable().parent().unwrap())?;
    fs::copy(std::env::current_exe()?, layout.executable())?;
    fs::write(layout.root.join("version.txt"), "old")?;
    let job = temp.path().join("layout.json");
    fs::write(&job, serde_json::to_vec(&layout)?)?;
    let mut cmd = Command::new(std::env::current_exe()?);
    cmd.arg("--initiate")
        .arg(job)
        .env("PICORA_TEST_PARENT_DELAY", "1");
    if mode == "crash" {
        cmd.env("PICORA_TEST_CRASH", "1");
    }
    if mode == "hang" {
        cmd.env("PICORA_TEST_HANG", "1");
    }
    let mut parent = cmd.spawn()?;
    std::thread::sleep(Duration::from_millis(300));
    ensure!(
        fs::read_to_string(layout.root.join("version.txt"))? == "old",
        "Replaced before parent exit"
    );
    ensure!(parent.wait()?.success(), "Updater handoff failed");
    let start = Instant::now();
    loop {
        if let Ok(v) = fs::read_to_string(layout.root.join("started.txt")) {
            ensure!(
                v == if mode == "healthy" { "new" } else { "old" },
                "Wrong version restarted"
            );
            break;
        }
        ensure!(
            start.elapsed() < Duration::from_secs(80),
            "Updated child did not start"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    // Wait for helper commit and fixture child exit before removing its files (Windows).
    let start = Instant::now();
    while layout.state_dir()?.join("journal.json").exists() {
        ensure!(
            start.elapsed() < Duration::from_secs(10),
            "Transaction not finalized"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    std::thread::sleep(Duration::from_millis(1200));
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
    let layout = InstallLayout::for_root(root, target)?;
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
    let start = Instant::now();
    loop {
        if let Ok(version) = fs::read_to_string(layout.root.join("started.txt")) {
            ensure!(
                version == "old",
                "Recovery continued running new executable"
            );
            ensure!(
                fs::read_to_string(layout.root.join("started-pid.txt"))?.parse::<u32>()?
                    != first_pid,
                "Recovery must restart in a separate process"
            );
            break;
        }
        ensure!(
            start.elapsed() < Duration::from_secs(20),
            "External recovery failed"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    std::thread::sleep(Duration::from_millis(1200));
    Ok(())
}
