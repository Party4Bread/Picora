use super::{
    App, BrowseState, Color32, CopyMode, Duration, Filter, LibrarySticker, Provider, Tab, Vec2,
    View, egui,
};
use crate::model::Pack;
use anyhow::Context;
use rust_i18n::t;

pub(super) struct Thumbnail {
    pub response: egui::Response,
    pub activated: bool,
}
impl App {
    pub(super) fn thumb(
        &mut self,
        ui: &mut egui::Ui,
        provider: Provider,
        url: &str,
        size: f32,
        selected: bool,
    ) -> Thumbnail {
        let response = ui.add(
            egui::Button::new("")
                .min_size(Vec2::splat(size))
                .frame(false),
        );
        // Discover lays out all rows. Clipped rows must not start downloads or
        // compete with visible thumbnails for the bounded texture cache.
        if !ui.is_rect_visible(response.rect) {
            return Thumbnail {
                response,
                activated: false,
            };
        }
        let fill = if selected {
            ui.visuals().selection.bg_fill
        } else if response.hovered() {
            ui.visuals().widgets.hovered.weak_bg_fill
        } else {
            Color32::TRANSPARENT
        };
        let stroke = if selected || response.has_focus() {
            ui.visuals().selection.stroke
        } else {
            egui::Stroke::NONE
        };
        ui.painter()
            .rect(response.rect, 8., fill, stroke, egui::StrokeKind::Inside);
        let retrying = self.failed.contains_key(url) && !self.visuals.contains_key(url);
        if let Some(v) = self.visuals.get_mut(url) {
            v.last_used = ui.ctx().cumulative_frame_nr();
            if response.hovered()
                && !v.animation_loaded
                && !self.failed.contains_key(url)
                && self.inflight.insert(url.into())
            {
                // Restore animation on demand while retaining the still on screen.
                self.services
                    .request_media_preview(provider, url.into(), true);
            }
            let frame = if response.hovered() && v.frames.len() > 1 {
                let (frame, wait) =
                    animation_frame(&v.delays, (ui.input(|i| i.time) * 1000.) as u64);
                ui.ctx().request_repaint_after(wait);
                frame
            } else {
                0
            };
            let texture = &v.frames[frame];
            let rect = response.rect.shrink(7.);
            let source = texture.size_vec2();
            let scale = (rect.width() / source.x).min(rect.height() / source.y);
            let rect = egui::Rect::from_center_size(rect.center(), source * scale);
            ui.painter().image(
                texture.id(),
                rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                Color32::WHITE,
            );
        } else if let Some(error) = self.failed.get(url) {
            ui.painter().text(
                response.rect.center(),
                egui::Align2::CENTER_CENTER,
                t!("Retry"),
                egui::FontId::proportional(12.),
                ui.visuals().error_fg_color,
            );
            response.clone().on_hover_text(error);
            if response.clicked() {
                self.failed.remove(url);
            }
        } else {
            ui.painter().circle_stroke(
                response.rect.center(),
                7.,
                egui::Stroke::new(1.5_f32, ui.visuals().weak_text_color()),
            );
            if !url.is_empty() && self.inflight.insert(url.into()) {
                self.services
                    .request_media_preview(provider, url.into(), false);
            }
        }
        Thumbnail {
            activated: response.clicked() && !retrying,
            response,
        }
    }
    fn browse_state(&self, view: View) -> Option<&BrowseState> {
        match view {
            View::Library => Some(&self.library_state),
            View::Picker => Some(&self.picker_state),
            _ => None,
        }
    }
    fn browse_state_mut(&mut self, view: View) -> Option<&mut BrowseState> {
        match view {
            View::Library => Some(&mut self.library_state),
            View::Picker => Some(&mut self.picker_state),
            _ => None,
        }
    }
    fn empty_grid(&mut self, ui: &mut egui::Ui, view: View) {
        ui.add_space(32.);
        if view == View::Discover {
            ui.heading(t!("No GIPHY results"));
            ui.label(t!(
                "Try another search, or clear it to see what is trending."
            ));
        } else if self
            .browse_state(view)
            .is_some_and(|s| !s.search.trim().is_empty())
        {
            ui.heading(t!("No matching stickers"));
            ui.label(t!("Try another search or clear it to see this collection."));
            if ui.button(t!("Clear search")).clicked()
                && let Some(state) = self.browse_state_mut(view)
            {
                state.search.clear();
            }
        } else if self.packs.is_empty() {
            ui.heading(t!("Your library is empty"));
            ui.label(t!("Browse a pack and add it to your library."));
            if ui.button(t!("Browse repositories")).clicked() {
                self.tab = Tab::Browse;
                self.open_library();
                self.browse();
            }
        } else if self
            .browse_state(view)
            .is_some_and(|s| matches!(s.filter, Filter::Group(_)))
        {
            ui.heading(t!("This group is empty"));
            ui.label(t!(
                "Right-click stickers in your library or Discover and choose this group."
            ));
        } else {
            ui.heading(t!("No stickers in this collection"));
            ui.label(t!(
                "Choose another pack or manage your stickers in the library."
            ));
            if ui.button(t!("Show all stickers")).clicked() {
                if let Some(state) = self.browse_state_mut(view) {
                    state.filter = Filter::All;
                } else {
                    self.tab = Tab::Library;
                    self.library_state.filter = Filter::All;
                }
            }
        }
    }
    pub(super) fn grid(&mut self, ui: &mut egui::Ui, items: &[LibrarySticker], view: View) {
        let picker = view == View::Picker;
        let selectable = matches!(view, View::Preview | View::Discover);
        if items.is_empty() {
            self.empty_grid(ui, view);
            return;
        }
        let columns = ((ui.available_width() + 8.) / 88.).floor().max(1.) as usize;
        if picker {
            self.picker_state.selected = self.picker_state.selected.min(items.len() - 1);
        }
        let popup_open = egui::Popup::is_any_open(ui.ctx());
        if picker && self.pending.is_none() && !popup_open {
            self.picker_state.selected =
                navigate(ui, self.picker_state.selected, items.len(), columns);
            if ui.input(|i| i.key_pressed(egui::Key::Enter))
                && !ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Ime(_))))
            {
                self.begin_sticker_copy(
                    items[self.picker_state.selected].clone(),
                    CopyMode::Auto,
                    true,
                );
            }
        }
        let editing_text = ui.ctx().wants_keyboard_input();
        let navigation = picker
            && !popup_open
            && !editing_text
            && ui.input(|i| {
                [
                    egui::Key::ArrowDown,
                    egui::Key::ArrowUp,
                    egui::Key::ArrowLeft,
                    egui::Key::ArrowRight,
                ]
                .iter()
                .any(|k| i.key_pressed(*k))
            });
        let mut scroll = egui::ScrollArea::vertical().id_salt(match view {
            View::Picker => "picker-grid",
            View::Discover => "discover-grid",
            _ => "library-grid",
        });
        if navigation {
            scroll = scroll.vertical_scroll_offset(
                (self.picker_state.selected / columns) as f32 * (80. + ui.spacing().item_spacing.y),
            );
        }
        // One query per frame instead of one per visible tile.
        let favorites = self.store.favorite_keys().unwrap_or_default();
        scroll.show_rows(ui, 80., items.len().div_ceil(columns), |ui, rows| {
            for row in rows {
                ui.horizontal(|ui| {
                    for column in 0..columns {
                        let n = row * columns + column;
                        let Some(item) = items.get(n) else { break };
                        ui.push_id(format!("{}:{}", item.pack_key, item.sticker.id), |ui| {
                            ui.vertical(|ui| {
                                ui.set_width(80.);
                                let r = self.thumb(
                                    ui,
                                    item.provider,
                                    &item.sticker.url,
                                    80.,
                                    (picker && n == self.picker_state.selected)
                                        || (selectable && self.selection.contains(item)),
                                );
                                r.response.widget_info(|| {
                                    egui::WidgetInfo::labeled(
                                        egui::WidgetType::Button,
                                        true,
                                        &item.sticker.label,
                                    )
                                });
                                r.response.clone().on_hover_text(format!(
                                    "{}\n{} · {}\n{}",
                                    item.sticker.label,
                                    item.pack_title,
                                    item.provider,
                                    t!("Right-click for more")
                                ));
                                let favorite = favorites
                                    .contains(&(item.pack_key.clone(), item.sticker.id.clone()));
                                self.favorite_star(ui, &r.response, item, favorite);
                                let modifiers = ui.input(|i| i.modifiers);
                                if r.activated
                                    && selectable
                                    && (self.selection.active
                                        || modifiers.command
                                        || modifiers.shift)
                                {
                                    self.selection.click(items, n, modifiers.shift);
                                } else if r.activated {
                                    if picker {
                                        self.picker_state.selected = n;
                                    }
                                    self.begin_sticker_copy(item.clone(), CopyMode::Auto, picker);
                                }
                                r.response.context_menu(|ui| {
                                    let chosen = self.selection.chosen(items);
                                    if selectable
                                        && chosen.len() > 1
                                        && self.selection.contains(item)
                                    {
                                        self.selection_menu(ui, &chosen);
                                    } else {
                                        self.sticker_menu(ui, item, view);
                                    }
                                });
                            });
                        });
                    }
                });
            }
        });
    }
    fn sticker_menu(&mut self, ui: &mut egui::Ui, item: &LibrarySticker, view: View) {
        if ui.button(t!("Copy still image")).clicked() {
            self.begin_sticker_copy(item.clone(), CopyMode::Still, false);
            ui.close();
        }
        if ui.button(t!("Paste original file")).clicked() {
            self.begin_sticker_copy(item.clone(), CopyMode::Original, view == View::Picker);
            ui.close();
        }
        if item.provider == Provider::Giphy && ui.button(t!("Copy GIF link")).clicked() {
            ui.ctx().copy_text(item.sticker.url.clone());
            self.message(t!("GIF link copied"), false);
            ui.close();
        }
        ui.separator();
        let saved = match self
            .store
            .is_sticker_favorite(&item.pack_key, &item.sticker.id)
        {
            Ok(saved) => saved,
            Err(e) => {
                ui.label(t!("Cannot read sticker: %{error}", error = e));
                return;
            }
        };
        // An unsaved sticker (pack preview or GIPHY result) is kept on its own
        // the first time it is favorited or added to a group.
        let keep = saved.is_none();
        let favorite = saved.unwrap_or(false);
        if ui
            .button(if favorite {
                t!("Unfavorite sticker")
            } else {
                t!("Favorite sticker")
            })
            .clicked()
        {
            let result = self.set_favorites(std::slice::from_ref(item), !favorite);
            self.report(result, &t!("Sticker favorite updated"));
            ui.close();
        }
        if !keep
            && ui
                .button(t!("Remove sticker from library"))
                .on_hover_text(t!(
                    "Removes it from groups and Recent too. Reimport the source pack to restore it."
                ))
                .clicked()
        {
            let result = self.store.remove_sticker(&item.pack_key, &item.sticker.id);
            self.report(
                result,
                &t!("Sticker removed. Reimport its source pack to restore it."),
            );
            self.refresh();
            ui.close();
        }
        ui.separator();
        ui.label(t!("Add to group"));
        if self.groups.is_empty() {
            ui.label(t!(
                "Create a group with + next to Custom groups in the library sidebar."
            ));
        }

        for group in self.groups.clone() {
            if ui.button(&group.name).clicked() {
                let result = self
                    .keep_unsaved(std::slice::from_ref(item))
                    .and_then(|()| {
                        self.store
                            .add_to_group(group.id, &item.pack_key, &item.sticker.id)
                    });
                self.report(result, &t!("Added to group"));
                // The group's cover is its first sticker.
                self.refresh();
                ui.close();
            }
        }
        let group = self
            .browse_state(view)
            .and_then(|state| match state.filter {
                Filter::Group(id) => Some(id),
                _ => None,
            });
        if let Some(id) = group {
            if ui.button(t!("Move earlier")).clicked() {
                let r = self
                    .store
                    .move_in_group(id, &item.pack_key, &item.sticker.id, -1);
                self.report(r, &t!("Order updated"));
                self.refresh();
                ui.close();
            }
            if ui.button(t!("Move later")).clicked() {
                let r = self
                    .store
                    .move_in_group(id, &item.pack_key, &item.sticker.id, 1);
                self.report(r, &t!("Order updated"));
                self.refresh();
                ui.close();
            }
            if ui.button(t!("Remove from group")).clicked() {
                let r = self
                    .store
                    .remove_from_group(id, &item.pack_key, &item.sticker.id);
                self.report(r, &t!("Removed from group"));
                self.refresh();
                ui.close();
            }
        }
    }
}

