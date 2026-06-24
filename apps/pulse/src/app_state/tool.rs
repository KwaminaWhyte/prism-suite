//! The editing-[`Tool`] enum extracted from `app_state/mod.rs` (workspace size
//! rule). Plain mode enum + labels/ordering — re-exported from `app_state` so
//! existing `crate::app_state::Tool` paths keep resolving.

/// The editing tools. A starter subset mirroring the egui app's transport/edit
/// modes; the GPUI host wires behavior per wave. (Pulse's egui app is modal —
/// select/scrub — rather than a brush palette like Pigment.)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    /// Select / move layers in the preview.
    Select,
    /// Scrub the playhead (transport).
    Hand,
    /// Pen / mask authoring (placeholder this pass).
    Pen,
    /// Reposition a layer's anchor point (pivot for transforms).
    AnchorPoint,
    /// Puppet warp: place/move deformation pins on the selected layer.
    Puppet,
}

impl Tool {
    /// Short label for the toolbar / tools strip.
    pub fn label(self) -> &'static str {
        match self {
            Tool::Select => "Select",
            Tool::Hand => "Hand",
            Tool::Pen => "Pen",
            Tool::AnchorPoint => "Anchor",
            Tool::Puppet => "Puppet",
        }
    }

    /// Stable ordering for the tools strip.
    pub const ALL: [Tool; 5] = [Tool::Select, Tool::Hand, Tool::Pen, Tool::AnchorPoint, Tool::Puppet];
}
