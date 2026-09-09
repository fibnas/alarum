mod app;
mod audio;
mod model;
mod storage;

use app::AlarumApp;
use eframe::egui::{IconData, Vec2, ViewportBuilder};

fn main() -> eframe::Result<()> {
    let icon = make_icon();
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_title("Alarum")
            .with_inner_size(Vec2::new(760.0, 540.0))
            .with_min_inner_size(Vec2::new(560.0, 400.0))
            .with_icon(icon),
        centered: true,
        ..Default::default()
    };

    eframe::run_native(
        "Alarum",
        options,
        Box::new(|cc| Box::new(AlarumApp::new(cc))),
    )
}

fn make_icon() -> IconData {
    // Simple 32×32 amber bell-like square so the window has an icon.
    const S: usize = 32;
    let mut rgba = vec![0u8; S * S * 4];
    for y in 0..S {
        for x in 0..S {
            let i = (y * S + x) * 4;
            let dx = x as f32 - 15.5;
            let dy = y as f32 - 14.0;
            let bell = (dx.abs() < 8.0 && dy > -8.0 && dy < 8.0 && dx.abs() + (dy + 4.0).max(0.0) * 0.35 < 9.0)
                || (dx * dx + (dy - 10.0) * (dy - 10.0) < 6.0);
            if bell {
                rgba[i] = 232;
                rgba[i + 1] = 168;
                rgba[i + 2] = 56;
                rgba[i + 3] = 255;
            }
        }
    }
    IconData {
        rgba,
        width: S as u32,
        height: S as u32,
    }
}
