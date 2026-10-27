mod design;
mod events;
mod giphy;
mod grid;
#[cfg(target_os = "linux")]
mod ime;
mod library;
mod selection;
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
use rust_i18n::t;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};

#[derive(Clone, PartialEq, Hash)]
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
    Preview,
    Discover,
}
#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Library,
    Browse,
    Accounts,
    Settings,
}
/// An inline group name editor in the library sidebar; `group` is `None` for a new group.
struct GroupEdit {
    group: Option<i64>,
    name: String,
    focus: bool,
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
    giphy: giphy::State,
    selection: selection::Selection,
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
    /// The picker's size with its title bar, recorded when it was last hidden.
    /// Hidden windows may report their size without one.
    picker_frame_size: Option<Vec2>,
    /// XWayland and X11 window managers may draw a title bar that winit
    /// leaves out of the window size; it is looked up through this window.
    #[cfg(target_os = "linux")]
    x11_window: Option<u32>,
    focus_search: bool,
    status: String,
    error: bool,
    browsing: bool,
    downloading_pack: bool,
    login_busy: bool,
    group_edit: Option<GroupEdit>,
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
    tray: Option<Tray>,
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
        settings.language.apply();
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
            giphy: giphy::State::default(),
            selection: selection::Selection::default(),
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
            picker_frame_size: None,
            #[cfg(target_os = "linux")]
            x11_window: {
                use raw_window_handle::{HasWindowHandle, RawWindowHandle};
                cc.window_handle()
                    .ok()
                    .and_then(|handle| match handle.as_raw() {
                        RawWindowHandle::Xlib(h) => u32::try_from(h.window).ok(),
                        RawWindowHandle::Xcb(h) => Some(h.window.get()),
                        _ => None,
                    })
            },
            focus_search: true,
            status: t!("Choose a sticker to copy and paste").into(),
            error: false,
            browsing: false,
            downloading_pack: false,
            login_busy: false,
            group_edit: None,
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
            app.updates
                .check(&app.services.runtime, ctx, app.settings.update_channel);
        }
        app.services
            .set_download_limit(app.settings.simultaneous_downloads);
        app.services.load_giphy_key();
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
                    t!(
                        "Tray unavailable: %{error}. Use picora --show to reopen.",
                        error = e
                    ),
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
                    anyhow::anyhow!(t!("Desktop shortcut session ended; reconnect in Settings"))
                });
                bus.send(Event::ShortcutRegistered(id, requested, false, Err(error)));
            }));
            self.shortcut_status = t!("Waiting for desktop shortcut permission…").into();
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
                self.shortcut_status = t!("Active: %{shortcut}", shortcut = requested).into();
                if persist {
                    self.settings.shortcut = requested;
                    let result = self.store.save_settings(&self.settings);
                    self.report(result, &t!("Shortcut saved"));
                }
            }
            Err(error) => {
                eprintln!("Shortcut unavailable: {error:#}");
                self.shortcut_status = format!("{error:#}");
                self.message(
                    t!(
                        "Shortcut unavailable: %{error}",
                        error = format!("{error:#}")
                    ),
                    true,
                );
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
        let position = self.picker_position(ctx);
        // Window managers may place a window again when it is shown, and some
        // ignore moves of hidden windows, so move it before and after.
        let move_picker = || {
            if let Some(position) = position {
                ctx.send_viewport_cmd_to(
                    egui::ViewportId::ROOT,
                    egui::ViewportCommand::OuterPosition(position),
                );
            }
        };
        move_picker();
        self.focus_picker(ctx);
        move_picker();
    }
    /// Where the picker should open, or `None` to leave it where it was.
    fn picker_position(&self, ctx: &egui::Context) -> Option<egui::Pos2> {
        use crate::model::PickerPosition;
        let position = self.settings.picker_position.effective();
        if position == PickerPosition::Last {
            return None;
        }
        let anchor = platform::placement::locate(
            position == PickerPosition::TextCursor,
            &self.services.runtime,
        )?;
        let (pixels_per_point, native) = ctx.input(|i| {
            (
                i.pixels_per_point,
                i.viewport().native_pixels_per_point.unwrap_or(1.),
            )
        });
        // Screen units per egui point: winit positions windows in physical
        // pixels, and macOS reports the screen in points.
        let units = if cfg!(target_os = "macos") {
            pixels_per_point / native
        } else {
            pixels_per_point
        };
        let size = self
            .picker_frame_size
            .or_else(|| self.frame_size(ctx))
            .unwrap_or(egui::vec2(460., 540.))
            * units;
        let corner = platform::placement::place(anchor, size, 6. * units);
        Some((corner.to_vec2() / units).to_pos2())
    }
    fn focus_picker(&mut self, ctx: &egui::Context) {
        self.focus_search = true;
        #[cfg(target_os = "macos")]
        if let Err(error) = focus::activate_current_app() {
            eprintln!("Could not focus the sticker window: {error:#}");
            self.message(
                t!("Click the sticker window to start typing. If it is hidden, choose Open picker from Picora’s menu bar icon."),
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
    /// The picker's current size including its title bar, in points.
    fn frame_size(&self, ctx: &egui::Context) -> Option<Vec2> {
        let (outer, inner) = ctx.input(|i| (i.viewport().outer_rect, i.viewport().inner_rect));
        let frame = match (outer, inner) {
            (Some(outer), Some(inner)) => outer.union(inner),
            (outer, inner) => outer.or(inner)?,
        };
        #[cfg(target_os = "linux")]
        if outer == inner
            && let Some(extra) = self.x11_window.and_then(platform::placement::frame_extents)
        {
            return Some(frame.size() + extra / ctx.pixels_per_point());
        }
        Some(frame.size())
    }
    fn set_picker_hidden(&mut self, ctx: &egui::Context) {
        if self.picker_open {
            self.picker_frame_size = self.frame_size(ctx).or(self.picker_frame_size);
        }
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
        self.selection.clear();
        self.browsing = true;
        self.services.preview(reference, self.catalog_id);
    }
    fn browse(&mut self) {
        self.catalog_id += 1;
        self.browsing = true;
        self.catalog.clear();
        self.giphy.results.clear();
        self.giphy.more = false;
        self.preview = None;
        self.selection.clear();
        if self.source != Provider::Giphy {
            self.services
                .browse(self.source, self.query.clone(), self.page, self.catalog_id);
        } else if self.giphy.key.is_empty() {
            // Discover asks for a key; a search would only fail.
            self.browsing = false;
        } else {
            self.services.search_giphy(
                self.giphy.key.clone(),
                self.giphy.kind,
                self.query.clone(),
                self.page,
                self.catalog_id,
            );
        }
    }
    fn collection_title(&self, filter: &Filter) -> String {
        match filter {
            Filter::All => t!("All stickers").into(),
            Filter::Favorites => t!("Favorites").into(),
            Filter::Recent => t!("Recent").into(),
            Filter::Pack(key) => self
                .packs
                .iter()
                .find(|p| p.pack.reference.key() == *key)
                .map_or_else(|| t!("All stickers").into(), |p| p.pack.title.clone()),
            Filter::Group(id) => self
                .groups
                .iter()
                .find(|g| g.id == *id)
                .map_or_else(|| t!("All stickers").into(), |g| g.name.clone()),
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
            View::Preview | View::Discover => {
                unreachable!("Previews and online results supply their own stickers")
            }
        };
        match result {
            Ok(items) => Some(items),
            Err(error) => {
                let message = t!(
                    "Could not load stickers: %{error}",
                    error = format!("{error:#}")
                )
                .into_owned();
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
        self.message(t!("Preparing clipboard…"), false);
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
                (t!("Favorites"), Filter::Favorites),
                (t!("Recent"), Filter::Recent),
                (t!("All"), Filter::All),
            ] {
                if ui
                    .selectable_value(&mut self.picker_state.filter, filter, label)
                    .changed()
                {
                    self.picker_state.selected = 0;
                }
            }
        });
    }
    fn picker_shortcuts(&mut self, ui: &mut egui::Ui) {
        if self.packs.is_empty() && self.groups.is_empty() {
            return;
        }
        egui::TopBottomPanel::bottom("picker-packs")
            .exact_height(48.)
            .show_separator_line(false)
            .show_inside(ui, |ui| {
                egui::ScrollArea::horizontal()
                    .id_salt("pack-shortcuts")
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for p in self.packs.clone() {
                                let cover = (p.pack.reference.provider, p.pack.thumbnail);
                                let filter = Filter::Pack(p.pack.reference.key());
                                self.picker_shortcut(ui, filter, &p.pack.title, Some(cover));
                            }
                            if !self.packs.is_empty() && !self.groups.is_empty() {
                                ui.separator();
                            }
                            for g in self.groups.clone() {
                                let cover = g.cover.map(|c| (c.provider, c.sticker.url));
                                self.picker_shortcut(ui, Filter::Group(g.id), &g.name, cover);
                            }
                        });
                    });
            });
    }
    fn picker_shortcut(
        &mut self,
        ui: &mut egui::Ui,
        filter: Filter,
        title: &str,
        cover: Option<(Provider, String)>,
    ) {
        ui.push_id(&filter, |ui| {
            let selected = self.picker_state.filter == filter;
            let (response, activated) = match cover.filter(|(_, url)| !url.is_empty()) {
                Some((provider, url)) => {
                    let t = self.thumb(ui, provider, &url, 38., selected);
                    (t.response, t.activated)
                }
                // Empty groups have no sticker to show; use their initial.
                None => {
                    let initial = title.chars().next().unwrap_or('?').to_uppercase();
                    let r = ui.add(
                        egui::Button::selectable(selected, initial.to_string())
                            .min_size(Vec2::splat(38.)),
                    );
                    let clicked = r.clicked();
                    (r, clicked)
                }
            };
            response
                .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, title));
            response.on_hover_text(title);
            if activated {
                self.picker_state.filter = filter.clone();
                self.picker_state.selected = 0;
            }
        });
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
                            .add(egui::Button::new(t!("Library")).frame_when_inactive(false))
                            .clicked()
                        {
                            self.open_library();
                        }
                    });
                });
                self.update_notice(ui);
                #[cfg(target_os = "macos")]
                if self.settings.auto_paste && !platform::macos_permissions::accessibility_allowed()
                {
                    ui.label(t!(
                        "Automatic paste needs macOS Accessibility permission. Copying still works."
                    ));
                    if ui.button(t!("Set up automatic paste")).clicked() {
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
                        .hint_text(t!("Search stickers…"))
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
                self.picker_shortcuts(ui);
                ui.add_space(4.);
                let Some(items) = self.load_items(ui, View::Picker) else {
                    return;
                };
                if ctx.content_rect().height() >= 400. {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(self.collection_title(&self.picker_state.filter))
                                    .small()
                                    .strong(),
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
                    bus.send(Event::UpdateHealthFailed(
                        t!(
                            "Update startup acknowledgement failed: %{error}",
                            error = format!("{e:#}")
                        )
                        .into_owned(),
                    ));
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
        if let Some(tray) = &mut self.tray {
            tray.follow_language();
        }
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
                    .with_title(t!("Picora · Library"))
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
/// The tray keeps native menu labels, so it relabels them when the interface
/// language changes.
struct Tray {
    icon: tray_icon::TrayIcon,
    items: [tray_icon::menu::MenuItem; 4],
    locale: String,
}
impl Tray {
    fn labels() -> [std::borrow::Cow<'static, str>; 4] {
        [
            t!("Open picker"),
            t!("Open library"),
            t!("Hide to tray"),
            t!("Quit Picora"),
        ]
    }
    fn tooltip() -> std::borrow::Cow<'static, str> {
        t!("Picora — sticker picker")
    }
    /// Called every frame; only a language change touches the native menu.
    fn follow_language(&mut self) {
        let locale = rust_i18n::locale();
        if *locale == *self.locale {
            return;
        }
        self.locale = locale.to_string();
        for (item, label) in self.items.iter().zip(Self::labels()) {
            item.set_text(label);
        }
        if let Err(error) = self.icon.set_tooltip(Some(Self::tooltip())) {
            eprintln!("Could not update the tray tooltip: {error}");
        }
    }
}
fn make_tray() -> Result<Tray> {
    use tray_icon::{
        Icon, TrayIconBuilder,
        menu::{Menu, MenuItem},
    };
    let [show, library, hide, quit] = Tray::labels();
    let items = [
        MenuItem::with_id("show", show, true, None),
        MenuItem::with_id("library", library, true, None),
        MenuItem::with_id("hide", hide, true, None),
        MenuItem::with_id("quit", quit, true, None),
    ];
    let menu = Menu::new();
    menu.append_items(&[&items[0], &items[1], &items[2], &items[3]])?;
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
    let icon = TrayIconBuilder::new()
        .with_tooltip(Tray::tooltip())
        .with_icon(Icon::from_rgba(rgba, 32, 32)?)
        .with_menu(Box::new(menu))
        .build()?;
    Ok(Tray {
        icon,
        items,
        locale: rust_i18n::locale().to_string(),
    })
}

#[cfg(test)]
mod library_tests;
