//! [`PrismAssets`] — static asset source that embeds all SVG icons at compile time.

use gpui::{AssetSource, Result, SharedString};
use std::borrow::Cow;

/// Implements [`AssetSource`] by embedding all Prism UI assets (SVG icons)
/// via `include_bytes!` so they are available without a runtime file-system.
pub struct PrismAssets;

impl AssetSource for PrismAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        let bytes: Option<&'static [u8]> = match path {
            "icons/cursor.svg"       => Some(include_bytes!("../assets/icons/cursor.svg")),
            "icons/pen.svg"          => Some(include_bytes!("../assets/icons/pen.svg")),
            "icons/pencil.svg"       => Some(include_bytes!("../assets/icons/pencil.svg")),
            "icons/brush.svg"        => Some(include_bytes!("../assets/icons/brush.svg")),
            "icons/eraser.svg"       => Some(include_bytes!("../assets/icons/eraser.svg")),
            "icons/move.svg"         => Some(include_bytes!("../assets/icons/move.svg")),
            "icons/zoom_in.svg"      => Some(include_bytes!("../assets/icons/zoom_in.svg")),
            "icons/zoom_out.svg"     => Some(include_bytes!("../assets/icons/zoom_out.svg")),
            "icons/eyedropper.svg"   => Some(include_bytes!("../assets/icons/eyedropper.svg")),
            "icons/text.svg"         => Some(include_bytes!("../assets/icons/text.svg")),
            "icons/rect.svg"         => Some(include_bytes!("../assets/icons/rect.svg")),
            "icons/ellipse.svg"      => Some(include_bytes!("../assets/icons/ellipse.svg")),
            "icons/line.svg"         => Some(include_bytes!("../assets/icons/line.svg")),
            "icons/polygon.svg"      => Some(include_bytes!("../assets/icons/polygon.svg")),
            "icons/lasso.svg"        => Some(include_bytes!("../assets/icons/lasso.svg")),
            "icons/wand.svg"         => Some(include_bytes!("../assets/icons/wand.svg")),
            "icons/crop.svg"         => Some(include_bytes!("../assets/icons/crop.svg")),
            "icons/heal.svg"         => Some(include_bytes!("../assets/icons/heal.svg")),
            "icons/clone.svg"        => Some(include_bytes!("../assets/icons/clone.svg")),
            "icons/gradient.svg"     => Some(include_bytes!("../assets/icons/gradient.svg")),
            "icons/fill.svg"         => Some(include_bytes!("../assets/icons/fill.svg")),
            "icons/shape.svg"        => Some(include_bytes!("../assets/icons/shape.svg")),
            "icons/eye.svg"          => Some(include_bytes!("../assets/icons/eye.svg")),
            "icons/eye_off.svg"      => Some(include_bytes!("../assets/icons/eye_off.svg")),
            "icons/lock.svg"         => Some(include_bytes!("../assets/icons/lock.svg")),
            "icons/unlock.svg"       => Some(include_bytes!("../assets/icons/unlock.svg")),
            "icons/layers.svg"       => Some(include_bytes!("../assets/icons/layers.svg")),
            "icons/add.svg"          => Some(include_bytes!("../assets/icons/add.svg")),
            "icons/remove.svg"       => Some(include_bytes!("../assets/icons/remove.svg")),
            "icons/trash.svg"        => Some(include_bytes!("../assets/icons/trash.svg")),
            "icons/check.svg"        => Some(include_bytes!("../assets/icons/check.svg")),
            "icons/close.svg"        => Some(include_bytes!("../assets/icons/close.svg")),
            "icons/chevron_down.svg" => Some(include_bytes!("../assets/icons/chevron_down.svg")),
            "icons/chevron_right.svg"=> Some(include_bytes!("../assets/icons/chevron_right.svg")),
            "icons/play.svg"         => Some(include_bytes!("../assets/icons/play.svg")),
            "icons/pause.svg"        => Some(include_bytes!("../assets/icons/pause.svg")),
            "icons/stop.svg"         => Some(include_bytes!("../assets/icons/stop.svg")),
            "icons/rewind.svg"       => Some(include_bytes!("../assets/icons/rewind.svg")),
            "icons/fast_forward.svg" => Some(include_bytes!("../assets/icons/fast_forward.svg")),
            "icons/scissors.svg"     => Some(include_bytes!("../assets/icons/scissors.svg")),
            "icons/link.svg"         => Some(include_bytes!("../assets/icons/link.svg")),
            "icons/unlink.svg"       => Some(include_bytes!("../assets/icons/unlink.svg")),
            "icons/grid.svg"         => Some(include_bytes!("../assets/icons/grid.svg")),
            "icons/settings.svg"     => Some(include_bytes!("../assets/icons/settings.svg")),
            "icons/export.svg"       => Some(include_bytes!("../assets/icons/export.svg")),
            "icons/import.svg"       => Some(include_bytes!("../assets/icons/import.svg")),
            "icons/undo.svg"         => Some(include_bytes!("../assets/icons/undo.svg")),
            "icons/redo.svg"         => Some(include_bytes!("../assets/icons/redo.svg")),
            "icons/align_left.svg"   => Some(include_bytes!("../assets/icons/align_left.svg")),
            "icons/align_center.svg" => Some(include_bytes!("../assets/icons/align_center.svg")),
            "icons/align_right.svg"  => Some(include_bytes!("../assets/icons/align_right.svg")),
            "icons/align_top.svg"    => Some(include_bytes!("../assets/icons/align_top.svg")),
            "icons/align_middle.svg" => Some(include_bytes!("../assets/icons/align_middle.svg")),
            "icons/align_bottom.svg" => Some(include_bytes!("../assets/icons/align_bottom.svg")),
            "icons/distribute_h.svg" => Some(include_bytes!("../assets/icons/distribute_h.svg")),
            "icons/distribute_v.svg" => Some(include_bytes!("../assets/icons/distribute_v.svg")),
            "icons/speaker.svg"      => Some(include_bytes!("../assets/icons/speaker.svg")),
            "icons/mute.svg"         => Some(include_bytes!("../assets/icons/mute.svg")),
            "icons/volume.svg"       => Some(include_bytes!("../assets/icons/volume.svg")),
            "icons/camera.svg"       => Some(include_bytes!("../assets/icons/camera.svg")),
            "icons/film.svg"         => Some(include_bytes!("../assets/icons/film.svg")),
            "icons/music.svg"        => Some(include_bytes!("../assets/icons/music.svg")),
            "icons/waveform.svg"     => Some(include_bytes!("../assets/icons/waveform.svg")),
            "icons/cut.svg"          => Some(include_bytes!("../assets/icons/cut.svg")),
            "icons/transition.svg"   => Some(include_bytes!("../assets/icons/transition.svg")),
            "icons/color_wheel.svg"  => Some(include_bytes!("../assets/icons/color_wheel.svg")),
            "icons/mask.svg"         => Some(include_bytes!("../assets/icons/mask.svg")),
            "icons/adjustment.svg"   => Some(include_bytes!("../assets/icons/adjustment.svg")),
            "icons/histogram.svg"    => Some(include_bytes!("../assets/icons/histogram.svg")),
            "icons/star.svg"         => Some(include_bytes!("../assets/icons/star.svg")),
            "icons/heart.svg"        => Some(include_bytes!("../assets/icons/heart.svg")),
            "icons/arrow_up.svg"     => Some(include_bytes!("../assets/icons/arrow_up.svg")),
            "icons/arrow_down.svg"   => Some(include_bytes!("../assets/icons/arrow_down.svg")),
            "icons/arrow_left.svg"   => Some(include_bytes!("../assets/icons/arrow_left.svg")),
            "icons/arrow_right.svg"  => Some(include_bytes!("../assets/icons/arrow_right.svg")),
            _ => None,
        };
        Ok(bytes.map(Cow::Borrowed))
    }

    fn list(&self, _path: &str) -> Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}
