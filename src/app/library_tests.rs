use super::*;
use crate::model::Sticker;

#[cfg(target_os = "macos")]
#[test]
fn macos_permission_failure_opens_setup_and_cancels_stale_paste() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let ctx = egui::Context::default();
    let mut app = fixture(temp.path().to_owned(), runtime.handle().clone(), &ctx);
    app.library_open = false;
    let ticket = app.services.paste_ticket();
    app.paste_at = Some((Instant::now() + Duration::from_secs(60), Target(0), None));
    app.services.bus.send(Event::Pasted(Err(
        platform::macos_permissions::AccessibilityRequired.into(),
    )));
    app.handle_events(&ctx);
    assert!(app.library_open);
    assert!(app.tab == Tab::Settings);
    assert!(app.error);
    assert!(app.status.contains("Cmd+V"));
    assert!(app.paste_at.is_none());
    assert!(!ticket.valid());
}

#[cfg(target_os = "macos")]
#[test]
fn macos_permission_panel_distinguishes_allowed_and_denied_without_prompting() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let ctx = egui::Context::default();
    let mut app = fixture(temp.path().to_owned(), runtime.handle().clone(), &ctx);
    for allowed in [false, true] {
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.macos_permissions_ui(ui, allowed));
        });
        assert_eq!(label(&output, "Allow automatic paste…").is_some(), !allowed);
        assert!(label(&output, "Check permission again").is_some());
        assert!(label(&output, "Open Accessibility settings").is_some());
    }
}

fn fixture(root: PathBuf, runtime: tokio::runtime::Handle, ctx: &egui::Context) -> App {
    design::configure(ctx);
    #[cfg(target_os = "linux")]
    ime::install(ctx);
    let store = Store::open(&root.join("library.sqlite3")).unwrap();
    let settings = Settings::default();
    let (tx, rx) = mpsc::channel();
    let (_, activation) = mpsc::channel();
    let services = Services::with_session(
        runtime,
        crate::media::MediaCache::new(root.join("media")).unwrap(),
        Bus {
            tx,
            ctx: ctx.clone(),
        },
        Ok(Vec::new()),
    )
    .unwrap();
    App {
        updates: crate::update::controller::Controller::default(),
        update_health_sent: false,
        packs: store.packs().unwrap(),
        groups: store.groups().unwrap(),
        store,
        services,
        rx,
        activation,
        shortcut_text: settings.shortcut.clone(),
        settings,
        library_state: BrowseState::new(Filter::All),
        picker_state: BrowseState::new(Filter::Favorites),
        tab: Tab::Library,
        query: String::new(),
        source: Provider::Dcinside,
        page: 1,
        catalog: Vec::new(),
        catalog_id: 0,
        preview: None,
        library_open: true,
        picker_open: false,
        focus_search: true,
        status: "Choose a sticker to copy and paste".into(),
        error: false,
        browsing: false,
        downloading_pack: false,
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
        shortcut_generation: 0,
        #[cfg(target_os = "linux")]
        portal_task: None,
        tray: None,
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
                    modifiers: egui::Modifiers::default(),
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
    app.library_state.filter = Filter::Pack("arca:42".into());
    app.library_state.search = "previous".into();
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![]);
    }
    let output = frame(&mut app, &ctx, vec![]);
    let button = label(&output, "Add pack to favorites").expect("favorite button visible");
    click(&mut app, &ctx, button);
    assert!(app.downloading_pack, "favorite click should start install");
    assert_eq!(app.store.packs().unwrap().len(), 2, "both packs persisted");
    app.preview = None;
    app.tab = Tab::Library;
    let output = frame(&mut app, &ctx, vec![]);
    let entry =
        label(&output, "New favorite pack").expect("new pack appears in sidebar immediately");
    click(&mut app, &ctx, entry);
    assert!(app.library_state.filter == Filter::Pack("arca:54950".into()));
}

