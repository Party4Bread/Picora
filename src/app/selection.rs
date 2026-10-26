//! Discover sticker lists can select several stickers and keep them together.
use super::{App, LibrarySticker, RichText, egui};
use std::collections::HashSet;

fn key(item: &LibrarySticker) -> (String, String) {
    (item.pack_key.clone(), item.sticker.id.clone())
}
#[derive(Default)]
pub(super) struct Selection {
    pub(super) active: bool,
    keys: HashSet<(String, String)>,
    anchor: Option<usize>,
}
impl Selection {
    pub(super) fn contains(&self, item: &LibrarySticker) -> bool {
        self.keys.contains(&key(item))
    }
    pub(super) fn clear(&mut self) {
        self.keys.clear();
        self.anchor = None;
    }
    /// Toggles the sticker at `index`; `extend` selects the range from the last click.
    pub(super) fn click(&mut self, items: &[LibrarySticker], index: usize, extend: bool) {
        match self.anchor.filter(|a| extend && *a < items.len()) {
            Some(anchor) => {
                for item in &items[anchor.min(index)..=anchor.max(index)] {
                    self.keys.insert(key(item));
                }
            }
            None => {
                let k = key(&items[index]);
                if !self.keys.remove(&k) {
                    self.keys.insert(k);
                }
            }
        }
        self.anchor = Some(index);
    }
    pub(super) fn chosen(&self, items: &[LibrarySticker]) -> Vec<LibrarySticker> {
        items
            .iter()
            .filter(|item| self.contains(item))
            .cloned()
            .collect()
    }
}
impl App {
    pub(super) fn selection_bar(&mut self, ui: &mut egui::Ui, items: &[LibrarySticker]) {
        let count = self.selection.chosen(items).len();
        ui.horizontal_wrapped(|ui| {
            if ui
                .selectable_label(self.selection.active, "Select")
                .on_hover_text("Click stickers to select them. Ctrl/Cmd-click also selects, and Shift-click selects a range.")
                .clicked()
            {
                self.selection.active = !self.selection.active;
                if !self.selection.active {
                    self.selection.clear();
                }
            }
            if self.selection.active || count > 0 {
                ui.label(format!("{count} selected"));
                if ui.button("Select all").clicked() {
                    self.selection.keys.extend(items.iter().map(key));
                }
                if count > 0 && ui.button("Clear").clicked() {
                    self.selection.clear();
                }
            }
        });
        if count > 0 {
            ui.label(
                RichText::new("Right-click a selected sticker to add them all to a group.")
                    .small()
                    .weak(),
            );
        }
    }
    pub(super) fn selection_menu(&mut self, ui: &mut egui::Ui, items: &[LibrarySticker]) {
        let count = items.len();
        ui.label(RichText::new(format!("{count} stickers selected")).strong());
        let favorites = self.store.favorite_keys().unwrap_or_default();
        let all_favorite = items
            .iter()
            .all(|i| favorites.contains(&(i.pack_key.clone(), i.sticker.id.clone())));
        let (label, done) = if all_favorite {
            (
                "Unfavorite selected",
                format!("{count} stickers removed from favorites"),
            )
        } else {
            ("Favorite selected", format!("{count} stickers favorited"))
        };
        if ui.button(label).clicked() {
            let result = self.set_favorites(items, !all_favorite);
            self.finish_selection(result, &done);
            ui.close();
        }
        ui.separator();
        ui.label("Add selected to group");
        if self.groups.is_empty() {
            ui.label("Create a group with + next to Custom groups in the library sidebar.");
        }
        for group in self.groups.clone() {
            if ui.button(&group.name).clicked() {
                let result = self.keep_unsaved(items).and_then(|()| {
                    items.iter().try_for_each(|i| {
                        self.store
                            .add_to_group(group.id, &i.pack_key, &i.sticker.id)
                    })
                });
                self.finish_selection(result, &format!("Added {count} stickers to {}", group.name));
                ui.close();
            }
        }
    }
    fn finish_selection(&mut self, result: anyhow::Result<()>, success: &str) {
        if result.is_ok() {
            self.selection.clear();
        }
        self.report(result, success);
        // Group covers and kept stickers change with the batch.
        self.refresh();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Provider, Sticker};
    #[test]
    fn click_toggles_and_shift_click_selects_a_range() {
        let items: Vec<_> = (0..6)
            .map(|i| LibrarySticker {
                pack_key: "dcinside:1".into(),
                pack_title: "Pack".into(),
                provider: Provider::Dcinside,
                sticker: Sticker {
                    id: i.to_string(),
                    label: i.to_string(),
                    url: String::new(),
                },
            })
            .collect();
        let ids = |s: &Selection| {
            s.chosen(&items)
                .iter()
                .map(|i| i.sticker.id.clone())
                .collect::<Vec<_>>()
        };
        let mut selection = Selection::default();
        selection.click(&items, 4, true);
        assert_eq!(
            ids(&selection),
            ["4"],
            "Shift without an anchor selects one"
        );
        selection.click(&items, 1, true);
        assert_eq!(ids(&selection), ["1", "2", "3", "4"]);
        selection.click(&items, 2, false);
        assert_eq!(ids(&selection), ["1", "3", "4"]);
        selection.clear();
        assert!(ids(&selection).is_empty());
        selection.click(&items[..2], 1, false);
        selection.click(&items, 5, true);
        assert_eq!(ids(&selection), ["1", "2", "3", "4", "5"]);
    }
}
