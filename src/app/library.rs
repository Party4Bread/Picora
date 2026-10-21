use super::{
    App, Filter, LibrarySticker, PackRef, Provider, RichText, Tab, View, apply_theme, design, egui,
    platform,
};
impl App {
    fn library_collections(&mut self, ui: &mut egui::Ui) {
        for (label, filter) in [("Favorites", Filter::Favorites), ("Recent", Filter::Recent)] {
            if design::nav_row(
                ui,
                label,
                self.tab == Tab::Library && self.library_state.filter == filter,
                None,
            )
            .clicked()
            {
                self.tab = Tab::Library;
                self.library_state.filter = filter;
            }
        }
        ui.add_space(8.);
        ui.label(RichText::new("Saved packs").small().weak());
        if self.packs.is_empty() {
            ui.label(RichText::new("No packs yet").small().weak());
        }
        for p in &self.packs {
            let filter = Filter::Pack(p.pack.reference.key());
            if design::nav_row(
                ui,
                &p.pack.title,
                self.tab == Tab::Library && self.library_state.filter == filter,
                Some(p.pack.stickers.len()),
            )
            .clicked()
            {
                self.tab = Tab::Library;
                self.library_state.filter = filter;
            }
        }
        if !self.groups.is_empty() {
            ui.add_space(8.);
            ui.label(RichText::new("Custom groups").small().weak());
            for g in &self.groups {
                let filter = Filter::Group(g.id);
                if design::nav_row(
                    ui,
                    &g.name,
                    self.tab == Tab::Library && self.library_state.filter == filter,
                    None,
                )
                .clicked()
                {
                    self.tab = Tab::Library;
                    self.library_state.filter = filter;
                }
            }
        }
    }
    fn library_sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("library-nav")
            .resizable(false)
            .exact_width(200.)
            .frame(
                egui::Frame::new()
                    .fill(ctx.style().visuals.faint_bg_color)
                    .inner_margin(12.),
            )
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 4.;
                ui.add_space(4.);
                ui.label(RichText::new("Picora").size(23.).strong());
                ui.add_space(8.);
                for (label, tab) in [
                    ("My library", Tab::Library),
                    ("Discover", Tab::Browse),
                    ("Groups", Tab::Groups),
                ] {
                    let selected = self.tab == tab
                        && (tab != Tab::Library || self.library_state.filter == Filter::All);
                    if design::nav_row(ui, label, selected, None).clicked() {
                        self.tab = tab;
                        if tab == Tab::Library {
                            self.library_state.filter = Filter::All;
                        }
                        if tab == Tab::Browse && self.catalog.is_empty() {
                            self.browse();
                        }
                    }
                }
                ui.add_space(8.);
                ui.separator();
                let height = (ui.available_height() - 128.).max(40.);
                egui::ScrollArea::vertical()
                    .id_salt("library-collections")
                    .max_height(height)
                    .show(ui, |ui| {
                        self.library_collections(ui);
                    });
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.label(RichText::new(&self.settings.shortcut).small().weak());
                    if design::nav_row(ui, "Open picker", false, None).clicked() {
                        self.target = None;
                        self.picker_open = true;
                        self.focus_search = true;
                        ctx.send_viewport_cmd_to(
                            egui::ViewportId::ROOT,
                            egui::ViewportCommand::Visible(true),
                        );
                        ctx.send_viewport_cmd_to(
                            egui::ViewportId::ROOT,
                            egui::ViewportCommand::Focus,
                        );
                    }
                    for (label, tab) in [("Settings", Tab::Settings), ("Accounts", Tab::Accounts)] {
                        if design::nav_row(ui, label, self.tab == tab, None).clicked() {
                            self.tab = tab;
                        }
                    }
                });
            });
    }
    pub(super) fn library(&mut self, ctx: &egui::Context) {
        self.library_sidebar(ctx);
        egui::TopBottomPanel::bottom("library-status").show(ctx, |ui| self.footer(ui));
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(ctx.style().visuals.panel_fill)
                    .inner_margin(24.),
            )
            .show(ctx, |ui| match self.tab {
                Tab::Library => self.library_content(ui),
                Tab::Browse => self.discover(ui),
                Tab::Groups => self.groups_ui(ui),
                Tab::Accounts => self.accounts(ui),
                Tab::Settings => {
                    egui::ScrollArea::vertical()
                        .id_salt("settings-scroll")
                        .show(ui, |ui| self.settings_ui(ui));
                }
            });
        if let Some(pack) = self.preview.clone() {
            let mut open = true;
            egui::Window::new(&pack.title)
                .id(egui::Id::new("pack-preview"))
                .open(&mut open)
                .default_width(560.)
                .default_height(480.)
                .show(ctx, |ui| {
                    ui.label(format!(
                        "{} · {} stickers · {}",
                        pack.reference.provider,
                        pack.stickers.len(),
                        pack.author
                    ));
                    ui.hyperlink_to("View on source website", &pack.url);
                    if ui
                        .add_enabled(
                            !self.downloading_pack,
                            egui::Button::new("Add pack to favorites · download for offline use"),
                        )
                        .clicked()
                    {
                        // Persist the user's choice before starting network work.
                        // A restart or partial download must not lose the favorite.
                        match self.store.save_favorite_pack(&pack) {
                            Ok(()) => {
                                self.refresh();
                                self.downloading_pack = true;
                                self.services.download_pack(pack.clone());
                            }
                            Err(e) => self.message(format!("Cannot save favorite: {e:#}"), true),
                        }
                    }
                    ui.separator();
                    let items: Vec<_> = pack
                        .stickers
                        .iter()
                        .map(|s| LibrarySticker {
                            pack_key: pack.reference.key(),
                            pack_title: pack.title.clone(),
                            provider: pack.reference.provider,
                            sticker: s.clone(),
                        })
                        .collect();
                    self.grid(ui, &items, View::Preview);
                });
            if !open {
                self.preview = None;
            }
        }
    }
    pub(super) fn library_content(&mut self, ui: &mut egui::Ui) {
        let title = self.collection_title(&self.library_state.filter);
        ui.heading(&title);
        ui.add_space(12.);
        ui.add_sized(
            [ui.available_width(), 36.],
            egui::TextEdit::singleline(&mut self.library_state.search)
                .hint_text("Search stickers…")
                .margin(egui::vec2(12., 8.)),
        );
        ui.add_space(8.);
        if let Filter::Pack(key) = self.library_state.filter.clone()
            && let Some(p) = self
                .packs
                .iter()
                .find(|p| p.pack.reference.key() == key)
                .cloned()
        {
            ui.menu_button("Pack options", |ui| {
                let mut favorite = p.favorite;
                if ui.checkbox(&mut favorite, "Favorite pack").changed() {
                    let r = self.store.set_pack_favorite(&key, favorite);
                    self.report(r, "Favorite updated");
                    self.refresh();
                }
                ui.hyperlink_to("Source", &p.pack.url);
                if ui.button("Remove from library").clicked() {
                    let r = self.store.remove_pack(&key);
                    self.report(
                        r,
                        "Pack removed; cached files retained for clipboard safety",
                    );
                    self.library_state.filter = Filter::All;
                    self.refresh();
                }
            });
        }
        if self.library_state.filter == Filter::Recent {
            ui.label("Recently used first, then newly added stickers.");
        }
        ui.label(
            RichText::new("Click to copy · Right-click for actions")
                .small()
                .weak(),
        );
        ui.add_space(8.);
        if let Some(items) = self.load_items(ui, View::Library) {
            ui.label(
                RichText::new(format!("{} stickers", items.len()))
                    .small()
                    .weak(),
            );
            self.grid(ui, &items, View::Library);
        }
    }
    pub(super) fn discover(&mut self, ui: &mut egui::Ui) {
        ui.heading("Discover stickers");
        ui.label("Find a pack, preview it, and make it yours.");
        ui.add_space(12.);
        ui.horizontal(|ui| {
            for source in [Provider::Dcinside, Provider::Arca] {
                if ui
                    .selectable_value(&mut self.source, source, source.to_string())
                    .changed()
                {
                    self.page = 1;
                    self.browse();
                }
            }
        });
        let mut submit = false;
        ui.horizontal(|ui| {
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.query)
                    .hint_text("Search packs or paste a pack URL")
                    .desired_width(ui.available_width() - 90.),
            );
            submit = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            submit |= ui
                .add_enabled(!self.browsing, egui::Button::new("Search"))
                .clicked();
        });
        if submit {
            if self.query.trim().starts_with("https://") {
                match PackRef::parse(&self.query) {
                    Ok(r) => {
                        self.preview_pack(r);
                    }
                    Err(e) => self.message(e.to_string(), true),
                }
            } else {
                self.page = 1;
                self.browse();
            }
        }
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    self.page > 1 && !self.browsing,
                    egui::Button::new("Previous"),
                )
                .clicked()
            {
                self.page -= 1;
                self.browse();
            }
            ui.label(format!("Page {}", self.page));
            if ui
                .add_enabled(
                    !self.catalog.is_empty() && !self.browsing,
                    egui::Button::new("Next"),
                )
                .clicked()
            {
                self.page += 1;
                self.browse();
            }
            if ui
                .add_enabled(!self.browsing, egui::Button::new("Refresh / retry"))
                .clicked()
            {
                self.failed.clear();
                self.browse();
            }
        });
        egui::ScrollArea::vertical().show(ui, |ui| {
            for pack in self.catalog.clone() {
                ui.horizontal(|ui| {
                    let r = self.thumb(ui, pack.reference.provider, &pack.thumbnail, 84., false);
                    ui.vertical(|ui| {
                        if ui.link(RichText::new(&pack.title).strong()).clicked() || r.activated {
                            self.preview_pack(pack.reference.clone());
                        }
                        ui.label(
                            RichText::new(format!("{} · {}", pack.reference.provider, pack.author))
                                .small()
                                .weak(),
                        );
                        ui.hyperlink_to("Source", &pack.url);
                    });
                });
                ui.separator();
            }
        });
    }
    pub(super) fn groups_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Your sticker groups");
        ui.label("Mix reactions from both repositories. Right-click a sticker to add it.");
        ui.add_space(12.);
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.group_name).hint_text("New group name"));
            if ui.button("Create group").clicked() {
                match self.store.create_group(&self.group_name) {
                    Ok(id) => {
                        self.chosen_group = Some(id);
                        self.group_name.trim().clone_into(&mut self.rename);
                        self.group_name.clear();
                        self.refresh();
                    }
                    Err(e) => self.message(e.to_string(), true),
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            for g in self.groups.clone() {
                if ui
                    .selectable_label(self.chosen_group == Some(g.id), &g.name)
                    .clicked()
                {
                    self.chosen_group = Some(g.id);
                    self.rename = g.name;
                }
            }
        });
        if let Some(id) = self.chosen_group {
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.rename);
                if ui.button("Rename").clicked() {
                    let r = self.store.rename_group(id, &self.rename);
                    self.report(r, "Group renamed");
                    self.refresh();
                }
                if ui.button("Delete group").clicked() {
                    let r = self.store.delete_group(id);
                    self.report(r, "Group deleted");
                    self.chosen_group = None;
                    self.refresh();
                }
            });
            if self.chosen_group != Some(id) {
                return;
            }
            if let Some(items) = self.load_items(ui, View::Group(id)) {
                self.grid(ui, &items, View::Group(id));
            }
        }
    }
    pub(super) fn accounts(&mut self, ui: &mut egui::Ui) {
        ui.heading("Accounts");
        ui.add_space(16.);
        ui.strong("DCInside");
        ui.label("Public packs can be browsed without signing in.");
        ui.add_space(24.);
        let account = self.services.account_status.lock().unwrap().clone();
        ui.horizontal_wrapped(|ui| {
            ui.strong("arca.live");
            let label = if self.login_busy && !account.checking() {
                "Waiting for sign-in…"
            } else {
                account.label()
            };
            ui.label(RichText::new(label).color(if account.error().is_some() {
                ui.visuals().error_fg_color
            } else {
                ui.visuals().text_color()
            }));
        });
        if account.verified() {
            ui.label("Catalog access verified.");
        }
        if let Some(error) = account.error() {
            ui.colored_label(ui.visuals().error_fg_color, error);
            ui.label("Check your connection, or sign in again to complete website verification.");
        }
        if let Some(error) = &account.storage_error {
            ui.colored_label(ui.visuals().error_fg_color, error);
            ui.label("You can use this session until Picora quits.");
        }
        ui.label("Sign in on arca.live’s own website. When you are done, close the login window to connect the session.");
        ui.label("Picora saves session cookies using OS-protected storage (Windows account encryption on Windows). Your password stays on the website.");
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(!self.login_busy, egui::Button::new("Sign in / reconnect"))
                .clicked()
            {
                self.login_busy = true;
                self.message(
                    "Complete sign-in in the arca.live window, then close it",
                    false,
                );
                self.services.login();
            }
            if ui
                .add_enabled(
                    !self.login_busy && account.has_session,
                    egui::Button::new("Check connection"),
                )
                .clicked()
            {
                self.login_busy = true;
                self.services.check_arca();
            }
            if ui
                .add_enabled(
                    !self.login_busy && (account.has_session || account.saved),
                    egui::Button::new("Sign out"),
                )
                .clicked()
            {
                let r = self.services.logout();
                self.report(r, "arca.live session removed");
            }
        });
    }
    pub(super) fn settings_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Settings");
        #[cfg(target_os = "macos")]
        {
            self.macos_permissions_ui(ui, platform::macos_permissions::accessibility_allowed());
            ui.add_space(16.);
        }
        self.updates_ui(ui);
        ui.add_space(16.);
        ui.strong("Global shortcut");
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.shortcut_text);
            if ui.button("Apply shortcut").clicked() {
                if self
                    .shortcut_text
                    .parse::<global_hotkey::hotkey::HotKey>()
                    .is_ok()
                {
                    self.register_shortcut();
                    if platform::is_wayland() || self.shortcut_status.starts_with("Active:") {
                        self.settings.shortcut.clone_from(&self.shortcut_text);
                        let r = self.store.save_settings(&self.settings);
                        self.report(r, "Shortcut saved");
                    }
                } else {
                    self.message("Use a shortcut such as Ctrl+Shift+Space", true);
                }
            }
        });
        ui.label(&self.shortcut_status);
        ui.add_space(16.);
        let mut changed = ui
            .checkbox(
                &mut self.settings.auto_paste,
                "Paste automatically after choosing a sticker",
            )
            .changed();
        ui.label("Picora never sends Enter. If input permission or focus restoration fails, paste manually.");
        if platform::is_wayland() {
            ui.add_space(12.);
            ui.label("Wayland requires your desktop’s permission to send the paste shortcut.");
            #[cfg(target_os = "linux")]
            if ui
                .button("Enable / reconnect Wayland keyboard access")
                .clicked()
            {
                self.services.enable_wayland(false);
                self.message("Approve keyboard access in the desktop dialog", false);
            }
        }
        ui.add_space(16.);
        let old = self.settings.launch_at_login;
        if ui
            .checkbox(&mut self.settings.launch_at_login, "Launch at login")
            .changed()
        {
            match std::env::current_exe()
                .map_err(anyhow::Error::from)
                .and_then(|p| platform::autostart::set(self.settings.launch_at_login, &p))
            {
                Ok(()) => changed = true,
                Err(e) => {
                    self.settings.launch_at_login = old;
                    self.message(e.to_string(), true);
                }
            }
        }
        ui.horizontal(|ui| {
            ui.label("Appearance");
            for theme in ["System", "Light", "Dark"] {
                changed |= ui
                    .selectable_value(&mut self.settings.theme, theme.into(), theme)
                    .changed();
            }
        });
        if changed {
            apply_theme(ui.ctx(), &self.settings.theme);
            let r = self.store.save_settings(&self.settings);
            self.report(r, "Settings saved");
        }
        ui.add_space(16.);
        if ui.button("Open data folder").clicked() {
            let r = open::that(&self.data_dir).map_err(anyhow::Error::from);
            self.report(r, "Opened data folder");
        }
        ui.label("Original files are retained so clipboard attachments remain available.");
        ui.add_space(16.);
        if ui.button("Quit Picora").clicked() {
            self.quit = true;
        }
    }

    #[cfg(target_os = "macos")]
    pub(super) fn macos_permissions_ui(&mut self, ui: &mut egui::Ui, allowed: bool) {
        ui.add_space(12.);
        ui.strong("Automatic paste on macOS");
        ui.label(if allowed {
            "Allowed — automatic paste has Accessibility access."
        } else {
            "Permission needed for automatic paste. Copying stickers works without it."
        });
        ui.label("Enable the copy of Picora you are running. Moving or updating the app may require granting access again.");
        ui.horizontal_wrapped(|ui| {
            if !allowed && ui.button("Allow automatic paste…").clicked() {
                platform::macos_permissions::request_accessibility();
                self.cancel_auto_paste();
                self.message("Enable Picora in System Settings → Privacy & Security → Accessibility, then return here. Choose the sticker again after granting access.", false);
                if let Err(error) = platform::macos_permissions::open_settings() {
                    self.message(format!("Could not open System Settings: {error}. Open Privacy & Security → Accessibility manually."), true);
                }
            }
            if ui.button("Open Accessibility settings").clicked()
                && let Err(error) = platform::macos_permissions::open_settings() {
                self.message(format!("Could not open System Settings: {error}. Open Privacy & Security → Accessibility manually."), true);
            }
            if ui.button("Check permission again").clicked() {
                self.cancel_auto_paste();
                self.message(if platform::macos_permissions::accessibility_allowed() {
                    "Accessibility is allowed. Return to your editor and choose the sticker again."
                } else {
                    "Accessibility is still unavailable. Enable this copy of Picora; if it is already enabled, quit and reopen Picora, then check again. You can still paste manually with Cmd+V."
                }, false);
            }
        });
        if !allowed {
            ui.label("Running from source? macOS may list the terminal or launcher instead.");
        }
    }
}