#[test]
fn picker_accepts_text_in_the_frame_it_receives_window_focus() {
    for events in [
        vec![egui::Event::Text("고양이".into())],
        vec![
            egui::Event::Ime(egui::ImeEvent::Enabled),
            egui::Event::Ime(egui::ImeEvent::Preedit("고양이".into())),
            egui::Event::Ime(egui::ImeEvent::Commit("고양이".into())),
            egui::Event::Ime(egui::ImeEvent::Disabled),
        ],
    ] {
        let root = tempfile::tempdir().unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let ctx = egui::Context::default();
        let mut app = fixture(root.path().to_owned(), runtime.handle().clone(), &ctx);
        let input = egui::RawInput {
            focused: true,
            events: std::iter::once(egui::Event::WindowFocused(true))
                .chain(events)
                .collect(),
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| app.picker(ctx));
        assert_eq!(app.picker_state.search, "고양이");
    }
}

#[test]
fn picker_waits_for_window_focus_before_consuming_search_focus_request() {
    let root = tempfile::tempdir().unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let ctx = egui::Context::default();
    let mut app = fixture(root.path().to_owned(), runtime.handle().clone(), &ctx);
    let _ = ctx.run(
        egui::RawInput {
            focused: false,
            ..Default::default()
        },
        |ctx| app.picker(ctx),
    );
    assert!(
        app.focus_search,
        "hidden-window render must not consume focus"
    );
    let _ = ctx.run(
        egui::RawInput {
            focused: true,
            events: vec![egui::Event::Text("cat".into())],
            ..Default::default()
        },
        |ctx| app.picker(ctx),
    );
    assert_eq!(app.picker_state.search, "cat");
    assert!(!app.focus_search);
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
                    modifiers: egui::Modifiers::default(),
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
                    modifiers: egui::Modifiers::default(),
                },
            ],
        );
    }
    assert!(app.picker_state.filter == Filter::Pack("dcinside:41982".into()));
    assert!(
        app.library_state.filter == Filter::All,
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
    app.picker_state.selected = 20;
    frame(&mut app, &ctx, vec![]);
    assert_eq!(
        app.picker_state.selected, 20,
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
                modifiers: egui::Modifiers::default(),
            }],
            ..Default::default()
        },
        |ctx| {
            egui::Popup::open_id(ctx, egui::Id::new("collection-menu"));
            egui::CentralPanel::default().show(ctx, |ui| {
                app.grid(
                    ui,
                    &app.items(&app.picker_state.filter, &app.picker_state.search)
                        .unwrap(),
                    View::Picker,
                );
            });
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
    ctx.enable_accesskit();
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
        let bounds = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find_map(|(_, node)| {
                (node.label() == Some(pack.stickers[0].label.as_str()))
                    .then(|| node.bounds())
                    .flatten()
            })
            .expect("accessible sticker button");
        let pos = egui::pos2(
            f64::midpoint(bounds.x0, bounds.x1) as f32,
            f64::midpoint(bounds.y0, bounds.y1) as f32,
        );
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
                        modifiers: egui::Modifiers::default(),
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
    app.picker_state.filter = Filter::Group(group);
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
        assert!(app.library_state.filter == Filter::All);
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
            app.picker_state.search.clone()
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
            &index.to_string(),
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
                        .response
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
                    rect = app
                        .thumb(ui, Provider::Dcinside, url, 80., false)
                        .response
                        .rect;
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

#[test]
fn accounts_show_saved_verified_and_failed_session_states() {
    let root = tempfile::tempdir().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let ctx = egui::Context::default();
    let mut app = fixture(root.path().to_owned(), rt.handle().clone(), &ctx);
    app.tab = Tab::Accounts;
    *app.services.account_status.lock().unwrap() = crate::services::account::AccountStatus {
        has_session: true,
        saved: true,
        ..Default::default()
    };
    frame(&mut app, &ctx, vec![]);
    let output = frame(&mut app, &ctx, vec![]);
    assert!(label(&output, "Session saved · not checked").is_some());
    assert!(label(&output, "Check connection").is_some());
    app.services
        .account_status
        .lock()
        .unwrap()
        .finish_check(Ok(()));
    let output = frame(&mut app, &ctx, vec![]);
    assert!(label(&output, "Connected").is_some());
    assert!(label(&output, "Catalog access verified.").is_some());
    app.services
        .account_status
        .lock()
        .unwrap()
        .finish_check(Err("Website verification required".into()));
    let output = frame(&mut app, &ctx, vec![]);
    assert!(label(&output, "Connection needs attention").is_some());
    assert!(label(&output, "Website verification required").is_some());
    assert!(label(&output, "Catalog access verified.").is_none());
}

#[test]
fn retry_thumbnail_click_does_not_activate_sticker() {
    let root = tempfile::tempdir().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let ctx = egui::Context::default();
    let mut app = fixture(root.path().to_owned(), rt.handle().clone(), &ctx);
    app.failed.insert("broken".into(), "download failed".into());
    let mut render = |events| {
        let mut result = None;
        let _ = ctx.run(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    result = Some(app.thumb(ui, Provider::Dcinside, "broken", 80., false));
                });
            },
        );
        result.unwrap()
    };
    let center = render(vec![]).response.rect.center();
    render(vec![
        egui::Event::PointerMoved(center),
        egui::Event::PointerButton {
            pos: center,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    ]);
    let response = render(vec![egui::Event::PointerButton {
        pos: center,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    assert!(response.response.clicked());
    assert!(!response.activated);
    assert!(!app.failed.contains_key("broken"));
    assert!(app.pending.is_none());
}

#[test]
fn failed_library_query_is_not_an_empty_collection() {
    let root = tempfile::tempdir().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let ctx = egui::Context::default();
    let mut app = fixture(root.path().to_owned(), rt.handle().clone(), &ctx);
    rusqlite::Connection::open(root.path().join("library.sqlite3"))
        .unwrap()
        .execute_batch("DROP TABLE recents")
        .unwrap();
    app.library_state.filter = Filter::Recent;
    assert!(
        app.items(&app.library_state.filter, &app.library_state.search)
            .is_err()
    );
    let output = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| app.library_content(ui));
    });
    assert!(app.error);
    assert!(app.status.contains("Could not load stickers"));
    assert!(label(&output, "No stickers in this collection").is_none());
}

