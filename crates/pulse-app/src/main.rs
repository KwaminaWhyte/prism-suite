//! Pulse — motion-graphics / compositing app, app #3 of the Prism creative
//! suite. v0 scaffold entry point.
//!
//! The app's modules live in the crate `lib` (`lib.rs`) so the GPUI host
//! (`pulse-gpui`) can reuse the comp model + compositor; this binary is the thin
//! egui entry point and its behavior is unchanged.

use pulse_app::app::PulseApp;

fn main() -> eframe::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 900.0])
            .with_title("Pulse"),
        ..Default::default()
    };

    eframe::run_native(
        "Pulse",
        options,
        Box::new(|cc| Ok(Box::new(PulseApp::new(cc)))),
    )
}
