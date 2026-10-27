use super::{
    App, Filter, GroupEdit, LibrarySticker, PackRef, Provider, RichText, Tab, View, design, egui,
    platform,
};
use rust_i18n::t;
impl App {
    fn library_collections(&mut self, ui: &mut egui::Ui) {
        for (label, filter) in [
            (t!("Favorites"), Filter::Favorites),
            (t!("Recent"), Filter::Recent),
        ] {
            if design::nav_row(
                ui,
                &label,
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
        ui.label(RichText::new(t!("Saved packs")).small().weak());
        if self.packs.is_empty() {
            ui.label(RichText::new(t!("No packs yet")).small().weak());
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
        ui.add_space(8.);
        ui.horizontal(|ui| {
            ui.label(RichText::new(t!("Custom groups")).small().weak());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(egui::Button::new("+").small().frame_when_inactive(false))
                    .on_hover_text(t!("New group"))
                    .clicked()
                {
                    self.edit_group(None, String::new());
                }
            });
        });
        let creating = self.group_edit.as_ref().is_some_and(|e| e.group.is_none());
        if self.groups.is_empty() && !creating {
            ui.label(RichText::new(t!("No groups yet")).small().weak());
        }
        for g in self.groups.clone() {
            if self
                .group_edit
                .as_ref()
                .is_some_and(|e| e.group == Some(g.id))
            {
                self.group_name_editor(ui);
                continue;
            }
            let filter = Filter::Group(g.id);
            let row = design::nav_row(
                ui,
                &g.name,
                self.tab == Tab::Library && self.library_state.filter == filter,
                None,
            );
            if row.clicked() {
                self.tab = Tab::Library;
                self.library_state.filter = filter.clone();
            }
            row.context_menu(|ui| {
                if ui.button(t!("Rename")).clicked() {
                    self.edit_group(Some(g.id), g.name.clone());
                    ui.close();
                }
                if ui.button(t!("Delete group")).clicked() {
                    let result = self.store.delete_group(g.id);
                    self.report(
                        result,
                        &t!("Group deleted. Its stickers stay in your library."),
                    );
                    for state in [&mut self.library_state, &mut self.picker_state] {
                        if state.filter == filter {
                            state.filter = Filter::All;
                        }
                    }
                    self.refresh();
                    ui.close();
                }
            });
        }
        if creating {
            self.group_name_editor(ui);
        }
    }
    fn edit_group(&mut self, group: Option<i64>, name: String) {
        self.group_edit = Some(GroupEdit {
            group,
            name,
            focus: true,
        });
    }
    /// Enter or clicking away commits; Escape or an empty name cancels.
    fn group_name_editor(&mut self, ui: &mut egui::Ui) {
        let Some(edit) = &mut self.group_edit else {
            return;
        };
        let response = ui.add(
            egui::TextEdit::singleline(&mut edit.name)
                .hint_text(t!("Group name"))
                .desired_width(f32::INFINITY),
        );
        if std::mem::take(&mut edit.focus) {
            response.request_focus();
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.group_edit = None;
            return;
        }
        if !response.lost_focus() {
            return;
        }
        let Some(edit) = self.group_edit.take() else {
            return;
        };
        if edit.name.trim().is_empty() {
            return;
        }
        let result = match edit.group {
            Some(id) => self.store.rename_group(id, &edit.name),
            None => self.store.create_group(&edit.name).map(|id| {
                self.tab = Tab::Library;
                self.library_state.filter = Filter::Group(id);
            }),
        };
        self.report(
            result,
            &if edit.group.is_some() {
                t!("Group renamed")
            } else {
                t!("Group created. Right-click stickers to add them.")
            },
        );
        self.refresh();
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
                self.update_notice(ui);
                ui.add_space(8.);
                for (label, tab) in [
                    (t!("My library"), Tab::Library),
                    (t!("Discover"), Tab::Browse),
                ] {
                    let selected = self.tab == tab
                        && (tab != Tab::Library || self.library_state.filter == Filter::All);
                    if design::nav_row(ui, &label, selected, None).clicked() {
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
                    if design::nav_row(ui, &t!("Open picker"), false, None).clicked() {
                        self.target = None;
                        self.picker_open = true;
                        self.focus_picker(ctx);
                    }
                    for (label, tab) in [
                        (t!("Settings"), Tab::Settings),
                        (t!("Accounts"), Tab::Accounts),
                    ] {
                        if design::nav_row(ui, &label, self.tab == tab, None).clicked() {
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
                    ui.label(t!(
                        "%{provider} · %{count} stickers · %{author}",
                        provider = pack.reference.provider,
                        count = pack.stickers.len(),
                        author = pack.author
                    ));
                    ui.hyperlink_to(t!("View on source website"), &pack.url);
                    ui.label(
                        RichText::new(t!("Right-click a sticker to favorite it or add it to a group without adding the whole pack."))
                            .small()
                            .weak(),
                    );
                    let key = pack.reference.key();
                    let saved = self
                        .packs
                        .iter()
                        .find(|p| p.pack.reference.key() == key)
                        .map(|p| p.favorite);
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .add_enabled(!self.downloading_pack, egui::Button::new(t!("Download pack")))
                            .on_hover_text(t!("Adds every sticker to your library for offline use"))
                            .clicked()
                        {
                            // Save before starting network work. A restart or
                            // partial download must not lose the pack.
                            match self.store.save_pack(&pack) {
                                Ok(()) => {
                                    self.refresh();
                                    self.downloading_pack = true;
                                    self.services.download_pack(pack.clone());
                                }
                                Err(e) => self.message(
                                    t!("Cannot save pack: %{error}", error = format!("{e:#}")),
                                    true,
                                ),
                            }
                        }
                        let mut favorite = saved == Some(true);
                        if ui.checkbox(&mut favorite, t!("Favorite pack")).changed() {
                            // Favoriting covers the whole pack, even if only some
                            // of its stickers were kept before.
                            let result = if favorite {
                                self.store.save_favorite_pack(&pack)
                            } else {
                                self.store.set_pack_favorite(&key, false)
                            };
                            self.report(
                                result,
                                &if favorite {
                                    t!("Pack added to favorites")
                                } else {
                                    t!("Pack removed from favorites")
                                },
                            );
                            self.refresh();
                        }
                    });
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
                    self.selection_bar(ui, &items);
                    ui.separator();
                    self.grid(ui, &items, View::Preview);
                });
            if !open {
                self.preview = None;
                self.selection.clear();
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
                .hint_text(t!("Search stickers…"))
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
            ui.menu_button(t!("Pack options"), |ui| {
                let mut favorite = p.favorite;
                if ui.checkbox(&mut favorite, t!("Favorite pack")).changed() {
                    let r = self.store.set_pack_favorite(&key, favorite);
                    self.report(r, &t!("Favorite updated"));
                    self.refresh();
                }
                if ui
                    .add_enabled(
                        !self.downloading_pack,
                        egui::Button::new(t!("Download for offline use")),
                    )
                    .clicked()
                {
                    self.downloading_pack = true;
                    self.services.download_pack(p.pack.clone());
                    ui.close();
                }
                ui.hyperlink_to(t!("Source"), &p.pack.url);
                if ui.button(t!("Remove from library")).clicked() {
                    let r = self.store.remove_pack(&key);
                    self.report(
                        r,
                        &t!("Pack removed; cached files retained for clipboard safety"),
                    );
                    self.library_state.filter = Filter::All;
                    self.refresh();
                }
            });
        }
        if self.library_state.filter == Filter::Recent {
            ui.label(t!("Recently used first, then newly added stickers."));
        }
        ui.label(
            RichText::new(t!("Click to copy · Right-click for actions"))
                .small()
                .weak(),
        );
        ui.add_space(8.);
        if let Some(items) = self.load_items(ui, View::Library) {
            ui.label(
                RichText::new(t!("%{count} stickers", count = items.len()))
                    .small()
                    .weak(),
            );
            self.grid(ui, &items, View::Library);
        }
    }
    pub(super) fn discover(&mut self, ui: &mut egui::Ui) {
        ui.heading(t!("Discover stickers"));
        ui.label(if self.source == Provider::Giphy {
            t!("Find GIFs and stickers, then keep the ones you like.")
        } else {
            t!("Find a pack, preview it, and make it yours.")
        });
        ui.add_space(12.);
        ui.horizontal(|ui| {
            for source in [Provider::Dcinside, Provider::Arca, Provider::Giphy] {
                if ui
                    .selectable_value(&mut self.source, source, source.to_string())
                    .changed()
                {
                    self.page = 1;
                    self.browse();
                }
            }
        });
        let giphy = self.source == Provider::Giphy;
        if giphy {
            self.giphy_kind(ui);
        }
        let mut submit = false;
        ui.horizontal(|ui| {
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.query)
                    .hint_text(if giphy {
                        t!("Search GIPHY, or leave empty for trending")
                    } else {
                        t!("Search packs or paste a pack URL")
                    })
                    .desired_width(ui.available_width() - 90.),
            );
            submit = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            submit |= ui
                .add_enabled(!self.browsing, egui::Button::new(t!("Search")))
                .clicked();
        });
        if submit {
            if !giphy && self.query.trim().starts_with("https://") {
                match PackRef::parse(&self.query) {
                    Ok(r) => {
                        self.preview_pack(r);
                    }
                    Err(e) => self.message(format!("{e:#}"), true),
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
                    egui::Button::new(t!("Previous")),
                )
                .clicked()
            {
                self.page -= 1;
                self.browse();
            }
            ui.label(t!("Page %{page}", page = self.page));
            if ui
                .add_enabled(
                    (if giphy {
                        self.giphy.more
                    } else {
                        !self.catalog.is_empty()
                    }) && !self.browsing,
                    egui::Button::new(t!("Next")),
                )
                .clicked()
            {
                self.page += 1;
                self.browse();
            }
            if ui
                .add_enabled(!self.browsing, egui::Button::new(t!("Refresh / retry")))
                .clicked()
            {
                self.failed.clear();
                self.browse();
            }
        });
        if giphy {
            self.giphy_discover(ui);
            return;
        }
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
                        ui.hyperlink_to(t!("Source"), &pack.url);
                    });
                });
                ui.separator();
            }
        });
    }
    pub(super) fn accounts(&mut self, ui: &mut egui::Ui) {
        ui.heading(t!("Accounts"));
        ui.add_space(16.);
        ui.strong("DCInside");
        ui.label(t!("Public packs can be browsed without signing in."));
        ui.add_space(24.);
        self.arca_account(ui);
        ui.add_space(24.);
        self.giphy_account(ui);
    }
    fn arca_account(&mut self, ui: &mut egui::Ui) {
        let account = match self.services.account_status() {
            Ok(account) => account.clone(),
            Err(error) => {
                ui.colored_label(ui.visuals().error_fg_color, format!("{error:#}"));
                return;
            }
        };
        ui.horizontal_wrapped(|ui| {
            ui.strong("arca.live");
            let label = if self.login_busy && !account.checking() {
                t!("Waiting for sign-in…")
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
            ui.label(t!("Catalog access verified."));
        }
        if let Some(error) = account.error() {
            ui.colored_label(ui.visuals().error_fg_color, error);
            ui.label(t!(
                "Check your connection, or sign in again to complete website verification."
            ));
        }
        if let Some(error) = &account.storage_error {
            ui.colored_label(ui.visuals().error_fg_color, error);
            ui.label(t!("You can use this session until Picora quits."));
        }
        ui.label(t!("Sign in on arca.live’s own website. When you are done, close the login window to connect the session."));
        ui.label(t!("Picora saves session cookies using OS-protected storage (Windows account encryption on Windows). Your password stays on the website."));
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    !self.login_busy,
                    egui::Button::new(t!("Sign in / reconnect")),
                )
                .clicked()
            {
                self.login_busy = true;
                self.message(
                    t!("Complete sign-in in the arca.live window, then close it"),
                    false,
                );
                self.services.login(true);
            }
            if ui
                .add_enabled(
                    !self.login_busy && account.has_session,
                    egui::Button::new(t!("Check connection")),
                )
                .clicked()
            {
                self.login_busy = true;
                self.services.check_arca();
            }
            if ui
                .add_enabled(
                    !self.login_busy && (account.has_session || account.saved),
                    egui::Button::new(t!("Sign out")),
                )
                .clicked()
            {
                let r = self.services.logout();
                self.report(r, &t!("arca.live session removed"));
            }
        });
        ui.collapsing(t!("Trouble signing in?"), |ui| {
            ui.label(t!("If the website reports a CSRF error or keeps returning to sign-in, try a fresh sign-in window. Your saved session stays in place until a new sign-in finishes."));
            if ui.add_enabled(!self.login_busy, egui::Button::new(t!("Open fresh sign-in window"))).clicked() {
                self.login_busy = true;
                self.message(t!("Sign in again in the new window, then close it to reconnect."), false);
                self.services.login(false);
            }
        });
    }
    pub(super) fn settings_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading(t!("Settings"));
        #[cfg(target_os = "macos")]
        {
            self.macos_permissions_ui(ui, platform::macos_permissions::accessibility_allowed());
            ui.add_space(16.);
        }
        self.updates_ui(ui);
        ui.add_space(16.);
        ui.strong(t!("Global shortcut"));
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.shortcut_text);
            if ui.button(t!("Apply shortcut")).clicked() {
                if self
                    .shortcut_text
                    .parse::<global_hotkey::hotkey::HotKey>()
                    .is_ok()
                {
                    self.register_shortcut(true);
                } else {
                    self.message(t!("Use a shortcut such as Ctrl+Shift+Space"), true);
                }
            }
        });
        ui.label(&self.shortcut_status);
        ui.add_space(16.);
        let mut changed = ui
            .checkbox(
                &mut self.settings.auto_paste,
                t!("Paste automatically after choosing a sticker"),
            )
            .changed();
        ui.label(t!("Picora never sends Enter. If input permission or focus restoration fails, paste manually."));
        if platform::is_wayland() {
            ui.add_space(12.);
            ui.label(t!(
                "Wayland requires your desktop’s permission to send the paste shortcut."
            ));
            #[cfg(target_os = "linux")]
            if ui
                .button(t!("Enable / reconnect Wayland keyboard access"))
                .clicked()
            {
                self.services.enable_wayland(false);
                self.message(t!("Approve keyboard access in the desktop dialog"), false);
            }
        }
        ui.add_space(16.);
        ui.horizontal(|ui| {
            ui.label(t!("Simultaneous downloads"));
            if ui
                .add(egui::Slider::new(
                    &mut self.settings.simultaneous_downloads,
                    crate::services::DOWNLOAD_LIMITS,
                ))
                .changed()
            {
                self.services
                    .set_download_limit(self.settings.simultaneous_downloads);
                changed = true;
            }
        });
        ui.label(t!("Higher values load packs and previews faster. Lower it if a sticker site starts rejecting requests."));
        ui.add_space(16.);
        let old = self.settings.launch_at_login;
        if ui
            .checkbox(&mut self.settings.launch_at_login, t!("Launch at login"))
            .changed()
        {
            match std::env::current_exe()
                .map_err(anyhow::Error::from)
                .and_then(|p| platform::autostart::set(self.settings.launch_at_login, &p))
            {
                Ok(()) => changed = true,
                Err(e) => {
                    self.settings.launch_at_login = old;
                    self.message(format!("{e:#}"), true);
                }
            }
        }
        ui.horizontal(|ui| {
            ui.label(t!("Appearance"));
            for (label, theme) in [
                (t!("System"), egui::ThemePreference::System),
                (t!("Light"), egui::ThemePreference::Light),
                (t!("Dark"), egui::ThemePreference::Dark),
            ] {
                changed |= ui
                    .selectable_value(&mut self.settings.theme, theme, label)
                    .changed();
            }
        });
        ui.horizontal(|ui| {
            ui.label(t!("Language"));
            let old = self.settings.language;
            egui::ComboBox::from_id_salt("language")
                .selected_text(old.name())
                .show_ui(ui, |ui| {
                    for language in crate::i18n::Language::ALL {
                        ui.selectable_value(&mut self.settings.language, language, language.name());
                    }
                });
            if self.settings.language != old {
                self.settings.language.apply();
                changed = true;
            }
        });
        if changed {
            ui.ctx().set_theme(self.settings.theme);
            let r = self.store.save_settings(&self.settings);
            self.report(r, &t!("Settings saved"));
        }
        ui.add_space(16.);
        if ui.button(t!("Open data folder")).clicked() {
            let r = open::that(&self.data_dir).map_err(anyhow::Error::from);
            self.report(r, &t!("Opened data folder"));
        }
        ui.label(t!(
            "Original files are retained so clipboard attachments remain available."
        ));
        ui.add_space(16.);
        if ui.button(t!("Quit Picora")).clicked() {
            self.quit = true;
        }
    }

    #[cfg(target_os = "macos")]
    pub(super) fn macos_permissions_ui(&mut self, ui: &mut egui::Ui, allowed: bool) {
        egui::Frame::group(ui.style()).inner_margin(16.).show(ui, |ui| {
            ui.heading(if allowed { t!("Automatic paste is ready") } else { t!("Enable automatic paste") });
            if allowed {
                ui.label(t!("Permission is enabled. Switch to your chat, open Picora, and choose the sticker again."));
                ui.label(t!("The previous sticker will not be pasted automatically."));
                if ui.button(t!("Back to my library")).clicked() {
                    self.tab = Tab::Library;
                }
                return;
            }
            ui.label(t!("Your sticker is copied. You can paste it now with Cmd+V, or finish setup below."));
            ui.add_space(8.);
            ui.label(t!("1. Open Accessibility settings using the button below."));
            ui.label(t!("2. Turn on Picora in the app list. If it is missing, click + and select Picora.app in Applications."));
            ui.label(t!("3. Return to Picora. This panel checks permission automatically."));
            ui.add_space(8.);
            if ui.button(t!("Open Accessibility settings")).clicked() {
                self.cancel_auto_paste();
                platform::macos_permissions::request_accessibility();
                if let Err(error) = platform::macos_permissions::open_settings() {
                    self.message(t!("Could not open System Settings: %{error}. Open Privacy & Security → Accessibility manually.", error = error), true);
                }
            }
            ui.collapsing(t!("Picora is already enabled, but it still does not work"), |ui| {
                ui.label(t!("macOS may still be allowing an older Picora build. This can happen once after updating from an older version. Remove Picora from the Accessibility list with −, add the current Picora.app with +, and turn it on. Then quit and reopen Picora."));
                ui.label(t!("Keep Picora in Applications. If you run it from source, macOS may list your terminal or launcher instead."));
            });
            if ui.button(t!("Use manual paste instead")).clicked() {
                self.cancel_auto_paste();
                self.settings.auto_paste = false;
                let result = self.store.save_settings(&self.settings);
                self.report(result, &t!("Automatic paste is off. Copy a sticker, then press Cmd+V in your chat."));
            }
        });
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs(1));
    }
}