impl App {
    fn source_pack(&self, item: &LibrarySticker) -> Option<Pack> {
        if item.provider == Provider::Giphy {
            return Some(crate::providers::giphy::collection());
        }
        self.preview
            .as_ref()
            .filter(|p| p.reference.key() == item.pack_key)
            .cloned()
    }
    /// Unsaved stickers are kept on their own before being favorited.
    pub(super) fn set_favorites(
        &mut self,
        items: &[LibrarySticker],
        favorite: bool,
    ) -> anyhow::Result<()> {
        self.keep_unsaved(items)?;
        items.iter().try_for_each(|i| {
            self.store
                .set_sticker_favorite(&i.pack_key, &i.sticker.id, favorite)
        })
    }
    /// A star in the tile's corner: filled for favorites, outlined on hover.
    fn favorite_star(
        &mut self,
        ui: &mut egui::Ui,
        tile: &egui::Response,
        item: &LibrarySticker,
        favorite: bool,
    ) {
        let rect = egui::Rect::from_center_size(
            tile.rect.right_top() + egui::vec2(-12., 12.),
            Vec2::splat(20.),
        );
        // Registered after the tile, so it takes the click in its corner.
        let response = ui.interact(rect, tile.id.with("favorite"), egui::Sense::click());
        let (action, label) = if favorite {
            (
                t!("Remove from favorites"),
                t!(
                    "Remove from favorites: %{label}",
                    label = item.sticker.label
                ),
            )
        } else {
            (
                t!("Add to favorites"),
                t!("Add to favorites: %{label}", label = item.sticker.label),
            )
        };
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &label));
        if !(favorite || tile.hovered() || response.hovered()) {
            return;
        }
        let painter = ui.painter();
        painter.circle_filled(
            rect.center(),
            10.,
            Color32::from_black_alpha(if response.hovered() { 140 } else { 90 }),
        );
        super::design::star(painter, rect.center(), 7., favorite);
        if response.on_hover_text(action).clicked() {
            let result = self.set_favorites(std::slice::from_ref(item), !favorite);
            self.report(
                result,
                &if favorite {
                    t!("Removed from favorites")
                } else {
                    t!("Added to favorites")
                },
            );
        }
    }
    /// Favorites and groups reference saved stickers. Unsaved stickers are
    /// saved into their source pack without its other stickers, then
    /// downloaded for offline use; cached media is reused.
    pub(super) fn keep_unsaved(&mut self, items: &[LibrarySticker]) -> anyhow::Result<()> {
        // Each entry holds only the stickers being kept from that pack.
        let mut sources: Vec<Pack> = Vec::new();
        for item in items {
            if self
                .store
                .is_sticker_favorite(&item.pack_key, &item.sticker.id)?
                .is_some()
            {
                continue;
            }
            if let Some(pack) = sources
                .iter_mut()
                .find(|p| p.reference.key() == item.pack_key)
            {
                pack.stickers.push(item.sticker.clone());
                continue;
            }
            let pack = self
                .source_pack(item)
                .with_context(|| t!("Open this sticker's pack again to manage it"))?;
            sources.push(Pack {
                stickers: vec![item.sticker.clone()],
                ..pack
            });
        }
        for pack in &sources {
            let collection = Pack {
                stickers: Vec::new(),
                ..pack.clone()
            };
            self.store.add_stickers(collection, &pack.stickers)?;
        }
        if !sources.is_empty() {
            self.refresh();
        }
        for pack in sources {
            let title = match pack.stickers.as_slice() {
                [one] => one.label.clone(),
                many => t!("%{count} stickers", count = many.len()).into_owned(),
            };
            self.downloading_pack = true;
            self.services.download_pack(Pack { title, ..pack });
        }
        Ok(())
    }
}

