use super::*;

pub(super) struct Fixture {
    pub services: Services,
    pub runtime: tokio::runtime::Runtime,
    pub rx: mpsc::Receiver<Event>,
    _dir: tempfile::TempDir,
}
impl Fixture {
    pub fn new() -> Self {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = mpsc::channel();
        let services = Services::with_session(
            runtime.handle().clone(),
            MediaCache::new(dir.path().into()).unwrap(),
            Bus {
                tx,
                ctx: egui::Context::default(),
            },
            Ok(vec![]),
        )
        .unwrap();
        Self {
            services,
            runtime,
            rx,
            _dir: dir,
        }
    }
    fn copied(&self) -> Result<LibrarySticker> {
        let Event::Copied(result) = self.rx.try_recv().unwrap() else {
            panic!("missing copy completion")
        };
        assert!(self.rx.try_recv().is_err(), "one completion per copy");
        result
    }
}
#[test]
fn damaged_shared_state_still_completes_requests() {
    let fixture = Fixture::new();
    let (s, rt, rx) = (&fixture.services, &fixture.runtime, &fixture.rx);
    let repos = s.repos.clone();
    let _ = std::thread::spawn(move || {
        let _guard = repos.lock().unwrap();
        panic!("simulate interrupted repository mutation");
    })
    .join();
    rt.block_on(async {
        s.browse(Provider::Arca, String::new(), 1, 42);
        tokio::task::yield_now().await;
    });
    let Event::Catalog(42, result) = rx.try_recv().unwrap() else {
        panic!("missing catalog completion")
    };
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Repository state unavailable")
    );

    let status = s.account_status.clone();
    let _ = std::thread::spawn(move || {
        let _guard = status.lock().unwrap();
        panic!("simulate interrupted account mutation");
    })
    .join();
    rt.block_on(async {
        s.check_arca();
        tokio::task::yield_now().await;
    });
    let Event::Login(result) = rx.try_recv().unwrap() else {
        panic!("missing account completion")
    };
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Account state unavailable")
    );
}
#[test]
fn copy_prepares_selected_representation_and_always_completes() {
    use crate::{media::CopyMode, model::Sticker};
    let fixture = Fixture::new();
    let (s, rt) = (&fixture.services, &fixture.runtime);
    let url = "https://fixture.invalid/video.mp4";
    let asset = s
        .cache
        .save(url, include_bytes!("../../tests/fixtures/animation.mp4"))
        .unwrap();
    let item = LibrarySticker {
        pack_key: "test-pack".into(),
        pack_title: "Test pack".into(),
        provider: Provider::Arca,
        sticker: Sticker {
            id: "sticker-42".into(),
            label: "Video sticker".into(),
            url: url.into(),
        },
    };
    for (mode, expected_file, extension) in [
        (CopyMode::Still, false, "mp4"),
        (CopyMode::Original, true, "mp4"),
        (CopyMode::Auto, true, "gif"),
    ] {
        rt.block_on(
            s.copy_with(item.clone(), mode, |prepared, file| async move {
                assert_eq!(file, expected_file);
                assert_eq!(prepared.original.extension().unwrap(), extension);
                assert!(prepared.still.is_file());
                Ok(())
            }),
        );
        let copied = fixture.copied().unwrap();
        assert_eq!(copied.pack_key, item.pack_key);
        assert_eq!(copied.sticker.id, item.sticker.id);
    }
    rt.block_on(s.copy_with(item.clone(), CopyMode::Still, |_, _| async {
        anyhow::bail!("clipboard denied")
    }));
    assert!(
        fixture
            .copied()
            .unwrap_err()
            .to_string()
            .contains("clipboard denied")
    );

    let converted = s
        .cache
        .prepare_copy(asset, CopyMode::Auto)
        .unwrap()
        .0
        .original;
    std::fs::remove_file(&converted).unwrap();
    std::fs::create_dir(&converted).unwrap();
    rt.block_on(s.copy_with(item, CopyMode::Auto, |_, _| async {
        panic!("failed preparation must not reach clipboard")
    }));
    assert!(
        format!("{:#}", fixture.copied().unwrap_err())
            .contains("Cannot save animated clipboard file")
    );
}

#[test]
fn pack_downloads_run_concurrently_and_stop_at_the_first_failure() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let (active, peak) = (AtomicUsize::new(0), AtomicUsize::new(0));
    let reported = Mutex::new(Vec::new());
    let fetch = |fail: Option<&'static str>| {
        let (active, peak) = (&active, &peak);
        move |url: String| async move {
            let now = active.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(now, Ordering::SeqCst);
            tokio::task::yield_now().await;
            active.fetch_sub(1, Ordering::SeqCst);
            anyhow::ensure!(Some(url.as_str()) != fail, "download failed: {url}");
            Ok(())
        }
    };
    let urls = || (0..10).map(|i| format!("u{i}")).collect::<Vec<_>>();
    for limit in [1, 4, 7] {
        peak.store(0, Ordering::SeqCst);
        reported.lock().unwrap().clear();
        runtime
            .block_on(download_all(
                urls(),
                limit,
                |done| reported.lock().unwrap().push(done),
                fetch(None),
            ))
            .unwrap();
        assert_eq!(peak.load(Ordering::SeqCst), limit);
    }
    assert_eq!(*reported.lock().unwrap(), (1..=10).collect::<Vec<_>>());

    reported.lock().unwrap().clear();
    let error = runtime
        .block_on(download_all(
            urls(),
            4,
            |done| reported.lock().unwrap().push(done),
            fetch(Some("u1")),
        ))
        .unwrap_err();
    assert!(error.to_string().contains("u1"));
    assert!(
        reported.lock().unwrap().len() < 10,
        "remaining downloads cancelled"
    );
}

#[test]
fn download_limit_changes_apply_without_cancelling_in_flight_work() {
    let fixture = Fixture::new();
    let (s, rt) = (&fixture.services, &fixture.runtime);
    let settle = || rt.block_on(tokio::task::yield_now());
    assert_eq!(s.semaphore.available_permits(), 4);
    s.set_download_limit(6);
    assert_eq!(s.semaphore.available_permits(), 6);
    // Three downloads in flight while the limit drops to two.
    let in_flight = rt
        .block_on(s.semaphore.clone().acquire_many_owned(3))
        .unwrap();
    s.set_download_limit(2);
    settle();
    assert!(
        s.semaphore.try_acquire().is_err(),
        "no new download starts until in-flight work is under the new limit"
    );
    drop(in_flight);
    settle();
    assert_eq!(s.semaphore.available_permits(), 2);
    for (requested, applied) in [(0, 1), (99, 16)] {
        s.set_download_limit(requested);
        settle();
        assert_eq!(s.semaphore.available_permits(), applied);
    }
}
