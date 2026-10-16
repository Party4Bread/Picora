use super::*;
fn fixture(root: PathBuf, runtime: tokio::runtime::Handle, ctx: &egui::Context) -> App {
    design::configure(ctx);
    #[cfg(target_os = "linux")]
    ime::install(ctx);
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
    fn find(shape: &egui::epaint::Shape, needle: &str, exact: bool) -> Option<egui::Pos2> {
        match shape {
            egui::epaint::Shape::Text(t)
                if (if exact {
                    t.galley.text() == needle
                } else {
                    t.galley.text().contains(needle)
                }) =>
            {
                Some(t.pos + t.galley.size() * 0.5)
            }
            egui::epaint::Shape::Vec(v) => v.iter().find_map(|s| find(s, needle, exact)),
            _ => None,
        }
    }
    output
        .shapes
        .iter()
        .find_map(|s| find(&s.shape, needle, true))
        .or_else(|| {
            output
                .shapes
                .iter()
                .find_map(|s| find(&s.shape, needle, false))
        })
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
fn favorite_button_populates_library_sidebar_before_download_finishes() {
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
    let entry =
        label(&output, "New favorite pack").expect("new pack appears in sidebar immediately");
    click(&mut app, &ctx, entry);
    assert!(app.filter == Filter::Pack("arca:54950".into()));
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

#[test]
fn library_grid_does_not_change_picker_keyboard_selection() {
    let root = tempfile::tempdir().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let ctx = egui::Context::default();
    let mut app = fixture(root.path().to_owned(), rt.handle().clone(), &ctx);
    let mut pack = crate::providers::dcinside::parse_detail(include_str!(
        "../../tests/fixtures/dc-detail.json"
    ))
    .unwrap();
    pack.stickers.truncate(1);
    app.store.save_favorite_pack(&pack).unwrap();
    app.refresh();
    app.selected = 20;
    frame(&mut app, &ctx, vec![]);
    assert_eq!(
        app.selected, 20,
        "rendering library must not move picker selection"
    );
}

#[test]
fn late_pack_response_cannot_replace_the_last_clicked_pack() {
    let root = tempfile::tempdir().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let ctx = egui::Context::default();
    let mut app = fixture(root.path().to_owned(), rt.handle().clone(), &ctx);
    let mut first = crate::providers::dcinside::parse_detail(include_str!(
        "../../tests/fixtures/dc-detail.json"
    ))
    .unwrap();
    first.title = "First pack".into();
    let mut second = first.clone();
    second.reference.id = "99".into();
    second.title = "Second pack".into();
    app.tab = Tab::Browse;
    app.catalog = vec![first.clone(), second.clone()];
    for name in ["First pack", "Second pack"] {
        let output = frame(&mut app, &ctx, vec![]);
        let pos = label(&output, name).unwrap();
        click(&mut app, &ctx, pos);
    }
    app.services.bus.send(Event::Preview(2, Ok(second)));
    app.handle_events(&ctx);
    app.services.bus.send(Event::Preview(1, Ok(first)));
    app.handle_events(&ctx);
    assert_eq!(app.preview.as_ref().unwrap().title, "Second pack");
}

#[test]
fn picker_enter_does_not_copy_a_sticker_while_a_popup_is_open() {
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
    app.refresh();
    let _ = ctx.run(
        egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
            ..Default::default()
        },
        |ctx| {
            egui::Popup::open_id(ctx, egui::Id::new("collection-menu"));
            egui::CentralPanel::default().show(ctx, |ui| app.grid(ui, app.items(), true));
        },
    );
    assert!(
        app.pending.is_none(),
        "Enter in a popup must not start copying the grid selection"
    );
}

#[test]
fn library_right_click_menu_can_unfavorite_and_remove_one_sticker() {
    let root = tempfile::tempdir().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let ctx = egui::Context::default();
    let mut app = fixture(root.path().to_owned(), rt.handle().clone(), &ctx);
    let mut pack = crate::providers::dcinside::parse_detail(include_str!(
        "../../tests/fixtures/dc-detail.json"
    ))
    .unwrap();
    pack.stickers.truncate(2);
    app.store.save_favorite_pack(&pack).unwrap();
    app.refresh();
    for action in ["Unfavorite sticker", "Remove sticker from library"] {
        let output = frame(&mut app, &ctx, vec![]);
        assert!(
            label(&output, "Manage").is_none(),
            "no duplicated Manage button"
        );
        let pos = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::epaint::Shape::Rect(r)
                    if (r.rect.width() - 80.).abs() < 0.1
                        && (r.rect.height() - 80.).abs() < 0.1 =>
                {
                    Some(r.rect.center())
                }
                _ => None,
            })
            .expect("sticker tile");
        for pressed in [true, false] {
            frame(
                &mut app,
                &ctx,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Secondary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
        assert!(
            app.pending.is_none(),
            "right-click must not copy the sticker"
        );
        let output = frame(&mut app, &ctx, vec![]);
        click(
            &mut app,
            &ctx,
            label(&output, action).expect("individual sticker action"),
        );
        assert_eq!(app.store.favorite_items().unwrap().len(), 1);
    }
    assert_eq!(app.packs[0].pack.stickers.len(), 1);
    assert_eq!(app.packs[0].pack.stickers[0].id, "959928");
    assert_eq!(app.store.favorite_items().unwrap().len(), 1);
}

#[test]
fn minimum_picker_keeps_custom_group_identity_visible() {
    let root = tempfile::tempdir().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let ctx = egui::Context::default();
    let mut app = fixture(root.path().to_owned(), rt.handle().clone(), &ctx);
    let group = app.store.create_group("Work replies").unwrap();
    app.refresh();
    app.picker_filter = Filter::Group(group);
    for _ in 0..3 {
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(340., 320.),
                )),
                ..Default::default()
            },
            |ctx| app.picker(ctx),
        );
        assert!(label(&output, "Work replies").is_some());
        assert!(label(&output, "Library").is_some());
        assert!(app.filter == Filter::All);
        assert!(app.pending.is_none());
    }
}

