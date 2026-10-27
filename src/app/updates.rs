use super::{App, egui};
use crate::update::{
    controller::State,
    release::{BUILD_VERSION, Channel, REPOSITORY},
};
use rust_i18n::t;
impl App {
    pub(super) fn update_notice(&mut self, ui: &mut egui::Ui) {
        let State::Available(update) = &self.updates.state else {
            return;
        };
        if ui
            .button(t!(
                "Update available · %{version}",
                version = update.version
            ))
            .clicked()
        {
            self.hide_picker(ui.ctx());
            self.open_library();
            self.tab = super::Tab::Settings;
        }
    }

    pub(super) fn updates_ui(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading(t!("Updates"));
        ui.label(format!("Picora {}", BUILD_VERSION));
        let previous_channel = self.settings.update_channel;
        ui.add_enabled_ui(!self.updates.busy(), |ui| {
            ui.horizontal(|ui| {
                ui.label(t!("Update channel"));
                egui::ComboBox::from_id_salt("update-channel")
                    .selected_text(match self.settings.update_channel {
                        Channel::Stable => t!("Stable"),
                        Channel::Dev => t!("Dev"),
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.settings.update_channel,
                            Channel::Stable,
                            t!("Stable"),
                        );
                        ui.selectable_value(
                            &mut self.settings.update_channel,
                            Channel::Dev,
                            t!("Dev"),
                        );
                    });
            });
        });
        if self.settings.update_channel == Channel::Dev {
            ui.label(t!(
                "Latest main build that passed CI. May contain unfinished changes."
            ));
        }
        if previous_channel != self.settings.update_channel {
            self.updates.cancel();
            let result = self.store.save_settings(&self.settings);
            self.report(result, &t!("Update channel saved"));
            self.updates.check(
                &self.services.runtime,
                ui.ctx(),
                self.settings.update_channel,
            );
        }
        if let Some(error) = &self.updates.previous_failure {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        if ui
            .checkbox(
                &mut self.settings.check_updates_automatically,
                t!("Check for updates at startup"),
            )
            .changed()
        {
            let result = self.store.save_settings(&self.settings);
            self.report(result, &t!("Update preference saved"));
        }
        if ui
            .add_enabled(
                !self.updates.busy(),
                egui::Button::new(t!("Check for updates")),
            )
            .clicked()
        {
            self.updates.check(
                &self.services.runtime,
                ui.ctx(),
                self.settings.update_channel,
            );
        }
        let mut install = false;
        let mut cancel = false;
        match &self.updates.state {
            State::Idle => {}
            State::Checking => {
                ui.spinner();
                ui.label(t!("Checking GitHub Releases…"));
            }
            State::Current => {
                ui.label(t!("No update is available on this channel."));
            }
            State::Available(update) => {
                ui.label(t!(
                    "Picora %{version} is available",
                    version = update.version
                ));
                egui::ScrollArea::vertical()
                    .id_salt("update-notes")
                    .max_height(140.)
                    .show(ui, |ui| {
                        ui.label(&update.notes);
                    });
                install = ui.button(t!("Update and restart")).clicked();
            }
            State::Downloading(done, total) => {
                ui.add(
                    egui::ProgressBar::new(*done as f32 / (*total).max(1) as f32).show_percentage(),
                );
                ui.label(t!("Downloading and verifying update…"));
                cancel = ui.button(t!("Cancel update")).clicked();
            }
            State::Staged(..) | State::HandingOff => {
                ui.spinner();
                ui.label(t!("Preparing restart…"));
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
        ui.hyperlink_to(t!("GitHub releases"), format!("{REPOSITORY}/releases"));
    }
}
