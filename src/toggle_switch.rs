use eframe::egui::{self, Widget};

pub struct ToggleSwitch {
    checked: bool,
}

impl ToggleSwitch {
    pub fn new(checked: bool) -> Self {
        Self { checked }
    }
}

impl Widget for ToggleSwitch {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let desired_size = ui.spacing().interact_size.y * egui::vec2(2.0, 1.0);

        let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click());

        // Attach some meta-data to the response which can be used by screen readers:
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::Checkbox,
                ui.is_enabled(),
                self.checked,
                "",
            )
        });

        if ui.is_rect_visible(rect) {
            // Let's ask for a simple animation from egui.
            // egui keeps track of changes in the boolean associated with the id and
            // returns an animated value in the 0-1 range for how much "on" we are.
            let how_on = ui.ctx().animate_bool_responsive(response.id, self.checked);
            // We will follow the current style by asking
            // "how should something that is being interacted with be painted?".
            // This will, for instance, give us different colors when the widget is hovered or clicked.
            let visuals = ui.style().interact_selectable(&response, self.checked);
            // All coordinates are in absolute screen coordinates so we use `rect` to place the elements.
            let rect = rect.expand(visuals.expansion);
            let radius = 0.5 * rect.height();
            ui.painter().rect(
                rect,
                radius,
                visuals.bg_fill,
                visuals.bg_stroke,
                egui::StrokeKind::Inside,
            );
            // Paint the circle, animating it from left to right with `how_on`:
            let circle_x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), how_on);
            let center = egui::pos2(circle_x, rect.center().y);
            ui.painter()
                .circle(center, 0.75 * radius, visuals.bg_fill, visuals.fg_stroke);
        }
        response
    }
}