#[test]
fn group_rendering_preserves_library_and_picker_queries() {
    let root = tempfile::tempdir().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let ctx = egui::Context::default();
    let mut app = fixture(root.path().to_owned(), rt.handle().clone(), &ctx);
    let group = app.store.create_group("Test group").unwrap();
    app.refresh();
    app.chosen_group = Some(group);
    app.library_state.filter = Filter::Recent;
    app.library_state.search = "library query".into();
    app.picker_state.search = "picker query".into();
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| app.groups_ui(ui));
    });
    assert!(app.library_state.filter == Filter::Recent);
    assert_eq!(app.library_state.search, "library query");
    assert_eq!(app.picker_state.search, "picker query");
    assert!(app.items(&Filter::Group(group), "").unwrap().is_empty());
}

#[test]
fn copy_completion_respects_cancellation_modifiers_and_clipboard_failure() {
    for case in [
        "success",
        "cancelled",
        "modifiers",
        "failure",
        "copy-only",
        "no-target",
        "preview",
        "blurred",
    ] {
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
        let item = app.items(&Filter::All, "").unwrap().remove(0);
        let interaction = app.services.paste_ticket();
        app.picker_open = true;
        app.pending = Some(PendingPaste {
            interaction,
            picker: None,
            target: (case != "no-target").then_some(Target(42)),
            from_picker: true,
            auto_paste: case != "copy-only",
        });
        if case == "cancelled" {
            app.open_library();
        }
        if case == "preview" {
            app.store.remove_pack(&item.pack_key).unwrap();
        }
        app.services.bus.send(Event::Copied(if case == "failure" {
            Err(anyhow::anyhow!("clipboard unavailable"))
        } else {
            Ok(item)
        }));
        let mut input = egui::RawInput::default();
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .focused = Some(case != "blurred");
        if case == "modifiers" {
            input.modifiers.shift = true;
        }
        let _ = ctx.run(input, |ctx| app.handle_events(ctx));
        assert!(app.pending.is_none(), "{case}");
        assert_eq!(
            app.paste_at.is_some(),
            matches!(case, "success" | "preview"),
            "{case}"
        );
        assert_eq!(
            app.picker_open,
            matches!(case, "cancelled" | "failure"),
            "{case}"
        );
        if case != "failure" {
            assert!(!app.error, "{case}: {}", app.status);
        }
        if case == "success" {
            app.open_library();
            assert!(
                app.paste_at.is_none(),
                "opening library cancels scheduled paste"
            );
        }
        if case == "failure" {
            assert!(app.error);
            assert!(app.status.contains("clipboard unavailable"));
        }
    }
}

