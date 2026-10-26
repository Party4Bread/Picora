use super::{App, LibrarySticker, Provider, Result, RichText, Tab, View, egui};
use crate::providers::giphy::{self, Kind};

pub(super) struct State {
    pub(super) key: String,
    pub(super) key_input: String,
    pub(super) key_loading: bool,
    pub(super) kind: Kind,
    pub(super) more: bool,
    pub(super) results: Vec<LibrarySticker>,
    logos: Option<[egui::TextureHandle; 2]>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            key: String::new(),
            key_input: String::new(),
            key_loading: true,
            kind: Kind::Stickers,
            more: false,
            results: Vec::new(),
            logos: None,
        }
    }
}
impl App {
    pub(super) fn giphy_key_loaded(&mut self, result: Result<Option<String>>) {
        self.giphy.key_loading = false;
        match result {
            Ok(key) => {
                self.giphy.key = key.unwrap_or_default();
                self.giphy.key_input.clone_from(&self.giphy.key);
                if self.source == Provider::Giphy && self.tab == Tab::Browse {
                    self.browse();
                }
            }
            Err(error) => self.message(
                format!(
                    "Saved GIPHY key could not be loaded. Enter it again in Accounts. {error:#}"
                ),
                true,
            ),
        }
    }
    pub(super) fn giphy_results(&mut self, result: Result<giphy::Page>) {
        self.browsing = false;
        match result {
            Ok(page) => {
                let collection = giphy::collection();
                self.giphy.more = page.more();
                self.giphy.results = page
                    .stickers()
                    .into_iter()
                    .map(|sticker| LibrarySticker {
                        pack_key: collection.reference.key(),
                        pack_title: collection.title.clone(),
                        provider: Provider::Giphy,
                        sticker,
                    })
                    .collect();
                self.message(
                    if self.giphy.results.is_empty() {
                        "No GIPHY results"
                    } else {
                        "Click to copy · Right-click to favorite or add to a group"
                    },
                    false,
                );
            }
            Err(error) => self.message(format!("{error:#}"), true),
        }
    }
    fn giphy_attribution(&mut self, ui: &mut egui::Ui) {
        if self.giphy.logos.is_none() {
            let load = |name, bytes: &[u8]| -> Result<egui::TextureHandle> {
                let decoded = crate::media::decode(bytes)?;
                let frame = &decoded.frames[0];
                let image = egui::ColorImage::from_rgba_unmultiplied(
                    [frame.width as usize, frame.height as usize],
                    &frame.rgba,
                );
                Ok(ui
                    .ctx()
                    .load_texture(name, image, egui::TextureOptions::LINEAR))
            };
            if let (Ok(light), Ok(dark)) = (
                load(
                    "giphy-attribution-light",
                    include_bytes!("../../assets/giphy-light.png"),
                ),
                load(
                    "giphy-attribution-dark",
                    include_bytes!("../../assets/giphy-dark.png"),
                ),
            ) {
                self.giphy.logos = Some([light, dark]);
            }
        }
        if let Some(logos) = &self.giphy.logos {
            let logo = &logos[usize::from(ui.visuals().dark_mode)];
            ui.image((logo.id(), logo.size_vec2()));
        }
    }
    pub(super) fn giphy_kind(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let mut changed = false;
            for (kind, label) in [(Kind::Stickers, "Stickers"), (Kind::Gifs, "GIFs")] {
                changed |= ui
                    .selectable_value(&mut self.giphy.kind, kind, label)
                    .changed();
            }
            if changed {
                self.page = 1;
                self.browse();
            }
        });
    }
    pub(super) fn giphy_discover(&mut self, ui: &mut egui::Ui) {
        self.giphy_attribution(ui);
        if self.giphy.key_loading {
            ui.spinner();
            return;
        }
        if self.giphy.key.is_empty() {
            ui.add_space(12.);
            ui.heading("Add your GIPHY API key");
            ui.label("GIPHY search uses your own API key. Add it in Accounts to search GIFs and stickers.");
            if ui.button("Open Accounts").clicked() {
                self.tab = Tab::Accounts;
            }
            return;
        }
        ui.label(
            RichText::new("Click to copy · Right-click to favorite or add to a group")
                .small()
                .weak(),
        );
        if !self.browsing {
            let items = self.giphy.results.clone();
            self.selection_bar(ui, &items);
            self.grid(ui, &items, View::Discover);
        }
    }
    pub(super) fn giphy_account(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.strong("GIPHY");
            ui.label(if self.giphy.key_loading {
                "Loading key…"
            } else if self.giphy.key.is_empty() {
                "No API key"
            } else {
                "API key ready"
            });
        });
        self.giphy_attribution(ui);
        ui.label("Search GIFs and stickers in Discover with your own GIPHY API key. Keep results by favoriting them or adding them to a group.");
        ui.add_enabled_ui(!self.giphy.key_loading, |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.giphy.key_input)
                    .password(true)
                    .hint_text("GIPHY API key"),
            );
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        !self.giphy.key_input.trim().is_empty(),
                        egui::Button::new("Use and save key"),
                    )
                    .clicked()
                {
                    self.giphy.key = self.giphy.key_input.trim().into();
                    self.giphy.key_loading = true;
                    self.giphy.results.clear();
                    self.services.save_giphy_key(self.giphy.key.clone());
                }
                if ui
                    .add_enabled(!self.giphy.key.is_empty(), egui::Button::new("Forget key"))
                    .clicked()
                {
                    self.giphy.key.clear();
                    self.giphy.key_input.clear();
                    self.giphy.key_loading = true;
                    self.giphy.results.clear();
                    self.services.forget_giphy_key();
                }
                ui.hyperlink_to("Get an API key", "https://developers.giphy.com/dashboard/");
            });
        });
        ui.label("Your key is sent only to GIPHY and saved in OS-protected storage. Search limits depend on your key's plan.");
    }
}
