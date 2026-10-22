mod design;
mod events;
mod giphy;
mod grid;
#[cfg(target_os = "linux")]
mod ime;
mod library;
mod updates;
use crate::{
    media::CopyMode,
    model::{Group, LibrarySticker, Pack, PackRef, Provider, SavedPack, Settings},
    platform::{
        self,
        focus::{self, Target},
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
struct BrowseState {
    filter: Filter,
    search: String,
    selected: usize,
}
impl BrowseState {
    fn new(filter: Filter) -> Self {
        Self {
            filter,
            search: String::new(),
            selected: 0,
        }
    }
}
#[derive(Clone, Copy, PartialEq)]
enum View {
    Library,
    Picker,
    Group(i64),
    Preview,
}
#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Library,
    Giphy,
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
    auto_paste: bool,
    target: Option<Target>,
    from_picker: bool,
}
pub struct App {
    giphy: giphy::Panel,
    updates: crate::update::controller::Controller,
    update_health_sent: bool,
    store: Store,
    services: Services,
    rx: mpsc::Receiver<Event>,
    activation: mpsc::Receiver<bool>,
    settings: Settings,
    packs: Vec<SavedPack>,
    groups: Vec<Group>,
    library_state: BrowseState,
    picker_state: BrowseState,
    tab: Tab,
    query: String,
    source: Provider,
    page: u32,
    catalog: Vec<Pack>,
    catalog_id: u64,
    preview: Option<Pack>,
    library_open: bool,
    picker_open: bool,
    focus_search: bool,
    status: String,
    error: bool,
    browsing: bool,
    downloading_pack: bool,
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
    shortcut_generation: u64,
    #[cfg(target_os = "linux")]
    portal_task: Option<tokio::task::JoinHandle<()>>,
    tray: Option<tray_icon::TrayIcon>,
    quit: bool,
    data_dir: PathBuf,
}
impl App {
    /// # Errors
    /// Returns an error if the library database, cache directory, or repository clients cannot initialize.
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
        ctx.set_theme(settings.theme);
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
            giphy: giphy::Panel::default(),
            updates: crate::update::controller::Controller::default(),
            update_health_sent: false,
            packs: store.packs()?,
            groups: store.groups()?,
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
            library_open: library,
            picker_open: !background && !library,
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
        };
        if app.packs.is_empty() && !background {
            app.library_open = true;
            app.picker_open = false;
        }
        if app.settings.check_updates_automatically {
            app.updates.check(&app.services.runtime, ctx);
        }
        app.register_shortcut(false);
        #[cfg(target_os = "linux")]
        if platform::is_wayland() {
            app.services.enable_wayland(true);
        }
        match make_tray() {
            Ok(tray) => app.tray = Some(tray),
            Err(e) => {
                app.library_open = true;
                app.message(
                    format!("Tray unavailable: {e}. Use picora --show to reopen."),
                    true,
                );
            }
        }
        ctx.send_viewport_cmd_to(
            egui::ViewportId::ROOT,
            egui::ViewportCommand::Visible(app.picker_open),
        );
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
                self.groups = g;
            }
            (Err(e), _) | (_, Err(e)) => self.message(format!("{e:#}"), true),
        }
    }
    fn register_native_shortcut(&mut self) -> Result<()> {
        if let Some(shortcut) = &mut self.shortcut {
            shortcut.replace(&self.shortcut_text)
        } else {
            self.shortcut = Some(Shortcut::new(&self.shortcut_text)?);
            Ok(())
        }
    }
    fn register_shortcut(&mut self, persist: bool) {
        self.shortcut_generation += 1;
        let id = self.shortcut_generation;
        let requested = self.shortcut_text.clone();
        #[cfg(target_os = "linux")]
        if platform::is_wayland() {
            if let Some(task) = self.portal_task.take() {
                task.abort();
            }
            let bus = self.services.bus.clone();
            let trigger = requested
                .replace("Ctrl", "CTRL")
                .replace("Shift", "SHIFT")
                .replace("Space", "space");
            self.portal_task = Some(self.services.runtime.spawn(async move {
                let activate = bus.clone();
                let ready = bus.clone();
                let ready_shortcut = requested.clone();
                let result = platform::wayland::shortcuts(
                    trigger,
                    move || {
                        ready.send(Event::ShortcutRegistered(
                            id,
                            ready_shortcut,
                            persist,
                            Ok(()),
                        ));
                    },
                    move || activate.send(Event::Show),
                )
                .await;
                let error = result.err().unwrap_or_else(|| {
                    anyhow::anyhow!("Desktop shortcut session ended; reconnect in Settings")
                });
                bus.send(Event::ShortcutRegistered(id, requested, false, Err(error)));
            }));
            self.shortcut_status = "Waiting for desktop shortcut permission…".into();
            return;
        }
        let result = self.register_native_shortcut();
        self.shortcut_registered(id, requested, persist, result);
    }
    fn shortcut_registered(
        &mut self,
        id: u64,
        requested: String,
        persist: bool,
        result: Result<()>,
    ) {
        if id != self.shortcut_generation {
            return;
        }
        match result {
            Ok(()) => {
                self.shortcut_status = format!("Active: {requested}");
                if persist {
                    self.settings.shortcut = requested;
                    let result = self.store.save_settings(&self.settings);
                    self.report(result, "Shortcut saved");
                }
            }
            Err(error) => {
                eprintln!("Shortcut unavailable: {error:#}");
                self.shortcut_status = format!("{error:#}");
                self.message(format!("Shortcut unavailable: {error:#}"), true);
            }
        }
    }
    fn cancel_auto_paste(&mut self) {
        self.paste_at = None;
        self.services.cancel_paste();
        if let Some(pending) = &mut self.pending {
            pending.auto_paste = false;
        }
    }
    fn show_picker(&mut self, ctx: &egui::Context) {
        self.cancel_auto_paste();
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
        self.picker_state.selected = 0;
        self.focus_picker(ctx);
    }
    fn focus_picker(&mut self, ctx: &egui::Context) {
        self.focus_search = true;
        #[cfg(target_os = "macos")]
        if let Err(error) = focus::activate_current_app() {
            eprintln!("Could not focus the sticker window: {error:#}");
            self.message(
                "Click the sticker window to start typing. If it is hidden, choose Open picker from Picora’s menu bar icon.",
                false,
            );
        }
        ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Focus);
    }
    fn hide_picker(&mut self, ctx: &egui::Context) {
        self.cancel_auto_paste();
        self.set_picker_hidden(ctx);
    }
    fn set_picker_hidden(&mut self, ctx: &egui::Context) {
        self.picker_open = false;
        ctx.send_viewport_cmd_to(
            egui::ViewportId::ROOT,
            egui::ViewportCommand::Visible(false),
        );
    }
    fn open_library(&mut self) {
        self.services.bus.ctx.send_viewport_cmd_to(
            egui::ViewportId::from_hash_of("library"),
            egui::ViewportCommand::Focus,
        );
        self.cancel_auto_paste();
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
    fn collection_title(&self, filter: &Filter) -> String {
        match filter {
            Filter::All => "All stickers".into(),
            Filter::Favorites => "Favorites".into(),
            Filter::Recent => "Recent".into(),
            Filter::Pack(key) => self
                .packs
                .iter()
                .find(|p| p.pack.reference.key() == *key)
                .map_or_else(|| "All stickers".into(), |p| p.pack.title.clone()),
            Filter::Group(id) => self
                .groups
                .iter()
                .find(|g| g.id == *id)
                .map_or_else(|| "All stickers".into(), |g| g.name.clone()),
        }
    }
    fn items(&self, filter: &Filter, search: &str) -> Result<Vec<LibrarySticker>> {
        let items = match filter {
            Filter::Group(id) => self.store.group_items(*id)?,
            Filter::Recent => self.store.recents()?,
            Filter::Favorites => self.store.favorite_items()?,
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
        let query = search.trim().to_lowercase();
        Ok(items
            .into_iter()
            .filter(|s| {
                query.is_empty()
                    || s.sticker.label.to_lowercase().contains(&query)
                    || s.pack_title.to_lowercase().contains(&query)
            })
            .collect())
    }
    fn load_items(&mut self, ui: &mut egui::Ui, view: View) -> Option<Vec<LibrarySticker>> {
        let result = match view {
            View::Library => self.items(&self.library_state.filter, &self.library_state.search),
            View::Picker => self.items(&self.picker_state.filter, &self.picker_state.search),
            View::Group(id) => self.items(&Filter::Group(id), ""),
            View::Preview => unreachable!("Previews supply their own stickers"),
        };
        match result {
            Ok(items) => Some(items),
            Err(error) => {
                let message = format!("Could not load stickers: {error:#}");
                ui.colored_label(ui.visuals().error_fg_color, &message);
                self.message(message, true);
                None
            }
        }
    }
    fn begin_sticker_copy(&mut self, item: LibrarySticker, mode: CopyMode, from_picker: bool) {
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
            auto_paste: self.settings.auto_paste,
            target,
            from_picker,
        });
        self.message("Preparing clipboard…", false);
        self.services.copy(item, mode);
    }
    fn report(&mut self, result: Result<()>, success: &str) {
        match result {
            Ok(()) => self.message(success, false),
            Err(e) => self.message(format!("{e:#}"), true),
        }
    }
    fn footer(&self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if self.pending.is_some() || self.downloading_pack || self.browsing || self.login_busy {
                ui.spinner();
            }
            ui.label(RichText::new(&self.status).size(12.).color(if self.error {
                ui.visuals().error_fg_color
            } else {
                ui.visuals().weak_text_color()
            }));
        });
    }
    fn picker_collections(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            for (label, filter) in [
                ("Favorites", Filter::Favorites),
                ("Recent", Filter::Recent),
                ("All", Filter::All),
            ] {
                if ui
                    .selectable_value(&mut self.picker_state.filter, filter, label)
                    .changed()
                {
                    self.picker_state.selected = 0;
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
                                &mut self.picker_state.filter,
                                Filter::Pack(p.pack.reference.key()),
                                &p.pack.title,
                            )
                            .changed()
                        {
                            self.picker_state.selected = 0;
                        }
                    }
                    if !self.groups.is_empty() {
                        ui.separator();
                        ui.label("Custom groups");
                        for g in &self.groups {
                            if ui
                                .selectable_value(
                                    &mut self.picker_state.filter,
                                    Filter::Group(g.id),
                                    &g.name,
                                )
                                .changed()
                            {
                                self.picker_state.selected = 0;
                            }
                        }
                    }
                });
        });
    }
    fn picker_pack_shortcuts(&mut self, ui: &mut egui::Ui) {
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
                                        let thumbnail = self.thumb(
                                            ui,
                                            p.pack.reference.provider,
                                            &p.pack.thumbnail,
                                            38.,
                                            self.picker_state.filter == filter,
                                        );
                                        thumbnail.response.on_hover_text(&p.pack.title);
                                        if thumbnail.activated {
                                            self.picker_state.filter = filter;
                                            self.picker_state.selected = 0;
                                        }
                                    });
                                }
                            });
                        });
                });
        }
    }
    fn picker(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(ctx.style().visuals.panel_fill)
                    .inner_margin(12.),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let title = if ctx.content_rect().height() < 400.
                        && matches!(self.picker_state.filter, Filter::Pack(_) | Filter::Group(_))
                    {
                        self.collection_title(&self.picker_state.filter)
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
                self.update_notice(ui);
                #[cfg(target_os = "macos")]
                if self.settings.auto_paste
                    && !platform::macos_permissions::accessibility_allowed()
                {
                    ui.label("Automatic paste needs macOS Accessibility permission. Copying still works.");
                    if ui.button("Set up automatic paste").clicked() {
                        self.hide_picker(ctx);
                        self.open_library();
                        self.tab = Tab::Settings;
                    }
                    ui.add_space(8.);
                }
                let search_id = egui::Id::new("picker-search");
                if self.focus_search && ui.input(|i| i.focused) {
                    // Focus before TextEdit processes this frame's text/IME
                    // events, and retain the request while the window activates.
                    ui.memory_mut(|memory| memory.request_focus(search_id));
                    self.focus_search = false;
                }
                let search = ui.add_sized(
                    [ui.available_width(), 36.],
                    egui::TextEdit::singleline(&mut self.picker_state.search)
                        .id(search_id)
                        .hint_text("Search stickers…")
                        .margin(egui::vec2(12., 8.))
                        .desired_width(f32::INFINITY),
                );
                if ui.input(|i| {
                    i.key_pressed(egui::Key::ArrowDown) || i.key_pressed(egui::Key::ArrowUp)
                }) {
                    search.surrender_focus();
                }
                self.picker_collections(ui);
                egui::TopBottomPanel::bottom("picker-status").show_inside(ui, |ui| self.footer(ui));
                self.picker_pack_shortcuts(ui);
                ui.add_space(4.);
                let Some(items) = self.load_items(ui, View::Picker) else { return; };
                if ctx.content_rect().height() >= 400. {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(self.collection_title(&self.picker_state.filter)).small().strong(),
                            )
                            .truncate(),
                        );
                        ui.label(RichText::new(items.len().to_string()).small().weak());
                    });
                    ui.add_space(4.);
                }
                self.grid(ui, &items, View::Picker);
            });
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
            self.cancel_auto_paste();
            self.pending = None;
            self.quit = true;
        }
        if matches!(
            self.updates.state,
            crate::update::controller::State::HandingOff
        ) {
            self.cancel_auto_paste();
            self.pending = None;
        }
        self.handle_events(ctx);
        self.giphy.poll(&self.services.runtime, ctx);
        if self.quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.hide_picker(ctx);
        }
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