#[test]
fn shortcut_settings_change_only_after_current_registration_succeeds() {
    let root = tempfile::tempdir().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let ctx = egui::Context::default();
    let mut app = fixture(root.path().to_owned(), rt.handle().clone(), &ctx);
    let original = app.settings.shortcut.clone();
    app.shortcut_generation = 2;
    app.shortcut_registered(1, "Ctrl+Alt+S".into(), true, Ok(()));
    assert_eq!(app.store.settings().unwrap().shortcut, original);
    app.shortcut_registered(
        2,
        "Ctrl+Alt+S".into(),
        true,
        Err(anyhow::anyhow!("permission denied")),
    );
    assert_eq!(app.store.settings().unwrap().shortcut, original);
    assert!(app.shortcut_status.contains("permission denied"));
    app.shortcut_registered(2, "Ctrl+Alt+S".into(), true, Ok(()));
    assert_eq!(app.store.settings().unwrap().shortcut, "Ctrl+Alt+S");
    assert_eq!(app.shortcut_status, "Active: Ctrl+Alt+S");
}

#[test]
fn media_failure_preserves_the_diagnostic_chain() {
    let dir = tempfile::tempdir().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let ctx = egui::Context::default();
    let mut app = fixture(dir.path().to_owned(), rt.handle().clone(), &ctx);
    let permit = std::sync::Arc::new(tokio::sync::Semaphore::new(1))
        .try_acquire_owned()
        .unwrap();
    app.services.bus.send(Event::Media(
        "test-sticker".into(),
        Err(anyhow::anyhow!("missing video track").context("Invalid MP4 sticker")),
        true,
        permit,
    ));
    app.handle_events(&ctx);
    assert_eq!(
        app.failed.get("test-sticker").unwrap(),
        "Invalid MP4 sticker: missing video track"
    );
}

#[test]
fn update_panel_cancels_downloads_but_not_handoff_and_shows_failure() {
    use crate::update::controller::State;
    fn render(app: &mut App, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| app.updates_ui(ui));
            },
        )
    }
    let dir = tempfile::tempdir().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let ctx = egui::Context::default();
    let mut app = fixture(dir.path().to_owned(), rt.handle().clone(), &ctx);
    app.updates.state = State::Downloading(5, 10);
    render(&mut app, &ctx, vec![]);
    let output = render(&mut app, &ctx, vec![]);
    let position = label(&output, "Cancel update").expect("download offers cancellation");
    for pressed in [true, false] {
        render(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::default(),
                },
            ],
        );
    }
    assert!(matches!(app.updates.state, State::Idle));
    app.updates.state = State::HandingOff;
    let output = render(&mut app, &ctx, vec![]);
    assert!(label(&output, "Cancel update").is_none());
    assert!(label(&output, "Preparing restart…").is_some());
    app.updates.state = State::Error("Update checksum or size mismatch".into());
    let output = render(&mut app, &ctx, vec![]);
    assert!(label(&output, "Update checksum or size mismatch").is_some());
}

#[test]
fn synchronous_reports_preserve_causes_and_clear_errors_after_success() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let ctx = egui::Context::default();
    let mut app = fixture(temp.path().to_owned(), runtime.handle().clone(), &ctx);
    app.report(
        Err(anyhow::anyhow!("Permission denied").context("Cannot delete saved session")),
        "Signed out",
    );
    assert!(app.error);
    assert_eq!(app.status, "Cannot delete saved session: Permission denied");
    app.report(Ok(()), "Signed out");
    assert!(!app.error);
    assert_eq!(app.status, "Signed out");
}
