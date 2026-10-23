use super::egui;
use crate::providers::giphy::{Giphy, Item, Kind, PAGE_SIZE, Page};
use anyhow::Result;
use std::sync::{Arc, mpsc};
use tokio::{runtime::Handle, sync::Semaphore, task::JoinHandle};

enum Event {
    Key(Result<Option<String>>),
    Stored(Result<()>, &'static str),
    Page(u64, Result<Page>),
    Preview(u64, usize, Result<egui::ColorImage>),
    Copy(u64, Result<egui::ColorImage>),
}
struct Tile {
    item: Item,
    texture: Option<egui::TextureHandle>,
    error: Option<String>,
}
pub(super) struct Panel {
    key: String,
    key_input: String,
    key_loading: bool,
    initialized: bool,
    query: String,
    kind: Kind,
    submitted: (Kind, String),
    offset: u32,
    more: bool,
    generation: u64,
    loading: bool,
    copying: bool,
    copy_generation: u64,
    copy_task: Option<JoinHandle<()>>,
    status: String,
    details: Option<String>,
    error: bool,
    tiles: Vec<Tile>,
    tx: mpsc::Sender<Event>,
    rx: mpsc::Receiver<Event>,
    search: Option<JoinHandle<()>>,
    previews: Vec<JoinHandle<()>>,
    slots: Arc<Semaphore>,
    logos: Option<[egui::TextureHandle; 2]>,
}
impl Default for Panel {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            key: String::new(),
            key_input: String::new(),
            key_loading: false,
            initialized: false,
            query: String::new(),
            kind: Kind::Stickers,
            submitted: (Kind::Stickers, String::new()),
            offset: 0,
            more: false,
            generation: 0,
            loading: false,
            copying: false,
            copy_generation: 0,
            copy_task: None,
            status: String::new(),
            details: None,
            error: false,
            tiles: Vec::new(),
            tx,
            rx,
            search: None,
            previews: Vec::new(),
            slots: Arc::new(Semaphore::new(4)),
            logos: None,
        }
    }
}
fn credentials() -> Result<keyring::Entry> {
    Ok(keyring::Entry::new("app.picora.desktop", "giphy-api-key")?)
}
fn color_image(bytes: &[u8], thumbnail: bool) -> Result<egui::ColorImage> {
    let decoded = if thumbnail {
        crate::media::preview(bytes)?
    } else {
        crate::media::decode(bytes)?
    };
    let frame = decoded
        .frames
        .first()
        .ok_or_else(|| anyhow::anyhow!("GIPHY image has no frames"))?;
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [frame.width as usize, frame.height as usize],
        &frame.rgba,
    ))
}
impl Panel {
    fn message(&mut self, result: Result<()>, success: &str) {
        self.error = result.is_err();
        self.details = result
            .as_ref()
            .err()
            .filter(|e| e.chain().count() > 1)
            .map(|e| format!("{e:#}"));
        self.status = result
            .err()
            .map_or_else(|| success.into(), |e| e.to_string());
    }
    fn cancel_copy(&mut self) {
        self.copy_generation += 1;
        if let Some(task) = self.copy_task.take() {
            task.abort();
        }
        self.copying = false;
    }
    fn copy_link(&mut self, ctx: &egui::Context, url: Result<String>) {
        self.cancel_copy();
        match url {
            Ok(url) => {
                ctx.copy_text(url);
                self.message(Ok(()), "GIF link copied. Paste it in your chat.");
            }
            Err(error) => self.message(Err(error), ""),
        }
    }
    fn clear_results(&mut self) {
        self.cancel_copy();
        self.generation += 1;
        if let Some(task) = self.search.take() {
            task.abort();
        }
        for task in self.previews.drain(..) {
            task.abort();
        }
        self.tiles.clear();
        self.loading = false;
        self.copying = false;
        self.more = false;
    }
    fn start(
        &mut self,
        runtime: &Handle,
        ctx: &egui::Context,
        offset: u32,
        query: String,
        kind: Kind,
    ) {
        self.clear_results();
        self.offset = offset;
        self.submitted = (kind, query.clone());
        self.loading = true;
        self.status.clear();
        self.details = None;
        self.error = false;
        let (tx, ctx, key, id) = (
            self.tx.clone(),
            ctx.clone(),
            self.key.clone(),
            self.generation,
        );
        self.search = Some(runtime.spawn(async move {
            let result = async { Giphy::new()?.search(&key, kind, &query, offset).await }.await;
            let _ = tx.send(Event::Page(id, result));
            ctx.request_repaint();
        }));
    }
    pub(super) fn poll(&mut self, runtime: &Handle, ctx: &egui::Context) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::Key(result) => {
                    self.key_loading = false;
                    match result {
                        Ok(key) => {
                            self.key = key.unwrap_or_default();
                            self.key_input.clone_from(&self.key);
                        }
                        Err(error) => self.message(
                            Err(error.context(
                                "Saved key could not be loaded. Enter your key to continue.",
                            )),
                            "",
                        ),
                    }
                }
                Event::Stored(result, success) => {
                    self.key_loading = false;
                    self.message(result, success);
                }
                Event::Page(id, result) if id == self.generation => {
                    self.loading = false;
                    match result {
                        Err(error) => self.message(Err(error), ""),
                        Ok(page) => {
                            self.more =
                                page.pagination.offset.saturating_add(page.pagination.count)
                                    < page.pagination.total_count;
                            self.message(
                                Ok(()),
                                if page.data.is_empty() {
                                    "No results. Try another search."
                                } else {
                                    "Copy a still image or a GIF link, then paste it in your chat."
                                },
                            );
                            self.tiles = page
                                .data
                                .into_iter()
                                .map(|item| Tile {
                                    item,
                                    texture: None,
                                    error: None,
                                })
                                .collect();
                            for (index, tile) in self.tiles.iter().enumerate() {
                                let url = tile
                                    .item
                                    .image("fixed_width_still")
                                    .or_else(|_| tile.item.image("original_still"))
                                    .map(str::to_owned);
                                let (tx, ctx, slots) =
                                    (self.tx.clone(), ctx.clone(), self.slots.clone());
                                self.previews.push(runtime.spawn(async move {
                                    let result = async {
                                        let _permit = slots.acquire_owned().await?;
                                        let bytes = Giphy::new()?.image(&url?).await?;
                                        tokio::task::spawn_blocking(move || {
                                            color_image(&bytes, true)
                                        })
                                        .await?
                                    }
                                    .await;
                                    let _ = tx.send(Event::Preview(id, index, result));
                                    ctx.request_repaint();
                                }));
                            }
                        }
                    }
                }
                Event::Preview(id, index, result) if id == self.generation => {
                    if let Some(tile) = self.tiles.get_mut(index) {
                        match result {
                            Ok(image) => {
                                tile.texture = Some(ctx.load_texture(
                                    format!("giphy-{id}-{index}"),
                                    image,
                                    egui::TextureOptions::LINEAR,
                                ))
                            }
                            Err(error) => tile.error = Some(format!("{error:#}")),
                        }
                    }
                }
                Event::Copy(id, result) if id == self.copy_generation => {
                    self.copying = false;
                    match result {
                        Ok(image) => {
                            ctx.copy_image(image);
                            self.message(Ok(()), "Still image copied. Paste it in your chat.");
                        }
                        Err(error) => self.message(Err(error), ""),
                    }
                }
                _ => (),
            }
        }
    }
    pub(super) fn show(&mut self, ui: &mut egui::Ui, runtime: &Handle) {
        if !self.initialized {
            self.initialized = true;
            self.key_loading = true;
            let (tx, ctx) = (self.tx.clone(), ui.ctx().clone());
            runtime.spawn_blocking(move || {
                let result = (|| match credentials()?.get_password() {
                    Ok(key) => Ok(Some(key)),
                    Err(keyring::Error::NoEntry) => Ok(None),
                    Err(e) => Err(e.into()),
                })();
                let _ = tx.send(Event::Key(result));
                ctx.request_repaint();
            });
        }
        self.poll(runtime, ui.ctx());
        ui.heading("GIPHY");
        if self.logos.is_none() {
            let load = |name, bytes: &[u8]| {
                color_image(bytes, false).map(|image| {
                    ui.ctx()
                        .load_texture(name, image, egui::TextureOptions::LINEAR)
                })
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
                self.logos = Some([light, dark]);
            }
        }
        if let Some(logos) = &self.logos {
            let logo = &logos[usize::from(ui.visuals().dark_mode)];
            ui.image((logo.id(), logo.size_vec2()));
        }
        ui.label("Online GIFs and stickers · use your own GIPHY API key");
        egui::CollapsingHeader::new("API key")
            .default_open(self.key.is_empty())
            .show(ui, |ui| {
                ui.add_enabled_ui(!self.key_loading, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.key_input)
                            .password(true)
                            .hint_text("GIPHY API key"),
                    );
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .add_enabled(
                                !self.key_input.trim().is_empty(),
                                egui::Button::new("Use and save key"),
                            )
                            .clicked()
                        {
                            self.clear_results();
                            self.key = self.key_input.trim().into();
                            self.key_loading = true;
                            let (key, tx, ctx) =
                                (self.key.clone(), self.tx.clone(), ui.ctx().clone());
                            runtime.spawn_blocking(move || {
                                let result = (|| {
                                    credentials()?.set_password(&key)?;
                                    Ok(())
                                })();
                                let result = result.map_err(|e: anyhow::Error| e.context("Your key is available for this session, but could not be saved."));
                                let _ = tx.send(Event::Stored(
                                    result,
                                    "Key saved in OS-protected storage.",
                                ));
                                ctx.request_repaint();
                            });
                        }
                        if ui.button("Forget key").clicked() {
                            self.clear_results();
                            self.key.clear();
                            self.key_input.clear();
                            self.key_loading = true;
                            let (tx, ctx) = (self.tx.clone(), ui.ctx().clone());
                            runtime.spawn_blocking(move || {
                                let result = (|| match credentials()?.delete_credential() {
                                    Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                                    Err(e) => Err(e.into()),
                                })();
                                let _ = tx.send(Event::Stored(result, "GIPHY key removed."));
                                ctx.request_repaint();
                            });
                        }
                        ui.hyperlink_to(
                            "Get an API key",
                            "https://developers.giphy.com/dashboard/",
                        );
                    });
                });
                ui.label(
                    "Your key is sent directly to GIPHY. Search limits depend on your key's plan.",
                );
            });
        let mut search = false;
        ui.add_enabled_ui(!self.key.is_empty() && !self.key_loading, |ui| {
            ui.horizontal_wrapped(|ui| {
                search |= ui
                    .selectable_value(&mut self.kind, Kind::Stickers, "Stickers")
                    .changed();
                search |= ui
                    .selectable_value(&mut self.kind, Kind::Gifs, "GIFs")
                    .changed();
                let input = ui.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .hint_text("Search GIPHY…")
                        .desired_width(220.),
                );
                search |= ui.button("Search").clicked()
                    || (input.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                if ui.button("Trending").clicked() {
                    self.query.clear();
                    search = true;
                }
            });
        });
        if search {
            self.start(runtime, ui.ctx(), 0, self.query.clone(), self.kind);
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    self.offset > 0 && !self.loading,
                    egui::Button::new("Previous"),
                )
                .clicked()
            {
                self.start(
                    runtime,
                    ui.ctx(),
                    self.offset.saturating_sub(PAGE_SIZE),
                    self.submitted.1.clone(),
                    self.submitted.0,
                );
            }
            if ui
                .add_enabled(self.more && !self.loading, egui::Button::new("Next"))
                .clicked()
            {
                self.start(
                    runtime,
                    ui.ctx(),
                    self.offset.saturating_add(PAGE_SIZE),
                    self.submitted.1.clone(),
                    self.submitted.0,
                );
            }
            if self.loading || self.key_loading || self.copying {
                ui.spinner();
            }
        });
        if self.error {
            ui.colored_label(ui.visuals().error_fg_color, &self.status);
        } else {
            ui.label(&self.status);
        }
        if let Some(details) = &self.details {
            ui.collapsing("Technical details", |ui| {
                ui.label(details);
            });
        }
        ui.label(
            egui::RichText::new("GIPHY results stay online and are not added to offline packs.")
                .small()
                .weak(),
        );
        let mut copy = None;
        let mut link = None;
        egui::ScrollArea::vertical()
            .id_salt("giphy-results")
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for tile in &self.tiles {
                        ui.allocate_ui_with_layout(
                            egui::vec2(164., 245.),
                            egui::Layout::top_down(egui::Align::LEFT),
                            |ui| {
                                ui.allocate_ui(egui::vec2(160., 120.), |ui| {
                                    if let Some(texture) = &tile.texture {
                                        ui.add(
                                            egui::Image::new((texture.id(), texture.size_vec2()))
                                                .fit_to_exact_size(egui::vec2(160., 120.)),
                                        );
                                    } else if let Some(error) = &tile.error {
                                        ui.label("Preview unavailable").on_hover_text(error);
                                    } else {
                                        ui.spinner();
                                    }
                                });
                                ui.add(egui::Label::new(&tile.item.title).truncate())
                                    .on_hover_text(&tile.item.title);
                                if !tile.item.username.is_empty() {
                                    ui.add(
                                        egui::Label::new(format!("By {}", tile.item.username))
                                            .truncate(),
                                    );
                                }
                                if ui
                                    .add_enabled(
                                        !self.copying,
                                        egui::Button::new("Copy still image"),
                                    )
                                    .clicked()
                                {
                                    copy =
                                        Some(tile.item.image("original_still").map(str::to_owned));
                                }
                                if ui.button("Copy GIF link").clicked() {
                                    link = Some(tile.item.image("original").map(str::to_owned));
                                }
                                ui.hyperlink_to("View on GIPHY", &tile.item.url);
                            },
                        );
                    }
                });
            });
        if let Some(url) = link {
            self.copy_link(ui.ctx(), url);
        } else if let Some(url) = copy {
            self.cancel_copy();
            self.copying = true;
            let (tx, ctx, id) = (self.tx.clone(), ui.ctx().clone(), self.copy_generation);
            self.copy_task = Some(runtime.spawn(async move {
                let result = async {
                    let bytes = Giphy::new()?.image(&url?).await?;
                    tokio::task::spawn_blocking(move || color_image(&bytes, false)).await?
                }
                .await;
                let _ = tx.send(Event::Copy(id, result));
                ctx.request_repaint();
            }));
        }
    }
}
impl Drop for Panel {
    fn drop(&mut self) {
        self.clear_results();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn changing_search_discards_old_clipboard_work_but_current_copy_reaches_platform() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let ctx = egui::Context::default();
        let mut panel = Panel::default();
        panel.copying = true;
        let old = panel.copy_generation;
        panel.clear_results();
        let pixel = || egui::ColorImage::filled([1, 1], egui::Color32::RED);
        panel.tx.send(Event::Copy(old, Ok(pixel()))).unwrap();
        let output = ctx.run(Default::default(), |ctx| panel.poll(runtime.handle(), ctx));
        assert!(
            !output
                .platform_output
                .commands
                .iter()
                .any(|command| matches!(command, egui::OutputCommand::CopyImage(_)))
        );
        panel
            .tx
            .send(Event::Copy(panel.copy_generation, Ok(pixel())))
            .unwrap();
        let output = ctx.run(Default::default(), |ctx| panel.poll(runtime.handle(), ctx));
        assert!(
            output
                .platform_output
                .commands
                .iter()
                .any(|command| matches!(command, egui::OutputCommand::CopyImage(_)))
        );
        assert!(!panel.copying);
        let pending = panel.copy_generation;
        panel.copying = true;
        let output = ctx.run(Default::default(), |ctx| {
            panel.copy_link(ctx, Ok("https://media.giphy.com/cat.gif".into()));
            panel.tx.send(Event::Copy(pending, Ok(pixel()))).unwrap();
            panel.poll(runtime.handle(), ctx);
        });
        assert!(output.platform_output.commands.iter().any(|command|
            matches!(command, egui::OutputCommand::CopyText(text) if text.ends_with("cat.gif"))));
        assert!(
            !output
                .platform_output
                .commands
                .iter()
                .any(|command| matches!(command, egui::OutputCommand::CopyImage(_)))
        );
        assert!(!panel.copying);
    }
}
