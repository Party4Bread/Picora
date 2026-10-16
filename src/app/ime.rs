//! Compatibility for commit-only Linux IMEs with egui 0.33.
//! See https://github.com/emilk/egui/issues/7906. Revisit when upgrading egui.
use super::*;

pub(super) fn install(ctx: &egui::Context) {
    // eframe's raw_input_hook is skipped for immediate child viewports (the
    // library). egui's begin-pass callback runs for every viewport instead.
    ctx.on_begin_pass(
        "picora-linux-ime",
        std::sync::Arc::new(|ctx| {
            let mut events = ctx.input_mut(|input| std::mem::take(&mut input.events));
            prepare(ctx, &mut events);
            ctx.input_mut(|input| input.events = events);
        }),
    );
}

fn prepare(ctx: &egui::Context, input_events: &mut Vec<egui::Event>) {
    let id = egui::Id::new("picora-ime-composition").with(ctx.viewport_id());
    let mut composing = ctx.data(|data| data.get_temp::<bool>(id).unwrap_or(false));
    let mut events = Vec::with_capacity(input_events.len());
    for event in input_events.drain(..) {
        match &event {
            egui::Event::Ime(egui::ImeEvent::Enabled) => composing = true,
            egui::Event::Ime(egui::ImeEvent::Disabled) => composing = false,
            egui::Event::Ime(egui::ImeEvent::Commit(_)) => {
                // egui-winit ignores Linux Ime::Enabled and normally starts composition
                // on Preedit. fcitx5 can commit without Preedit. egui 0.33 otherwise
                // compares the current cursor against a stale composition cursor and
                // silently rejects text after the first syllable. Initialize it only
                // for a standalone commit; preserve an existing preedit's selection.
                if !composing {
                    events.push(egui::Event::Ime(egui::ImeEvent::Enabled));
                }
                composing = false;
            }
            egui::Event::WindowFocused(false) => composing = false,
            _ => {}
        }
        events.push(event);
    }
    *input_events = events;
    ctx.data_mut(|data| data.insert_temp(id, composing));
}
