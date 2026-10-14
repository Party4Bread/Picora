use super::*;
impl App {
    pub(super) fn library(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("library-nav")
            .resizable(false)
            .default_width(164.)
            .show(ctx, |ui| {
                ui.add_space(10.);
                ui.heading("Picora");
                ui.label(RichText::new("A home for your reactions").small().weak());
                ui.add_space(24.);
                for (label, tab) in [
                    ("My library", Tab::Library),
                    ("Discover", Tab::Browse),
                    ("Groups", Tab::Groups),
                    ("Accounts", Tab::Accounts),
                    ("Settings", Tab::Settings),
                ] {
                    if ui
                        .add_sized(
                            [144., 34.],
                            egui::Button::new(label).selected(self.tab == tab),
                        )
                        .clicked()
                    {
                        self.tab = tab;
                        if tab == Tab::Browse && self.catalog.is_empty() {
                            self.browse();
                        }
                    }
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    if ui.button("Open picker").clicked() {
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
                    ui.label(RichText::new(&self.settings.shortcut).small().weak());
                });
            });
        egui::TopBottomPanel::bottom("library-status").show(ctx, |ui| self.footer(ui));
        egui::CentralPanel::default().show(ctx, |ui| match self.tab {
            Tab::Library => self.library_content(ui),
            Tab::Browse => self.discover(ui),
            Tab::Groups => self.groups_ui(ui),
            Tab::Accounts => self.accounts(ui),
            Tab::Settings => self.settings_ui(ui),
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
                            !self.installing,
                            egui::Button::new("Add pack to favorites · download for offline use"),
                        )
                        .clicked()
                    {
                        self.installing = true;
                        self.services.install(pack.clone());
                    }
                    ui.separator();
                    let items = pack
                        .stickers
                        .iter()
                        .map(|s| LibrarySticker {
                            pack_key: pack.reference.key(),
                            pack_title: pack.title.clone(),
                            provider: pack.reference.provider,
                            sticker: s.clone(),
                        })
                        .collect();
                    self.grid(ui, items, false);
                });
            if !open {
                self.preview = None;
            }
        }
    }
    pub(super) fn library_content(&mut self, ui: &mut egui::Ui) {
        ui.heading("My library");
        ui.label("Downloaded packs stay available when you’re offline.");
        ui.add_space(12.);
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("saved-pack")
                .width(280.)
                .selected_text(match &self.filter {
                    Filter::Pack(k) => self
                        .packs
                        .iter()
                        .find(|p| p.pack.reference.key() == *k)
                        .map(|p| p.pack.title.as_str())
                        .unwrap_or("All stickers"),
                    _ => "All stickers",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.filter, Filter::All, "All stickers");
                    for p in &self.packs {
                        ui.selectable_value(
                            &mut self.filter,
                            Filter::Pack(p.pack.reference.key()),
                            &p.pack.title,
                        );
                    }
                });
            ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Search library"));
        });
        if let Filter::Pack(key) = self.filter.clone()
            && let Some(p) = self
                .packs
                .iter()
                .find(|p| p.pack.reference.key() == key)
                .cloned()
        {
            ui.horizontal(|ui| {
                let mut favorite = p.favorite;
                if ui.checkbox(&mut favorite, "Favorite pack").changed() {
                    let r = self.store.favorite(&key, favorite);
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
                    self.filter = Filter::All;
                    self.refresh();
                }
            });
        }
        self.grid(ui, self.items(), false);
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
                        self.browsing = true;
                        self.services.preview(r);
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
                        if ui.link(RichText::new(&pack.title).strong()).clicked() || r.clicked() {
                            self.browsing = true;
                            self.services.preview(pack.reference.clone());
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
                        self.rename = self.group_name.trim().to_owned();
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
                self.filter = Filter::All;
                return;
            }
            self.filter = Filter::Group(id);
            let saved = std::mem::take(&mut self.search);
            self.grid(ui, self.items(), false);
            self.search = saved;
        }
    }
    pub(super) fn accounts(&mut self, ui: &mut egui::Ui) {
        ui.heading("Accounts");
        ui.add_space(16.);
        ui.strong("DCInside");
        ui.label("Public packs can be browsed without signing in.");
        ui.add_space(24.);
        ui.strong("Arca");
        ui.label("Sign in on Arca’s own website. When you are done, close the login window to connect the session.");
        ui.label("Picora saves session cookies using OS-protected storage (Windows account encryption on Windows). Your password stays on the website.");
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!self.login_busy, egui::Button::new("Sign in / reconnect"))
                .clicked()
            {
                self.login_busy = true;
                self.message("Complete sign-in in the Arca window, then close it", false);
                self.services.login();
            }
            if ui
                .add_enabled(!self.login_busy, egui::Button::new("Sign out"))
                .clicked()
            {
                let r = self.services.logout();
                self.report(r, "Arca session removed");
            }
        });
    }
    pub(super) fn settings_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Settings");
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
                        self.settings.shortcut = self.shortcut_text.clone();
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
                self.services.enable_wayland();
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
}