#[test]
#[cfg(target_os = "linux")]
fn picker_accepts_repeated_korean_commits_without_preedit() {
    for prefix in ["", "cat "] {
        let root = tempfile::tempdir().unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let ctx = egui::Context::default();
        let mut app = fixture(root.path().to_owned(), rt.handle().clone(), &ctx);
        let mut render = |events| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(460., 540.),
                )),
                events,
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| app.picker(ctx));
            app.picker_search.clone()
        };
        render(vec![]);
        render(vec![]);
        render(vec![egui::Event::Text(prefix.into())]);
        for (syllable, expected) in [("가", "가"), ("나", "가나"), ("다", "가나다")] {
            let actual = render(vec![
                egui::Event::Ime(egui::ImeEvent::Commit(syllable.into())),
                egui::Event::Ime(egui::ImeEvent::Disabled),
            ]);
            assert_eq!(actual, format!("{prefix}{expected}"));
        }
        // A normal multi-frame preedit must still replace its marked text once.
        render(vec![
            egui::Event::Ime(egui::ImeEvent::Enabled),
            egui::Event::Ime(egui::ImeEvent::Preedit("ㄹ".into())),
        ]);
        render(vec![egui::Event::Ime(egui::ImeEvent::Preedit("라".into()))]);
        let actual = render(vec![
            egui::Event::Ime(egui::ImeEvent::Commit("라".into())),
            egui::Event::Ime(egui::ImeEvent::Disabled),
        ]);
        assert_eq!(actual, format!("{prefix}가나다라"));
        assert!(app.pending.is_none(), "composition must not copy a sticker");
    }
}

#[test]
#[cfg(target_os = "linux")]
fn korean_commits_work_in_viewport_passes_without_eframe_input_hook() {
    for viewport in [
        egui::ViewportId::ROOT,
        egui::ViewportId::from_hash_of("library"),
    ] {
        let root = tempfile::tempdir().unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let ctx = egui::Context::default();
        let _app = fixture(root.path().to_owned(), rt.handle().clone(), &ctx);
        let mut text = String::new();
        let mut render = |events| {
            let mut input = egui::RawInput {
                viewport_id: viewport,
                events,
                ..Default::default()
            };
            input.viewports.entry(viewport).or_default();
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.text_edit_singleline(&mut text).request_focus();
                });
            });
            text.clone()
        };
        render(vec![]);
        for (syllable, expected) in [("고", "고"), ("양", "고양"), ("이", "고양이")] {
            assert_eq!(
                render(vec![
                    egui::Event::Ime(egui::ImeEvent::Commit(syllable.into())),
                    egui::Event::Ime(egui::ImeEvent::Disabled)
                ]),
                expected
            );
        }
    }
}

