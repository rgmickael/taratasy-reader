use eframe::egui;

mod document;
mod ebook;
mod state;
mod ui;

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Taratasy Reader")
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([800.0, 500.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Taratasy Reader",
        options,
        Box::new(|cc| Box::new(ui::ReaderApp::new(cc))),
    )
}
