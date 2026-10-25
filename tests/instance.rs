use picora::platform::instance::Instance;
use std::{
    io::{BufRead, Read, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

const ROOT: &str = "PICORA_INSTANCE_TEST_ROOT";
const MODE: &str = "PICORA_INSTANCE_TEST_MODE";

#[test]
fn instance_process_probe() {
    let Some(root) = std::env::var_os(ROOT) else {
        return;
    };
    let mode = std::env::var(MODE).unwrap();
    let (tx, rx) = mpsc::channel();
    let instance = Instance::acquire(
        Path::new(&root),
        mode == "library",
        mode == "background",
        tx,
    )
    .unwrap();
    if mode != "owner" {
        assert!(
            instance.is_none(),
            "secondary process must not become owner"
        );
        return;
    }
    let _instance = instance.expect("first process must own the instance lock");
    reply("ready");
    let mut input = std::io::stdin();
    loop {
        let mut command = [0];
        input.read_exact(&mut command).unwrap();
        match command[0] {
            b'R' => reply(if rx.recv_timeout(Duration::from_secs(10)).unwrap() {
                "library"
            } else {
                "picker"
            }),
            b'Q' => {
                assert!(matches!(
                    rx.recv_timeout(Duration::from_millis(250)),
                    Err(mpsc::RecvTimeoutError::Timeout)
                ));
                reply("quiet");
            }
            b'X' => return,
            _ => panic!("unknown probe command"),
        }
    }
}

fn reply(message: &str) {
    println!("PICORA_PROBE:{message}");
    std::io::stdout().flush().unwrap();
}

fn command(root: &Path, mode: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "instance_process_probe", "--nocapture"])
        .env(ROOT, root)
        .env(MODE, mode);
    command
}

struct Owner {
    child: Child,
    replies: mpsc::Receiver<String>,
}
impl Owner {
    fn start(root: &Path) -> Self {
        let mut child = command(root, "owner")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, replies) = mpsc::channel();
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(stdout).lines() {
                let line = line.unwrap();
                if let Some((_, response)) = line.split_once("PICORA_PROBE:") {
                    let _ = tx.send(response.to_owned());
                }
            }
        });
        let owner = Self { child, replies };
        owner.expect("ready");
        owner
    }
    fn expect(&self, expected: &str) {
        assert_eq!(
            self.replies.recv_timeout(Duration::from_secs(15)).unwrap(),
            expected
        );
    }
    fn send(&mut self, command: u8) {
        self.child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(&[command])
            .unwrap();
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn wait_success(child: &mut Child) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "probe failed: {status}");
            return;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("probe process timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn activation_routes_to_owner_and_lock_is_released_on_exit() {
    let root = tempfile::tempdir().unwrap();
    let mut owner = Owner::start(root.path());
    for (mode, expected) in [
        ("picker", "picker"),
        ("library", "library"),
        ("background", "quiet"),
    ] {
        let mut secondary = command(root.path(), mode)
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        wait_success(&mut secondary);
        owner.send(if mode == "background" { b'Q' } else { b'R' });
        owner.expect(expected);
    }
    owner.send(b'X');
    wait_success(&mut owner.child);
    let mut replacement = Owner::start(root.path());
    replacement.send(b'X');
    wait_success(&mut replacement.child);
}

#[test]
fn long_data_paths_still_allow_secondary_activation() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("long-data-directory-".repeat(8));
    let mut owner = Owner::start(&root);
    #[cfg(unix)]
    let secondary_root = {
        let alias = temp.path().join("alias");
        std::os::unix::fs::symlink(&root, &alias).unwrap();
        alias
    };
    #[cfg(not(unix))]
    let secondary_root = root.clone();
    let mut secondary = command(&secondary_root, "library")
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    wait_success(&mut secondary);
    owner.send(b'R');
    owner.expect("library");
    owner.send(b'X');
    wait_success(&mut owner.child);
}
