use super::*;
impl App {
    pub(super) fn thumb(
        &mut self,
        ui: &mut egui::Ui,
        provider: Provider,
        url: &str,
        size: f32,
        selected: bool,
    ) -> egui::Response {
        let response = ui.add(
            egui::Button::new("")
                .min_size(Vec2::splat(size))
                .selected(selected),
        );
        if let Some(v) = self.visuals.get(url) {
            let frame = if response.hovered() && v.frames.len() > 1 {
                ui.ctx().request_repaint_after(Duration::from_millis(35));
                let total: u64 = v.delays.iter().sum();
                let mut t = (ui.input(|i| i.time) * 1000.) as u64 % total.max(1);
                let mut n = 0;
                for (i, d) in v.delays.iter().enumerate() {
                    if t < *d {
                        n = i;
                        break;
                    }
                    t -= *d;
                }
                n
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
                "Retry",
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
                self.services.media(provider, url.into());
            }
        }
        response
    }
    pub(super) fn grid(&mut self, ui: &mut egui::Ui, items: Vec<LibrarySticker>, picker: bool) {
        if items.is_empty() {
            ui.add_space(32.);
            if !self.search.trim().is_empty() {
                ui.heading("No matching stickers");
                ui.label("Try another search or clear it to see this collection.");
                if ui.button("Clear search").clicked() {
                    self.search.clear();
                }
            } else if self.packs.is_empty() {
                ui.heading("Your library is empty");
                ui.label("Browse a pack and add it to your library.");
                if ui.button("Browse repositories").clicked() {
                    self.tab = Tab::Browse;
                    self.open_library();
                    self.browse();
                }
            } else {
                ui.heading("No stickers in this collection");
                ui.label("Choose another pack or manage your stickers in the library.");
                if ui.button("Show all stickers").clicked() {
                    self.filter = Filter::All;
                }
            }
            return;
        }
        let columns = ((ui.available_width() + 8.) / 88.).floor().max(1.) as usize;
        if picker {
            self.selected = self.selected.min(items.len() - 1);
        }
        let popup_open = egui::Popup::is_any_open(ui.ctx());
        if picker && self.pending.is_none() && !popup_open {
            self.selected = navigate(ui, self.selected, items.len(), columns);
            if ui.input(|i| i.key_pressed(egui::Key::Enter))
                && !ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Ime(_))))
            {
                self.choose(items[self.selected].clone(), CopyMode::Auto, true);
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
        let mut scroll = egui::ScrollArea::vertical().id_salt(if picker {
            "picker-grid"
        } else {
            "library-grid"
        });
        if navigation {
            scroll = scroll.vertical_scroll_offset(
                (self.selected / columns) as f32 * (80. + ui.spacing().item_spacing.y),
            );
        }
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
                                    picker && n == self.selected,
                                );
                                r.clone().on_hover_text(format!(
                                    "{}\n{} · {}\nRight-click for more",
                                    item.sticker.label, item.pack_title, item.provider
                                ));
                                if r.clicked() {
                                    if picker {
                                        self.selected = n;
                                    }
                                    self.choose(item.clone(), CopyMode::Auto, picker);
                                }
                                r.context_menu(|ui| self.sticker_menu(ui, item, picker));
                            });
                        });
                    }
                });
            }
        });
    }
    fn sticker_menu(&mut self, ui: &mut egui::Ui, item: &LibrarySticker, picker: bool) {
        if ui.button("Copy still image").clicked() {
            self.choose(item.clone(), CopyMode::Still, false);
            ui.close();
        }
        if ui.button("Paste original file").clicked() {
            self.choose(item.clone(), CopyMode::Original, picker);
            ui.close();
        }
        ui.separator();
        let favorite = match self
            .store
            .sticker_favorite(&item.pack_key, &item.sticker.id)
        {
            Ok(Some(value)) => value,
            Ok(None) => {
                ui.label("Add this pack to your library to manage its stickers.");
                return;
            }
            Err(e) => {
                ui.label(format!("Cannot read sticker: {e}"));
                return;
            }
        };
        if ui
            .button(if favorite {
                "Unfavorite sticker"
            } else {
                "Favorite sticker"
            })
            .clicked()
        {
            let result = self
                .store
                .favorite_sticker(&item.pack_key, &item.sticker.id, !favorite);
            self.report(result, "Sticker favorite updated");
            ui.close();
        }
        if ui
            .button("Remove sticker from library")
            .on_hover_text(
                "Removes it from groups and Recent too. Reimport the source pack to restore it.",
            )
            .clicked()
        {
            let result = self.store.remove_sticker(&item.pack_key, &item.sticker.id);
            self.report(
                result,
                "Sticker removed. Reimport its source pack to restore it.",
            );
            self.refresh();
            ui.close();
        }
        ui.separator();
        ui.label("Add to group");
        if self.groups.is_empty() {
            ui.label("Create a custom group in Library → Groups.");
        }

        for group in self.groups.clone() {
            if ui.button(&group.name).clicked() {
                let result = self
                    .store
                    .add_to_group(group.id, &item.pack_key, &item.sticker.id);
                self.report(result, "Added to group");
                ui.close();
            }
        }
        if let Filter::Group(id) = self.filter {
            if ui.button("Move earlier").clicked() {
                let r = self
                    .store
                    .move_in_group(id, &item.pack_key, &item.sticker.id, -1);
                self.report(r, "Order updated");
                ui.close();
            }
            if ui.button("Move later").clicked() {
                let r = self
                    .store
                    .move_in_group(id, &item.pack_key, &item.sticker.id, 1);
                self.report(r, "Order updated");
                ui.close();
            }
            if ui.button("Remove from group").clicked() {
                let r = self
                    .store
                    .remove_from_group(id, &item.pack_key, &item.sticker.id);
                self.report(r, "Removed from group");
                ui.close();
            }
        }
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