#[test]
fn animation_cache_pressure_keeps_still_thumbnails() {
    let root = tempfile::tempdir().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let ctx = egui::Context::default();
    let mut app = fixture(root.path().to_owned(), rt.handle().clone(), &ctx);
    // Three 12-byte animations exceed the 32-byte test cache, but their stills fit.
    for index in 0..3 {
        let frames = (0..3)
            .map(|frame| {
                ctx.load_texture(
                    format!("preview-{index}-{frame}"),
                    egui::ColorImage::new([1, 1], vec![Color32::WHITE]),
                    egui::TextureOptions::LINEAR,
                )
            })
            .collect();
        app.cache_visual(
            index.to_string(),
            Visual {
                frames,
                delays: vec![100; 3],
                bytes: 12,
                animation_loaded: true,
                last_used: index,
            },
            32,
        );
    }
    assert_eq!(
        app.visuals.len(),
        3,
        "cache pressure must not erase a loaded still"
    );
    assert!(app.visuals.values().all(|v| !v.frames.is_empty()));
    assert!(app.visuals.values().map(|v| v.bytes).sum::<usize>() <= 32);
}

#[test]
fn clipped_thumbnail_does_not_request_media() {
    let root = tempfile::tempdir().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let ctx = egui::Context::default();
    let mut app = fixture(root.path().to_owned(), rt.handle().clone(), &ctx);
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.set_clip_rect(egui::Rect::from_min_max(
                egui::Pos2::ZERO,
                egui::pos2(100., 100.),
            ));
            ui.add_space(200.);
            app.thumb(ui, Provider::Dcinside, "offscreen", 84., false);
        });
    });
    assert!(app.inflight.is_empty());
}

#[test]
fn evicted_animation_keeps_still_and_reloads_only_on_hover() {
    let root = tempfile::tempdir().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let ctx = egui::Context::default();
    let mut app = fixture(root.path().to_owned(), rt.handle().clone(), &ctx);
    let texture = ctx.load_texture(
        "still",
        egui::ColorImage::new([1, 1], vec![Color32::WHITE]),
        egui::TextureOptions::LINEAR,
    );
    app.visuals.insert(
        "demoted".into(),
        Visual {
            frames: vec![texture],
            delays: vec![100],
            bytes: 4,
            animation_loaded: false,
            last_used: 0,
        },
    );
    let render = |app: &mut App, events| {
        let mut rect = egui::Rect::NOTHING;
        let _ = ctx.run(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    rect = app
                        .thumb(ui, Provider::Dcinside, "demoted", 84., false)
                        .rect;
                });
            },
        );
        rect
    };
    let rect = render(&mut app, vec![]);
    render(&mut app, vec![]);
    assert!(
        app.inflight.is_empty(),
        "idle redraw must not reload animation"
    );
    for _ in 0..2 {
        render(&mut app, vec![egui::Event::PointerMoved(rect.center())]);
    }
    assert_eq!(app.inflight.len(), 1);
    assert_eq!(
        app.visuals["demoted"].frames.len(),
        1,
        "still must remain during reload"
    );
}

#[test]
fn visible_animation_loads_still_first_then_frames_on_hover() {
    let root = tempfile::tempdir().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let ctx = egui::Context::default();
    let mut app = fixture(root.path().to_owned(), rt.handle().clone(), &ctx);
    let url = "cached-animation";
    app.services
        .cache
        .save(url, include_bytes!("../../tests/fixtures/animation.mp4"))
        .unwrap();
    let render = |app: &mut App, events| {
        let mut rect = egui::Rect::NOTHING;
        let _ = ctx.run(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    rect = app.thumb(ui, Provider::Dcinside, url, 80., false).rect;
                });
            },
        );
        rect
    };
    let finish = |app: &mut App| {
        rt.block_on(async {
            tokio::time::timeout(Duration::from_secs(5), async {
                while !app.inflight.is_empty() {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                    app.handle_events(&ctx);
                }
            })
            .await
            .unwrap();
        });
        assert!(app.failed.is_empty(), "{:?}", app.failed);
    };
    let rect = render(&mut app, vec![]);
    finish(&mut app);
    assert_eq!(app.visuals[url].frames.len(), 1);
    assert!(!app.visuals[url].animation_loaded);
    for _ in 0..2 {
        render(&mut app, vec![egui::Event::PointerMoved(rect.center())]);
    }
    finish(&mut app);
    assert!(app.visuals[url].animation_loaded);
    assert!(app.visuals[url].frames.len() > 1);
}
