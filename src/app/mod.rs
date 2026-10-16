mod design;
mod events;
mod grid;
#[cfg(target_os = "linux")]
mod ime;
mod library;
mod updates;
use crate::{
    media::CopyMode,
    model::*,
    platform::{
        self,
        focus::{self, Target},
        paste::{Action, PasteFlow},
        shortcut::Shortcut,
    },
    services::{Bus, Event, Services},
    store::Store,
};
use anyhow::Result;
use eframe::egui::{self, Color32, RichText, Vec2};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};

#[derive(Clone, PartialEq)]
enum Filter {
    Favorites,
    Recent,
    All,
    Pack(String),
    Group(i64),
}
#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Library,
    Browse,
    Groups,
    Accounts,
    Settings,
}
struct Visual {
    frames: Vec<egui::TextureHandle>,
    delays: Vec<u64>,
    bytes: usize,
    animation_loaded: bool,
    last_used: u64,
}
struct PendingPaste {
    interaction: platform::paste::Ticket,
    picker: Option<Target>,
    flow: PasteFlow,
    target: Option<Target>,
    from_picker: bool,
}
pub struct App {
    updates: crate::update::controller::Controller,
    update_health_sent: bool,
    store: Store,
    services: Services,
    rx: mpsc::Receiver<Event>,
    activation: mpsc::Receiver<bool>,
    settings: Settings,
    packs: Vec<SavedPack>,
    groups: Vec<Group>,
    filter: Filter,
    picker_filter: Filter,
    picker_search: String,
    tab: Tab,
    search: String,
    query: String,
    source: Provider,
    page: u32,
    catalog: Vec<Pack>,
    catalog_id: u64,
    preview: Option<Pack>,
    library_open: bool,
    picker_open: bool,
    focus_search: bool,
    selected: usize,
    status: String,
    error: bool,
    browsing: bool,
    installing: bool,
    login_busy: bool,
    group_name: String,
    rename: String,
    chosen_group: Option<i64>,
    visuals: HashMap<String, Visual>,
    inflight: HashSet<String>,
    failed: HashMap<String, String>,
    target: Option<Target>,
    pending: Option<PendingPaste>,
    paste_at: Option<(Instant, Target, Option<Target>)>,
    shortcut: Option<Shortcut>,
    shortcut_text: String,
    shortcut_status: String,
    #[cfg(target_os = "linux")]
    portal_task: Option<tokio::task::JoinHandle<()>>,
    _tray: Option<tray_icon::TrayIcon>,
    quit: bool,
    data_dir: PathBuf,
}
impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        runtime: tokio::runtime::Handle,
        root: PathBuf,
        activation: mpsc::Receiver<bool>,
        background: bool,
        library: bool,
    ) -> Result<Self> {
        let ctx = &cc.egui_ctx;
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "noto-kr".into(),
            egui::FontData::from_static(include_bytes!("../../assets/NotoSansKR-Regular.otf"))
                .into(),
        );
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "noto-kr".into());
        ctx.set_fonts(fonts);
        design::configure(ctx);
        #[cfg(target_os = "linux")]
        ime::install(ctx);
        let store = Store::open(&root.join("library.sqlite3"))?;
        let settings = store.settings()?;
        apply_theme(ctx, &settings.theme);
        let (tx, rx) = mpsc::channel();
        let bus = Bus {
            tx,
            ctx: ctx.clone(),
        };
        let services = Services::new(
            runtime,
            crate::media::MediaCache::new(root.join("media"))?,
            bus,
        )?;
        let mut app = Self {
            updates: Default::default(),
            update_health_sent: false,
            packs: store.packs()?,
            groups: store.groups()?,
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
            library_open: library,
            picker_open: !background && !library,
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
        };
        if app.packs.is_empty() && !background {
            app.library_open = true;
            app.picker_open = false;
        }
        if app.settings.check_updates_automatically {
            app.updates.check(&app.services.runtime, ctx);
        }
        app.register_shortcut();
        match make_tray() {
            Ok(tray) => app._tray = Some(tray),
            Err(e) => {
                app.library_open = true;
                app.message(
                    format!("Tray unavailable: {e}. Use picora --show to reopen."),
                    true,
                );
            }
        }
        Ok(app)
    }
    fn message(&mut self, s: impl Into<String>, error: bool) {
        self.status = s.into();
        self.error = error;
    }
    fn refresh(&mut self) {
        match (self.store.packs(), self.store.groups()) {
            (Ok(p), Ok(g)) => {
                self.packs = p;
                self.groups = g
            }
            (Err(e), _) | (_, Err(e)) => self.message(e.to_string(), true),
        }
    }
    fn register_shortcut(&mut self) {
        if platform::is_wayland() {
            #[cfg(target_os = "linux")]
            {
                if let Some(task) = self.portal_task.take() {
                    task.abort();
                }
                let bus = self.services.bus.clone();
                let trigger = self
                    .shortcut_text
                    .replace("Ctrl", "CTRL")
                    .replace("Shift", "SHIFT")
                    .replace("Space", "space");
                self.portal_task = Some(self.services.runtime.spawn(async move {
                    let activate = bus.clone();
                    if let Err(e) =
                        platform::wayland::shortcuts(trigger, move || activate.send(Event::Show))
                            .await
                    {
                        bus.send(Event::Progress(format!("Shortcut: {e}")));
                    }
                }));
                self.shortcut_status =
                    "Desktop shortcut permission requested. Use picora --show if unavailable."
                        .into();
            }
        } else {
            let result = if let Some(s) = &mut self.shortcut {
                s.replace(&self.shortcut_text)
            } else {
                Shortcut::new(&self.shortcut_text).map(|s| self.shortcut = Some(s))
            };
            self.shortcut_status = match result {
                Ok(()) => format!("Active: {}", self.shortcut_text),
                Err(e) => format!("{e:#}"),
            };
        }
    }
    fn show_picker(&mut self, ctx: &egui::Context) {
        self.paste_at = None;
        self.services.cancel_paste();
        if self.picker_open {
            self.hide_picker(ctx);
            return;
        }
        self.target = if ctx.input(|i| {
            i.raw
                .viewports
                .get(&egui::ViewportId::from_hash_of("library"))
                .is_some_and(|v| v.focused == Some(true))
        }) {
            None
        } else {
            focus::capture_external()
        };
        self.picker_open = true;
        self.focus_search = true;
        self.selected = 0;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }
    fn hide_picker(&mut self, ctx: &egui::Context) {
        self.paste_at = None;
        self.services.cancel_paste();
        if let Some(p) = &mut self.pending {
            p.flow.cancel_auto();
        }
        self.picker_open = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
    }
    fn open_library(&mut self) {
        self.services.bus.ctx.send_viewport_cmd_to(
            egui::ViewportId::from_hash_of("library"),
            egui::ViewportCommand::Focus,
        );
        self.paste_at = None;
        self.services.cancel_paste();
        if let Some(p) = &mut self.pending {
            p.flow.cancel_auto();
        }
        self.library_open = true;
        self.target = None;
    }
    fn preview_pack(&mut self, reference: PackRef) {
        self.catalog_id += 1;
        self.preview = None;
        self.browsing = true;
        self.services.preview(reference, self.catalog_id);
    }
    fn browse(&mut self) {
        self.catalog_id += 1;
        self.browsing = true;
        self.catalog.clear();
        self.preview = None;
        self.services
            .browse(self.source, self.query.clone(), self.page, self.catalog_id);
    }
    fn collection_title(&self) -> String {
        match &self.filter {
            Filter::All => "All stickers".into(),
            Filter::Favorites => "Favorites".into(),
            Filter::Recent => "Recent".into(),
            Filter::Pack(key) => self
                .packs
                .iter()
                .find(|p| p.pack.reference.key() == *key)
                .map(|p| p.pack.title.clone())
                .unwrap_or_else(|| "All stickers".into()),
            Filter::Group(id) => self
                .groups
                .iter()
                .find(|g| g.id == *id)
                .map(|g| g.name.clone())
                .unwrap_or_else(|| "All stickers".into()),
        }
    }
    fn items(&self) -> Vec<LibrarySticker> {
        let items = match &self.filter {
            Filter::Group(id) => self.store.group_items(*id).unwrap_or_default(),
            Filter::Recent => self.store.recents().unwrap_or_default(),
            Filter::Favorites => self.store.favorite_items().unwrap_or_default(),
            filter => self
                .packs
                .iter()
                .filter(|p| match filter {
                    Filter::Pack(key) => p.pack.reference.key() == *key,
                    _ => true,
                })
                .flat_map(|p| {
                    p.pack.stickers.iter().map(|s| LibrarySticker {
                        pack_key: p.pack.reference.key(),
                        pack_title: p.pack.title.clone(),
                        provider: p.pack.reference.provider,
                        sticker: s.clone(),
                    })
                })
                .collect(),
        };
        let query = self.search.trim().to_lowercase();
        items
            .into_iter()
            .filter(|s| {
                query.is_empty()
                    || s.sticker.label.to_lowercase().contains(&query)
                    || s.pack_title.to_lowercase().contains(&query)
            })
            .collect()
    }
    fn choose(&mut self, item: LibrarySticker, mode: CopyMode, from_picker: bool) {
        if matches!(
            self.updates.state,
            crate::update::controller::State::HandingOff
        ) || self.pending.is_some()
            || self.paste_at.is_some()
        {
            return;
        }
        let target = if from_picker { self.target } else { None };
        self.pending = Some(PendingPaste {
            interaction: self.services.paste_ticket(),
            picker: focus::capture(),
            flow: PasteFlow::new(self.settings.auto_paste, target.is_some()),
            target,
            from_picker,
        });
        self.message("Preparing clipboard…", false);
        self.services.copy(item, mode);
    }
    fn report(&mut self, result: Result<()>, success: &str) {
        match result {
            Ok(()) => self.message(success, false),
            Err(e) => self.message(e.to_string(), true),
        }
    }
    fn footer(&self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if self.pending.is_some() || self.installing || self.browsing || self.login_busy {
                ui.spinner();
            }
            ui.label(RichText::new(&self.status).size(12.).color(if self.error {
                ui.visuals().error_fg_color
            } else {
                ui.visuals().weak_text_color()
            }));
        });
    }
    fn picker(&mut self, ctx: &egui::Context) {
        std::mem::swap(&mut self.filter, &mut self.picker_filter);
        std::mem::swap(&mut self.search, &mut self.picker_search);
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(ctx.style().visuals.panel_fill)
                    .inner_margin(12.),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let title = if ctx.content_rect().height() < 400.
                        && matches!(self.filter, Filter::Pack(_) | Filter::Group(_))
                    {
                        self.collection_title()
                    } else {
                        "Picora".to_owned()
                    };
                    ui.add_sized(
                        [(ui.available_width() - 84.).max(40.), 26.],
                        egui::Label::new(RichText::new(&title).size(18.).strong()).truncate(),
                    )
                    .on_hover_text(&title);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(egui::Button::new("Library").frame_when_inactive(false))
                            .clicked()
                        {
                            self.open_library();
                        }
                    });
                });
                let search = ui.add_sized(
                    [ui.available_width(), 36.],
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text("Search stickers…")
                        .margin(egui::vec2(12., 8.))
                        .desired_width(f32::INFINITY),
                );
                if self.focus_search {
                    search.request_focus();
                    self.focus_search = false;
                }
                if ui.input(|i| {
                    i.key_pressed(egui::Key::ArrowDown) || i.key_pressed(egui::Key::ArrowUp)
                }) {
                    search.surrender_focus();
                }
                ui.horizontal_wrapped(|ui| {
                    for (label, filter) in [
                        ("Favorites", Filter::Favorites),
                        ("Recent", Filter::Recent),
                        ("All", Filter::All),
                    ] {
                        if ui
                            .selectable_value(&mut self.filter, filter, label)
                            .changed()
                        {
                            self.selected = 0;
                        }
                    }
                    let collection_label = if ui.max_rect().width() < 380. {
                        "More"
                    } else {
                        "Packs & groups"
                    };
                    egui::ComboBox::from_id_salt("collections")
                        .width(if ui.max_rect().width() < 380. {
                            68.
                        } else {
                            140.
                        })
                        .selected_text(collection_label)
                        .show_ui(ui, |ui| {
                            ui.label("Saved packs");
                            for p in &self.packs {
                                if ui
                                    .selectable_value(
                                        &mut self.filter,
                                        Filter::Pack(p.pack.reference.key()),
                                        &p.pack.title,
                                    )
                                    .changed()
                                {
                                    self.selected = 0;
                                }
                            }
                            if !self.groups.is_empty() {
                                ui.separator();
                                ui.label("Custom groups");
                                for g in &self.groups {
                                    if ui
                                        .selectable_value(
                                            &mut self.filter,
                                            Filter::Group(g.id),
                                            &g.name,
                                        )
                                        .changed()
                                    {
                                        self.selected = 0;
                                    }
                                }
                            }
                        });
                });
                egui::TopBottomPanel::bottom("picker-status").show_inside(ui, |ui| self.footer(ui));
                if !self.packs.is_empty() {
                    egui::TopBottomPanel::bottom("picker-packs")
                        .exact_height(48.)
                        .show_separator_line(false)
                        .show_inside(ui, |ui| {
                            egui::ScrollArea::horizontal()
                                .id_salt("pack-shortcuts")
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        for p in self.packs.clone() {
                                            ui.push_id(p.pack.reference.key(), |ui| {
                                                let filter = Filter::Pack(p.pack.reference.key());
                                                if self
                                                    .thumb(
                                                        ui,
                                                        p.pack.reference.provider,
                                                        &p.pack.thumbnail,
                                                        38.,
                                                        self.filter == filter,
                                                    )
                                                    .on_hover_text(&p.pack.title)
                                                    .clicked()
                                                {
                                                    self.filter = filter;
                                                    self.selected = 0;
                                                }
                                            });
                                        }
                                    });
                                });
                        });
                }
                ui.add_space(4.);
                let items = self.items();
                if ctx.content_rect().height() >= 400. {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(self.collection_title()).small().strong(),
                            )
                            .truncate(),
                        );
                        ui.label(RichText::new(items.len().to_string()).small().weak());
                    });
                    ui.add_space(4.);
                }
                self.grid(ui, items, true);
            });
        std::mem::swap(&mut self.filter, &mut self.picker_filter);
        std::mem::swap(&mut self.search, &mut self.picker_search);
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.hide_picker(ctx);
        }
    }
}
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        #[cfg(target_os = "linux")]
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
        if !self.update_health_sent {
            self.update_health_sent = true;
            let bus = self.services.bus.clone();
            self.services.runtime.spawn_blocking(move || {
                if let Err(e) = crate::update::helper::acknowledge_health() {
                    bus.send(Event::UpdateHealthFailed(format!(
                        "Update startup acknowledgement failed: {e:#}"
                    )));
                }
            });
        }
        if self.updates.poll(&self.services.runtime, ctx) {
            self.services.cancel_paste();
            self.paste_at = None;
            self.pending = None;
            self.quit = true;
        }
        if matches!(
            self.updates.state,
            crate::update::controller::State::HandingOff
        ) {
            self.services.cancel_paste();
            self.paste_at = None;
            self.pending = None;
        }
        self.handle_events(ctx);
        if self.quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.hide_picker(ctx);
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(self.picker_open));
        if self.picker_open {
            self.picker(ctx);
        }
        if self.library_open {
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("library"),
                egui::ViewportBuilder::default()
                    .with_title("Picora · Library")
                    .with_inner_size([960., 700.])
                    .with_min_inner_size([640., 460.]),
                |ctx, _| {
                    if ctx.input(|i| i.viewport().close_requested()) {
                        self.library_open = false;
                    }
                    self.library(ctx);
                },
            );
        }
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}
fn apply_theme(ctx: &egui::Context, theme: &str) {
    ctx.set_theme(match theme {
        "Light" => egui::ThemePreference::Light,
        "Dark" => egui::ThemePreference::Dark,
        _ => egui::ThemePreference::System,
    });
}
fn make_tray() -> Result<tray_icon::TrayIcon> {
    use tray_icon::{
        Icon, TrayIconBuilder,
        menu::{Menu, MenuItem},
    };
    let menu = Menu::new();
    menu.append_items(&[
        &MenuItem::with_id("show", "Open picker", true, None),
        &MenuItem::with_id("library", "Open library", true, None),
        &MenuItem::with_id("hide", "Hide to tray", true, None),
        &MenuItem::with_id("quit", "Quit Picora", true, None),
    ])?;
    let mut rgba = vec![0u8; 32 * 32 * 4];
    for y in 4..28 {
        for x in 4..28 {
            let i = (y * 32 + x) * 4;
            rgba[i..i + 4].copy_from_slice(if (10..=21).contains(&x) && (9..=17).contains(&y) {
                &[240, 253, 250, 255]
            } else {
                &[20, 120, 112, 255]
            });
        }
    }
    Ok(TrayIconBuilder::new()
        .with_tooltip("Picora — sticker picker")
        .with_icon(Icon::from_rgba(rgba, 32, 32)?)
        .with_menu(Box::new(menu))
        .build()?)
}

#[cfg(test)]
mod library_tests;
