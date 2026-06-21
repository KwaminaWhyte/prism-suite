use super::*;

impl App {
    pub(super) fn apply_text(&mut self, action: Action) {
        match action {
            Action::SetTextSize(s) => self.text_size = s.clamp(6.0, 400.0),
            _ => {}
        }
    }
}
