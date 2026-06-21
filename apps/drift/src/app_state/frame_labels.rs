use super::{App, Action};

/// A named label attached to a specific frame on a layer.
#[derive(Clone, Debug)]
pub struct FrameLabel {
    pub id: usize,
    pub layer_id: usize,
    pub frame: usize,
    pub label: String,
    pub comment: String,
    /// If true this is a blank keyframe — does not show the previous frame's content.
    pub is_blank_keyframe: bool,
}

impl App {
    pub fn apply_frame_labels(&mut self, action: Action) {
        match action {
            Action::AddFrameLabel { layer_id, frame, label } => {
                // If a label already exists on this layer/frame, update it instead.
                if let Some(existing) = self
                    .frame_labels
                    .iter_mut()
                    .find(|fl| fl.layer_id == layer_id && fl.frame == frame)
                {
                    existing.label = label;
                    return;
                }
                let id = self.next_label_id;
                self.next_label_id += 1;
                self.frame_labels.push(FrameLabel {
                    id,
                    layer_id,
                    frame,
                    label,
                    comment: String::new(),
                    is_blank_keyframe: false,
                });
            }
            Action::SetFrameBlank { layer_id, frame, blank } => {
                if let Some(fl) = self
                    .frame_labels
                    .iter_mut()
                    .find(|fl| fl.layer_id == layer_id && fl.frame == frame)
                {
                    fl.is_blank_keyframe = blank;
                } else {
                    // Auto-create a label entry if none exists for this frame.
                    let id = self.next_label_id;
                    self.next_label_id += 1;
                    self.frame_labels.push(FrameLabel {
                        id,
                        layer_id,
                        frame,
                        label: String::new(),
                        comment: String::new(),
                        is_blank_keyframe: blank,
                    });
                }
            }
            Action::SetFrameComment { label_id, comment } => {
                if let Some(fl) = self.frame_labels.iter_mut().find(|fl| fl.id == label_id) {
                    fl.comment = comment;
                }
            }
            Action::RemoveFrameLabel { label_id } => {
                self.frame_labels.retain(|fl| fl.id != label_id);
            }
            Action::GoToLabel { label } => {
                if let Some(fl) = self.frame_labels.iter().find(|fl| fl.label == label) {
                    self.current_frame = fl.frame;
                }
            }
            _ => {}
        }
    }

    /// Returns the frame label for a specific layer/frame, if any.
    pub fn frame_label_at(&self, layer_id: usize, frame: usize) -> Option<&FrameLabel> {
        self.frame_labels
            .iter()
            .find(|fl| fl.layer_id == layer_id && fl.frame == frame)
    }

    /// Returns all labels for a given layer, sorted by frame.
    pub fn labels_for_layer(&self, layer_id: usize) -> Vec<&FrameLabel> {
        let mut labels: Vec<&FrameLabel> = self
            .frame_labels
            .iter()
            .filter(|fl| fl.layer_id == layer_id)
            .collect();
        labels.sort_by_key(|fl| fl.frame);
        labels
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Action};
    use super::super::LayerKind;

    fn app() -> App {
        App::new()
    }

    fn layer(a: &mut App) -> usize {
        a.apply(Action::AddLayer { name: "L".to_string(), kind: LayerKind::Vector });
        a.layers.last().unwrap().id
    }

    #[test]
    fn test_add_frame_label() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::AddFrameLabel { layer_id: lid, frame: 10, label: "start".to_string() });
        assert_eq!(a.frame_labels.len(), 1);
        assert_eq!(a.frame_labels[0].label, "start");
        assert_eq!(a.frame_labels[0].frame, 10);
    }

    #[test]
    fn test_add_frame_label_updates_existing() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::AddFrameLabel { layer_id: lid, frame: 5, label: "old".to_string() });
        a.apply(Action::AddFrameLabel { layer_id: lid, frame: 5, label: "new".to_string() });
        assert_eq!(a.frame_labels.len(), 1);
        assert_eq!(a.frame_labels[0].label, "new");
    }

    #[test]
    fn test_add_frame_label_id_increments() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::AddFrameLabel { layer_id: lid, frame: 0, label: "a".to_string() });
        a.apply(Action::AddFrameLabel { layer_id: lid, frame: 1, label: "b".to_string() });
        assert_ne!(a.frame_labels[0].id, a.frame_labels[1].id);
    }

    #[test]
    fn test_set_frame_blank() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::AddFrameLabel { layer_id: lid, frame: 10, label: "key".to_string() });
        a.apply(Action::SetFrameBlank { layer_id: lid, frame: 10, blank: true });
        assert!(a.frame_labels[0].is_blank_keyframe);
    }

    #[test]
    fn test_set_frame_blank_creates_entry() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::SetFrameBlank { layer_id: lid, frame: 20, blank: true });
        assert_eq!(a.frame_labels.len(), 1);
        assert!(a.frame_labels[0].is_blank_keyframe);
        assert_eq!(a.frame_labels[0].frame, 20);
    }

    #[test]
    fn test_set_frame_comment() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::AddFrameLabel { layer_id: lid, frame: 5, label: "loop".to_string() });
        let fl_id = a.frame_labels[0].id;
        a.apply(Action::SetFrameComment { label_id: fl_id, comment: "animation loop point".to_string() });
        assert_eq!(a.frame_labels[0].comment, "animation loop point");
    }

    #[test]
    fn test_remove_frame_label() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::AddFrameLabel { layer_id: lid, frame: 5, label: "x".to_string() });
        let fl_id = a.frame_labels[0].id;
        a.apply(Action::RemoveFrameLabel { label_id: fl_id });
        assert!(a.frame_labels.is_empty());
    }

    #[test]
    fn test_go_to_label_seeks_frame() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::AddFrameLabel { layer_id: lid, frame: 48, label: "intro".to_string() });
        a.apply(Action::GoToLabel { label: "intro".to_string() });
        assert_eq!(a.current_frame, 48);
    }

    #[test]
    fn test_go_to_label_nonexistent_noop() {
        let mut a = app();
        a.apply(Action::GoToLabel { label: "missing".to_string() });
        assert_eq!(a.current_frame, 0);
    }

    #[test]
    fn test_frame_label_at_helper() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::AddFrameLabel { layer_id: lid, frame: 10, label: "hit".to_string() });
        let fl = a.frame_label_at(lid, 10);
        assert!(fl.is_some());
        assert_eq!(fl.unwrap().label, "hit");
    }

    #[test]
    fn test_labels_for_layer_sorted() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::AddFrameLabel { layer_id: lid, frame: 30, label: "c".to_string() });
        a.apply(Action::AddFrameLabel { layer_id: lid, frame: 10, label: "a".to_string() });
        a.apply(Action::AddFrameLabel { layer_id: lid, frame: 20, label: "b".to_string() });
        let labels = a.labels_for_layer(lid);
        assert_eq!(labels.len(), 3);
        assert_eq!(labels[0].frame, 10);
        assert_eq!(labels[1].frame, 20);
        assert_eq!(labels[2].frame, 30);
    }

    #[test]
    fn test_default_label_not_blank() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::AddFrameLabel { layer_id: lid, frame: 0, label: "start".to_string() });
        assert!(!a.frame_labels[0].is_blank_keyframe);
    }
}
