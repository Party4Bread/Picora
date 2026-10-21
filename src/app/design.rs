use super::{Color32, egui};

pub(super) fn configure(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8., 8.);
        style.spacing.button_padding = egui::vec2(12., 6.);
        style.spacing.interact_size.y = 30.;
        style.spacing.scroll = egui::style::ScrollStyle::solid();
        style
            .text_styles
            .insert(egui::TextStyle::Heading, egui::FontId::proportional(22.));
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(14.));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(14.));
        style
            .text_styles
            .insert(egui::TextStyle::Small, egui::FontId::proportional(12.));
        let v = &mut style.visuals;
        let dark = v.dark_mode;
        v.panel_fill = if dark {
            Color32::from_rgb(24, 27, 31)
        } else {
            Color32::from_rgb(250, 251, 252)
        };
        v.window_fill = if dark {
            Color32::from_rgb(31, 35, 40)
        } else {
            Color32::WHITE
        };
        v.extreme_bg_color = if dark {
            Color32::from_rgb(34, 39, 45)
        } else {
            Color32::from_rgb(238, 241, 244)
        };
        v.faint_bg_color = if dark {
            Color32::from_rgb(30, 34, 39)
        } else {
            Color32::from_rgb(242, 244, 246)
        };
        v.override_text_color = Some(if dark {
            Color32::from_rgb(230, 234, 238)
        } else {
            Color32::from_rgb(36, 43, 50)
        });
        v.weak_text_color = Some(if dark {
            Color32::from_rgb(157, 168, 180)
        } else {
            Color32::from_rgb(93, 107, 120)
        });
        v.selection.bg_fill = if dark {
            Color32::from_rgb(32, 68, 66)
        } else {
            Color32::from_rgb(216, 239, 232)
        };
        v.selection.stroke = egui::Stroke::new(
            1.0_f32,
            if dark {
                Color32::from_rgb(110, 215, 183)
            } else {
                Color32::from_rgb(16, 108, 82)
            },
        );
        v.hyperlink_color = v.selection.stroke.color;
        v.widgets.noninteractive.bg_stroke = egui::Stroke::new(
            1.0_f32,
            if dark {
                Color32::from_rgb(49, 56, 64)
            } else {
                Color32::from_rgb(220, 226, 231)
            },
        );
        v.widgets.inactive.weak_bg_fill = v.faint_bg_color;
        v.widgets.inactive.bg_fill = v.extreme_bg_color;
        v.widgets.inactive.bg_stroke = v.widgets.noninteractive.bg_stroke;
        v.widgets.hovered.weak_bg_fill = if dark {
            Color32::from_rgb(45, 53, 61)
        } else {
            Color32::from_rgb(228, 234, 239)
        };
        v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, v.weak_text_color.unwrap());
        v.widgets.active.weak_bg_fill = v.selection.bg_fill;
        for widget in [
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.noninteractive,
            &mut v.widgets.open,
        ] {
            widget.corner_radius = egui::CornerRadius::same(6);
            widget.expansion = 0.;
        }
        v.window_corner_radius = egui::CornerRadius::same(10);
    });
}

pub(super) fn nav_row(
    ui: &mut egui::Ui,
    label: &str,
    selected: bool,
    count: Option<usize>,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 32.), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            label,
        )
    });
    let fill = if selected {
        ui.visuals().selection.bg_fill
    } else if response.hovered() {
        ui.visuals().widgets.hovered.weak_bg_fill
    } else {
        Color32::TRANSPARENT
    };
    let stroke = if response.has_focus() {
        ui.visuals().selection.stroke
    } else {
        egui::Stroke::NONE
    };
    ui.painter()
        .rect(rect, 6., fill, stroke, egui::StrokeKind::Inside);
    let color = if selected {
        ui.visuals().selection.stroke.color
    } else {
        ui.visuals().text_color()
    };
    let count_width = if count.is_some() { 36. } else { 0. };
    let mut job = egui::text::LayoutJob::simple_singleline(
        label.into(),
        egui::FontId::proportional(14.),
        color,
    );
    job.wrap.max_width = (rect.width() - 24. - count_width).max(1.);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(
        egui::pos2(rect.left() + 12., rect.center().y - galley.size().y / 2.),
        galley,
        color,
    );
    if let Some(n) = count {
        ui.painter().text(
            egui::pos2(rect.right() - 10., rect.center().y),
            egui::Align2::RIGHT_CENTER,
            n.to_string(),
            egui::FontId::proportional(12.),
            ui.visuals().weak_text_color(),
        );
    }
    response.on_hover_text(label)
}
