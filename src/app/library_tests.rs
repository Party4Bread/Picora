use super::*;
fn fixture(root: PathBuf, runtime: tokio::runtime::Handle, ctx: &egui::Context) -> App {
    let store = Store::open(&root.join("library.sqlite3")).unwrap();
    let settings = Settings::default();
    let (tx, rx) = mpsc::channel();
    let (_, activation) = mpsc::channel();
    let services = Services::new(
        runtime,
        crate::media::MediaCache::new(root.join("media")).unwrap(),
        Bus {
            tx,
            ctx: ctx.clone(),
        },
    )
    .unwrap();
    App {
        updates: Default::default(),
        update_health_sent: false,
        packs: store.packs().unwrap(),
        groups: store.groups().unwrap(),
        store,
        services,
        rx,
        activation,
        shortcut_text: settings.shortcut.clone(),
        settings,
        filter: Filter::All,
        picker_filter: Filter::Favorites,
        picker_search: String::new(),
        tab: Tab::Library,
        search: String::new(),
        query: String::new(),
        source: Provider::Dcinside,
        page: 1,
        catalog: Vec::new(),
        catalog_id: 0,
        preview: None,
        library_open: true,
        picker_open: false,
        focus_search: true,
        selected: 0,
        status: "Choose a sticker to copy and paste".into(),
        error: false,
        browsing: false,
        installing: false,
        login_busy: false,
        group_name: String::new(),
        rename: String::new(),
        chosen_group: None,
        visuals: HashMap::new(),
        inflight: HashSet::new(),
        failed: HashMap::new(),
        target: None,
        pending: None,
        paste_at: None,
        shortcut: None,
        shortcut_status: String::new(),
        #[cfg(target_os = "linux")]
        portal_task: None,
        _tray: None,
        quit: false,
        data_dir: root,
    }
}
fn frame(app: &mut App, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(960., 700.),
            )),
            events,
            ..Default::default()
        },
        |ctx| app.library(ctx),
    )
}
fn label(output: &egui::FullOutput, needle: &str) -> Option<egui::Pos2> {
    fn find(shape: &egui::epaint::Shape, needle: &str) -> Option<egui::Pos2> {
        match shape {
            egui::epaint::Shape::Text(t) if t.galley.text().contains(needle) => {
                Some(t.pos + t.galley.size() * 0.5)
            }
            egui::epaint::Shape::Vec(v) => v.iter().find_map(|s| find(s, needle)),
            _ => None,
        }
    }
    output.shapes.iter().find_map(|s| find(&s.shape, needle))
}
fn click(app: &mut App, ctx: &egui::Context, pos: egui::Pos2) {
    for pressed in [true, false] {
        frame(
            app,
            ctx,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ],
        );
    }
}
#[test]
fn favorite_button_populates_library_dropdown_before_download_finishes() {
    let root = tempfile::tempdir().unwrap();
    // No executor is running: downloading cannot complete before the assertion.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let ctx = egui::Context::default();
    let mut app = fixture(root.path().to_owned(), rt.handle().clone(), &ctx);
    app.tab = Tab::Browse;
    app.preview = Some(Pack {
        reference: PackRef {
            provider: Provider::Arca,
            id: "54950".into(),
        },
        title: "New favorite pack".into(),
        author: String::new(),
        url: "https://arca.live/e/54950".into(),
        thumbnail: String::new(),
        stickers: vec![Sticker {
            id: "one".into(),
            label: "Sticker".into(),
            url: "https://ac.arca.live/test.png".into(),
        }],
    });
    let mut previous = app.preview.clone().unwrap();
    previous.reference.id = "42".into();
    previous.title = "Previous favorite pack".into();
    app.store.save_favorite_pack(&previous).unwrap();
    app.refresh();
    app.filter = Filter::Pack("arca:42".into());
    app.search = "previous".into();
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![]);
    }
    let output = frame(&mut app, &ctx, vec![]);
    let button = label(&output, "Add pack to favorites").expect("favorite button visible");
    click(&mut app, &ctx, button);
    assert!(app.installing, "favorite click should start install");
    assert_eq!(app.store.packs().unwrap().len(), 2, "both packs persisted");
    app.preview = None;
    app.tab = Tab::Library;
    let output = frame(&mut app, &ctx, vec![]);
    let dropdown = label(&output, "Previous favorite pack").expect("dropdown visible");
    click(&mut app, &ctx, dropdown);
    let output = frame(&mut app, &ctx, vec![]);
    assert!(
        label(&output, "New favorite pack").is_some(),
        "new pack must appear in open dropdown immediately"
    );
}

#[test]
fn picker_dropdown_lists_saved_dcinside_packs_alongside_custom_groups() {
    let root = tempfile::tempdir().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let ctx = egui::Context::default();
    let mut app = fixture(root.path().to_owned(), rt.handle().clone(), &ctx);
    let pack = crate::providers::dcinside::parse_detail(include_str!(
        "../../tests/fixtures/dc-detail.json"
    ))
    .unwrap();
    app.store.save_favorite_pack(&pack).unwrap();
    app.store.create_group("Custom reactions").unwrap();
    app.refresh();
    let render = |app: &mut App, events| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(460., 540.),
                )),
                events,
                ..Default::default()
            },
            |ctx| app.picker(ctx),
        )
    };
    for _ in 0..3 {
        render(&mut app, vec![]);
    }
    let output = render(&mut app, vec![]);
    let pos = label(&output, "Groups")
        .or_else(|| label(&output, "Packs & groups"))
        .expect("picker collection dropdown");
    for pressed in [true, false] {
        render(
            &mut app,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ],
        );
    }
    let output = render(&mut app, vec![]);
    assert!(label(&output, "Custom reactions").is_some());
    let pos =
        label(&output, "슬픈 고양이콘").expect("saved DCInside pack must be selectable in picker");
    for pressed in [true, false] {
        render(
            &mut app,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ],
        );
    }
    assert!(app.picker_filter == Filter::Pack("dcinside:41982".into()));
    assert!(
        app.filter == Filter::All,
        "picker selection must not change library filter"
    );
    let output = render(&mut app, vec![]);
    assert!(
        label(&output, "슬픈 고양이콘").is_some(),
        "selected pack title must remain visible"
    );
}
