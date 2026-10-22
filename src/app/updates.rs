use super::{App, egui};
use crate::update::{controller::State, release::REPOSITORY};
impl App {
    pub(super) fn update_notice(&mut self, ui: &mut egui::Ui) {
        let State::Available(update) = &self.updates.state else {
            return;
        };
        if ui
            .button(format!("Update available · {}", update.version))
            .clicked()
        {
            self.hide_picker(ui.ctx());
            self.open_library();
            self.tab = super::Tab::Settings;
        }
    }

    pub(super) fn updates_ui(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading("Updates");
        ui.label(format!("Picora {}", env!("CARGO_PKG_VERSION")));
        if let Some(error) = &self.updates.previous_failure {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        if ui
            .checkbox(
                &mut self.settings.check_updates_automatically,
                "Check for updates at startup",
            )
            .changed()
        {
            let result = self.store.save_settings(&self.settings);
            self.report(result, "Update preference saved");
        }
        if ui
            .add_enabled(!self.updates.busy(), egui::Button::new("Check for updates"))
            .clicked()
        {
            self.updates.check(&self.services.runtime, ui.ctx());
        }
        let mut install = false;
        let mut cancel = false;
        match &self.updates.state {
            State::Idle => {}
            State::Checking => {
                ui.spinner();
                ui.label("Checking GitHub Releases…");
            }
            State::Current => {
                ui.label("No newer release is available.");
            }
            State::Available(update) => {
                ui.label(format!("Picora {} is available", update.version));
                egui::ScrollArea::vertical()
                    .id_salt("update-notes")
                    .max_height(140.)
                    .show(ui, |ui| {
                        ui.label(&update.notes);
                    });
                install = ui.button("Update and restart").clicked();
            }
            State::Downloading(done, total) => {
                ui.add(
                    egui::ProgressBar::new(*done as f32 / (*total).max(1) as f32).show_percentage(),
                );
                ui.label("Downloading and verifying update…");
                cancel = ui.button("Cancel update").clicked();
            }
            State::Staged(..) | State::HandingOff => {
                ui.spinner();
                ui.label("Preparing restart…");
            }
            State::Error(error) => {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
        }
        if install {
            self.updates.download(&self.services.runtime, ui.ctx());
        }
        if cancel {
            self.updates.cancel();
        }
        ui.hyperlink_to("GitHub releases", format!("{REPOSITORY}/releases"));
    }
}