fn navigate(ui: &egui::Ui, mut selected: usize, length: usize, columns: usize) -> usize {
    let editing_text = ui.ctx().wants_keyboard_input();
    ui.input(|i| {
        if !editing_text && i.key_pressed(egui::Key::ArrowRight) {
            selected = (selected + 1).min(length - 1);
        }
        if !editing_text && i.key_pressed(egui::Key::ArrowLeft) {
            selected = selected.saturating_sub(1);
        }
        if i.key_pressed(egui::Key::ArrowDown) {
            selected = (selected + columns).min(length - 1);
        }
        if i.key_pressed(egui::Key::ArrowUp) {
            selected = selected.saturating_sub(columns);
        }
    });

    selected
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keyboard_navigation_reads_context_without_nested_locks() {
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::ArrowRight,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        let mut selected = 0;
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                selected = navigate(ui, selected, 10, 5);
            });
        });
        assert_eq!(selected, 1);
    }
}

fn animation_frame(delays: &[u64], elapsed: u64) -> (usize, Duration) {
    let total = delays.iter().sum::<u64>().max(1);
    let mut position = elapsed % total;
    for (index, delay) in delays.iter().enumerate() {
        if position < *delay {
            return (index, Duration::from_millis(delay - position));
        }
        position -= delay;
    }
    (0, Duration::from_millis(100))
}

#[cfg(test)]
mod animation_timing_tests {
    use super::*;
    #[test]
    fn repaint_waits_for_next_frame_boundary_and_wraps() {
        let delays = [100, 200];
        assert_eq!(animation_frame(&delays, 0), (0, Duration::from_millis(100)));
        assert_eq!(animation_frame(&delays, 99), (0, Duration::from_millis(1)));
        assert_eq!(
            animation_frame(&delays, 100),
            (1, Duration::from_millis(200))
        );
        assert_eq!(
            animation_frame(&delays, 350),
            (0, Duration::from_millis(50))
        );
    }
}
