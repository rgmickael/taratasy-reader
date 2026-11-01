use crate::document::Document;
use crate::ebook::{self, Block, Book};
use crate::state::AppState;
use eframe::egui::{
    self, Button, Color32, FontFamily, FontId, Frame, Label, Margin, RichText, Rounding,
    ScrollArea, Sense, Stroke, Vec2,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const ORANGE: Color32 = Color32::from_rgb(244, 124, 32);

#[derive(Clone, Copy)]
struct Theme {
    dark: bool,
    paper: Color32,
    sidebar: Color32,
    tabbar: Color32,
    text: Color32,
    muted: Color32,
    line: Color32,
    soft: Color32,
    button: Color32,
    notice_bg: Color32,
    notice_text: Color32,
}

impl Theme {
    fn new(dark: bool) -> Self {
        if dark {
            Self {
                dark,
                paper: Color32::from_rgb(27, 27, 31),
                sidebar: Color32::from_rgb(34, 34, 39),
                tabbar: Color32::from_rgb(21, 21, 24),
                text: Color32::from_rgb(224, 222, 218),
                muted: Color32::from_rgb(152, 150, 158),
                line: Color32::from_rgb(54, 54, 60),
                soft: Color32::from_rgb(72, 48, 28),
                button: Color32::from_rgb(52, 52, 58),
                notice_bg: Color32::from_rgb(78, 36, 32),
                notice_text: Color32::from_rgb(255, 190, 180),
            }
        } else {
            Self {
                dark,
                paper: Color32::from_rgb(251, 248, 243),
                sidebar: Color32::WHITE,
                tabbar: Color32::from_rgb(243, 239, 233),
                text: Color32::from_rgb(38, 38, 42),
                muted: Color32::from_rgb(120, 120, 128),
                line: Color32::from_rgb(230, 226, 220),
                soft: Color32::from_rgb(253, 236, 220),
                button: Color32::from_rgb(240, 235, 228),
                notice_bg: Color32::from_rgb(253, 232, 228),
                notice_text: Color32::from_rgb(170, 40, 30),
            }
        }
    }
}

fn reader_family() -> FontFamily {
    FontFamily::Name("reader".into())
}

enum Action {
    Open,
    Activate(PathBuf),
    Close(PathBuf),
    GoTo(usize),
    Next,
    Prev,
    ScrollBy(f32),
    ToggleSummary,
    ToggleDark,
    FontDelta(f32),
    Quit,
}

#[derive(Clone, Copy, Default)]
struct ScrollInfo {
    offset: f32,
    content: f32,
    viewport: f32,
}

type ImageCache = HashMap<(PathBuf, String), Option<egui::TextureHandle>>;

pub struct ReaderApp {
    state: AppState,
    theme: Theme,
    books: HashMap<PathBuf, Result<Book, String>>,
    notice: Option<String>,
    covers: HashMap<PathBuf, Option<egui::TextureHandle>>,
    images: ImageCache,
    pending_scroll: Option<(f32, u8)>,
    last_scroll: ScrollInfo,
    dirty_since: Option<Instant>,
}

impl ReaderApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_fonts(&cc.egui_ctx);
        let state = AppState::load();
        let dark = state.dark.unwrap_or(cc.egui_ctx.style().visuals.dark_mode);
        let theme = Theme::new(dark);
        setup_style(&cc.egui_ctx, &theme);

        let mut app = Self {
            state,
            theme,
            books: HashMap::new(),
            notice: None,
            covers: HashMap::new(),
            images: HashMap::new(),
            pending_scroll: None,
            last_scroll: ScrollInfo::default(),
            dirty_since: None,
        };
        if let Some(path) = app.state.active.clone() {
            app.ensure_loaded(&path);
            let y = app.state.active_doc().map_or(0.0, |d| d.scroll);
            app.pending_scroll = Some((y, 4));
        }
        app
    }

    fn ensure_loaded(&mut self, path: &Path) {
        if self.books.contains_key(path) {
            return;
        }
        let res = ebook::load(path).map_err(|e| format!("{e:#}"));
        if let Ok(b) = &res {
            if let Some(d) = self.state.doc_mut(path) {
                d.total_chapters = b.chapters.len();
                d.chapter = d.chapter.min(b.chapters.len().saturating_sub(1));
                d.title = b.title.clone();
                d.author = b.author.clone();
                d.progress = b.progress_at(d.chapter, 0.0);
            }
        }
        self.books.insert(path.to_path_buf(), res);
    }

    fn ensure_cover(&mut self, ctx: &egui::Context, path: &Path) {
        if self.covers.contains_key(path) {
            return;
        }
        let Some(Ok(book)) = self.books.get(path) else { return };
        let tex = book.cover.as_ref().and_then(|b| decode_texture(ctx, &path.display().to_string(), b, 360));
        self.covers.insert(path.to_path_buf(), tex);
    }

    fn open_path(&mut self, path: PathBuf) {
        if !self.state.docs.iter().any(|d| d.path == path) {
            self.state.docs.push(Document {
                path: path.clone(),
                title: path.file_stem().and_then(|s| s.to_str()).unwrap_or("Book").to_string(),
                total_chapters: 1,
                ..Default::default()
            });
        }
        self.ensure_loaded(&path);
        let err = match self.books.get(&path) {
            Some(Err(e)) => Some(e.clone()),
            _ => None,
        };
        if let Some(e) = err {
            self.notice = Some(format!("Could not open {}: {e}", path.display()));
            self.books.remove(&path);
            self.state.close(&path);
        } else {
            let y = self.state.doc_mut(&path).map_or(0.0, |d| d.scroll);
            self.pending_scroll = Some((y, 4));
            self.state.active = Some(path);
        }
        self.state.save();
    }

    fn set_chapter(&mut self, chapter: usize) {
        let Some(path) = self.state.active.clone() else { return };
        let progress = match self.books.get(&path) {
            Some(Ok(b)) => b.progress_at(chapter.min(b.chapters.len().saturating_sub(1)), 0.0),
            _ => return,
        };
        if let Some(d) = self.state.doc_mut(&path) {
            let chapter = chapter.min(d.total_chapters.saturating_sub(1));
            if chapter != d.chapter {
                d.chapter = chapter;
                d.scroll = 0.0;
                d.progress = progress;
                self.pending_scroll = Some((0.0, 3));
                self.last_scroll = ScrollInfo::default();
                self.state.save();
            }
        }
    }

    fn apply(&mut self, ctx: &egui::Context, action: Action) {
        match action {
            Action::Open => {
                if let Some(p) = rfd::FileDialog::new()
                    .add_filter("E-books", &["epub", "mobi", "azw", "azw3", "prc"])
                    .pick_file()
                {
                    self.open_path(p);
                }
            }
            Action::Activate(p) => {
                self.ensure_loaded(&p);
                let y = self.state.doc_mut(&p).map_or(0.0, |d| d.scroll);
                self.pending_scroll = Some((y, 4));
                self.state.active = Some(p);
                self.state.save();
            }
            Action::Close(p) => {
                self.state.close(&p);
                self.books.remove(&p);
                self.covers.remove(&p);
                self.images.retain(|(path, _), _| path != &p);
                if let Some(a) = self.state.active.clone() {
                    self.ensure_loaded(&a);
                    let y = self.state.doc_mut(&a).map_or(0.0, |d| d.scroll);
                    self.pending_scroll = Some((y, 4));
                }
                self.state.save();
            }
            Action::GoTo(n) => self.set_chapter(n),
            Action::Next => {
                if let Some(d) = self.state.active_doc() {
                    self.set_chapter(d.chapter + 1);
                }
            }
            Action::Prev => {
                if let Some(d) = self.state.active_doc() {
                    self.set_chapter(d.chapter.saturating_sub(1));
                }
            }
            Action::ScrollBy(dy) => {
                let s = self.last_scroll;
                let at_bottom = s.offset >= s.content - s.viewport - 2.0;
                if dy > 0.0 && at_bottom {
                    self.apply(ctx, Action::Next);
                } else {
                    self.pending_scroll = Some(((s.offset + dy).max(0.0), 1));
                }
            }
            Action::ToggleSummary => {
                self.state.show_summary = !self.state.show_summary;
                self.state.save();
            }
            Action::ToggleDark => {
                self.state.dark = Some(!self.theme.dark);
                self.theme = Theme::new(!self.theme.dark);
                setup_style(ctx, &self.theme);
                self.state.save();
            }
            Action::FontDelta(d) => {
                self.state.font_size = (self.state.font_size + d).clamp(13.0, 34.0);
                self.state.save();
            }
            Action::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        }
    }

    fn track_scroll(&mut self, info: Option<ScrollInfo>) {
        let Some(i) = info else { return };
        self.last_scroll = i;
        if self.pending_scroll.is_some() {
            return;
        }
        let Some(path) = self.state.active.clone() else { return };
        let Some(chapter) = self.state.active_doc().map(|d| d.chapter) else { return };
        let range = (i.content - i.viewport).max(1.0);
        let progress = match self.books.get(&path) {
            Some(Ok(b)) => b.progress_at(chapter, i.offset / range),
            _ => return,
        };
        if let Some(d) = self.state.doc_mut(&path) {
            if (d.scroll - i.offset).abs() > 1.0 {
                d.scroll = i.offset;
                d.progress = progress;
                self.dirty_since.get_or_insert_with(Instant::now);
            }
        }
    }

    fn flush_dirty(&mut self, ctx: &egui::Context) {
        if let Some(t) = self.dirty_since {
            if t.elapsed() >= Duration::from_millis(900) {
                self.state.save();
                self.dirty_since = None;
            } else {
                ctx.request_repaint_after(Duration::from_millis(300));
            }
        }
    }
}

impl eframe::App for ReaderApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut actions: Vec<Action> = Vec::new();

        if ctx.memory(|m| m.focused().is_none()) {
            let vp = self.last_scroll.viewport.max(200.0);
            ctx.input(|i| {
                if i.key_pressed(egui::Key::ArrowRight) {
                    actions.push(Action::Next);
                }
                if i.key_pressed(egui::Key::ArrowLeft) {
                    actions.push(Action::Prev);
                }
                if i.key_pressed(egui::Key::PageDown) || i.key_pressed(egui::Key::Space) {
                    actions.push(Action::ScrollBy(vp * 0.9));
                }
                if i.key_pressed(egui::Key::PageUp) {
                    actions.push(Action::ScrollBy(-vp * 0.9));
                }
                if i.key_pressed(egui::Key::ArrowDown) {
                    actions.push(Action::ScrollBy(60.0));
                }
                if i.key_pressed(egui::Key::ArrowUp) {
                    actions.push(Action::ScrollBy(-60.0));
                }
                if i.modifiers.command && i.key_pressed(egui::Key::O) {
                    actions.push(Action::Open);
                }
                if i.modifiers.command && i.key_pressed(egui::Key::D) {
                    actions.push(Action::ToggleDark);
                }
            });
        }

        if let Some(p) = self.state.active.clone() {
            self.ensure_cover(ctx, &p);
        }
        self.menu_bar(ctx, &mut actions);
        self.tab_bar(ctx, &mut actions);
        self.notice_bar(ctx);
        if self.state.active_doc().is_some() {
            self.nav_bar(ctx, &mut actions);
            if self.state.show_summary {
                self.summary_panel(ctx, &mut actions);
            }
        }
        let info = self.central(ctx, &mut actions);
        self.track_scroll(info);

        for a in actions {
            self.apply(ctx, a);
        }
        self.flush_dirty(ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.state.save();
    }
}

impl ReaderApp {
    fn menu_bar(&self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let t = self.theme;
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open book...    Ctrl+O").clicked() {
                        actions.push(Action::Open);
                        ui.close_menu();
                    }
                    if let Some(a) = &self.state.active {
                        if ui.button("Close tab").clicked() {
                            actions.push(Action::Close(a.clone()));
                            ui.close_menu();
                        }
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        actions.push(Action::Quit);
                    }
                });
                ui.menu_button("View", |ui| {
                    let dark_label = if t.dark { "Light mode    Ctrl+D" } else { "Dark mode    Ctrl+D" };
                    if ui.button(dark_label).clicked() {
                        actions.push(Action::ToggleDark);
                        ui.close_menu();
                    }
                    let label = if self.state.show_summary { "Hide summary panel" } else { "Show summary panel" };
                    if ui.button(label).clicked() {
                        actions.push(Action::ToggleSummary);
                        ui.close_menu();
                    }
                    if ui.button("Larger text").clicked() {
                        actions.push(Action::FontDelta(1.0));
                    }
                    if ui.button("Smaller text").clicked() {
                        actions.push(Action::FontDelta(-1.0));
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new("Taratasy Reader").color(ORANGE).strong());
                });
            });
        });
    }

    fn tab_bar(&self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let t = self.theme;
        egui::TopBottomPanel::top("tabs")
            .frame(Frame::none().fill(t.tabbar).inner_margin(Margin::symmetric(8.0, 0.0)))
            .show(ctx, |ui| {
                ScrollArea::horizontal().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        for d in &self.state.docs {
                            let active = self.state.active.as_ref() == Some(&d.path);
                            let mut title: String = d.title.chars().take(26).collect();
                            if d.title.chars().count() > 26 {
                                title.push('\u{2026}');
                            }
                            let text = RichText::new(title).size(14.0).color(if active { t.text } else { t.muted });
                            let text = if active { text.strong() } else { text };

                            let resp = ui
                                .add(Label::new(text).selectable(false).sense(Sense::click()))
                                .on_hover_text(d.path.display().to_string())
                                .on_hover_cursor(egui::CursorIcon::PointingHand);
                            if resp.clicked() {
                                actions.push(Action::Activate(d.path.clone()));
                            }
                            let close = ui.add(
                                Label::new(RichText::new("\u{00d7}").size(16.0).color(t.muted))
                                    .selectable(false)
                                    .sense(Sense::click()),
                            );
                            let close = close.on_hover_cursor(egui::CursorIcon::PointingHand);
                            if close.clicked() {
                                actions.push(Action::Close(d.path.clone()));
                            }
                            if active {
                                let r = resp.rect.union(close.rect);
                                ui.painter().hline(r.x_range(), ui.max_rect().bottom() - 1.0, Stroke::new(3.0_f32, ORANGE));
                            }
                            ui.add_space(14.0);
                        }
                        if ui
                            .add(
                                Label::new(RichText::new("+").size(18.0).color(ORANGE).strong())
                                    .selectable(false)
                                    .sense(Sense::click()),
                            )
                            .on_hover_text("Open book")
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            actions.push(Action::Open);
                        }
                    });
                });
                ui.add_space(2.0);
            });
    }

    fn notice_bar(&mut self, ctx: &egui::Context) {
        let t = self.theme;
        let Some(msg) = self.notice.clone() else { return };
        egui::TopBottomPanel::top("notice")
            .frame(Frame::none().fill(t.notice_bg).inner_margin(Margin::symmetric(12.0, 6.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(msg).color(t.notice_text));
                    if ui.small_button("Dismiss").clicked() {
                        self.notice = None;
                    }
                });
            });
    }

    fn nav_bar(&self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let t = self.theme;
        let Some(doc) = self.state.active_doc() else { return };
        let n = doc.total_chapters.max(1);
        egui::TopBottomPanel::bottom("nav")
            .frame(
                Frame::none()
                    .fill(t.sidebar)
                    .inner_margin(Margin::symmetric(16.0, 8.0))
                    .stroke(Stroke::new(1.0_f32, t.line)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.add_enabled(doc.chapter > 0, Button::new("\u{2190} Prev")).clicked() {
                        actions.push(Action::Prev);
                    }
                    ui.spacing_mut().slider_width = (ui.available_width() - 290.0).max(100.0);
                    let mut c = doc.chapter + 1;
                    let slider = egui::Slider::new(&mut c, 1..=n).show_value(false);
                    if ui.add_enabled(n > 1, slider).changed() {
                        actions.push(Action::GoTo(c - 1));
                    }
                    ui.label(
                        RichText::new(format!("Section {} / {}  \u{b7}  {:.0}%", doc.chapter + 1, n, doc.progress * 100.0))
                            .color(t.muted),
                    );
                    if ui.add_enabled(doc.chapter + 1 < n, Button::new("Next \u{2192}")).clicked() {
                        actions.push(Action::Next);
                    }
                });
            });
    }

    fn summary_panel(&self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let t = self.theme;
        let Some(doc) = self.state.active_doc() else { return };
        let book = match self.books.get(&doc.path) {
            Some(Ok(b)) => Some(b),
            _ => None,
        };

        egui::SidePanel::left("summary")
            .exact_width(300.0)
            .resizable(false)
            .frame(
                Frame::none()
                    .fill(t.sidebar)
                    .inner_margin(Margin::same(18.0))
                    .stroke(Stroke::new(1.0_f32, t.line)),
            )
            .show(ctx, |ui| {
                let (bar, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 4.0), Sense::hover());
                ui.painter().rect_filled(bar, Rounding::same(2.0), ORANGE);
                ui.add_space(14.0);

                if let Some(Some(tex)) = self.covers.get(&doc.path) {
                    let sz = tex.size_vec2();
                    let scale = (150.0 / sz.y).min(200.0 / sz.x);
                    ui.vertical_centered(|ui| {
                        ui.add(
                            egui::Image::new(egui::load::SizedTexture::new(tex.id(), sz * scale))
                                .rounding(Rounding::same(6.0)),
                        );
                    });
                    ui.add_space(14.0);
                }

                ui.add(Label::new(RichText::new(&doc.title).size(20.0).strong().color(t.text)));
                if !doc.author.is_empty() {
                    ui.add(Label::new(RichText::new(&doc.author).color(t.muted)));
                }
                ui.add_space(16.0);

                section_label(ui, "PROGRESS");
                ui.add(
                    egui::ProgressBar::new(doc.progress)
                        .fill(ORANGE)
                        .desired_height(10.0)
                        .rounding(Rounding::same(5.0)),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!(
                        "Section {} of {}  \u{b7}  {:.0}%",
                        doc.chapter + 1,
                        doc.total_chapters,
                        doc.progress * 100.0
                    ))
                    .color(t.text),
                );
                if let Some(b) = book {
                    let words = b.total_chars() as f32 * (1.0 - doc.progress) / 6.0;
                    let mins = words / 230.0;
                    let left = if mins >= 60.0 {
                        format!("~{}h {:02}m left", (mins / 60.0) as u32, (mins % 60.0) as u32)
                    } else {
                        format!("~{} min left", mins.max(1.0) as u32)
                    };
                    ui.label(RichText::new(left).color(t.muted));
                }
                ui.add_space(16.0);

                section_label(ui, "CONTENTS");
                if let Some(book) = book {
                    ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        for (i, c) in book.chapters.iter().enumerate() {
                            let is_cur = i == doc.chapter;
                            let mut title: String = c.title.chars().take(40).collect();
                            if c.title.chars().count() > 40 {
                                title.push('\u{2026}');
                            }
                            let text = RichText::new(title).size(14.0).color(if is_cur { ORANGE } else { t.text });
                            let text = if is_cur { text.strong() } else { text };
                            let resp = Frame::none()
                                .fill(if is_cur { t.soft } else { Color32::TRANSPARENT })
                                .rounding(Rounding::same(4.0))
                                .inner_margin(Margin::symmetric(6.0, 3.0))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.add(Label::new(text).selectable(false))
                                })
                                .response
                                .interact(Sense::click())
                                .on_hover_cursor(egui::CursorIcon::PointingHand);
                            if resp.clicked() {
                                actions.push(Action::GoTo(i));
                            }
                        }
                    });
                } else {
                    ui.label(RichText::new("Not available").color(t.muted));
                }
            });
    }

    fn central(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) -> Option<ScrollInfo> {
        let t = self.theme;
        let mut info = None;
        let pending = self.pending_scroll.map(|(y, _)| y);
        egui::CentralPanel::default().frame(Frame::none().fill(t.paper)).show(ctx, |ui| {
            let Some(doc) = self.state.active_doc() else {
                welcome(ui, &t, actions);
                return;
            };
            match self.books.get(&doc.path) {
                Some(Ok(book)) => {
                    info = reader(
                        ui,
                        ctx,
                        &t,
                        book,
                        &doc.path,
                        doc.chapter,
                        self.state.font_size,
                        &mut self.images,
                        pending,
                    );
                }
                Some(Err(e)) => {
                    ui.vertical_centered(|ui| {
                        ui.add_space(120.0);
                        ui.label(RichText::new("This book could not be opened").size(20.0).color(t.text));
                        ui.label(RichText::new(e).color(t.muted));
                        ui.add_space(12.0);
                        if ui.button("Close tab").clicked() {
                            actions.push(Action::Close(doc.path.clone()));
                        }
                    });
                }
                None => {
                    ui.label("Loading...");
                }
            }
        });
        if let Some((y, n)) = self.pending_scroll {
            self.pending_scroll = if n <= 1 { None } else { Some((y, n - 1)) };
            ctx.request_repaint();
        }
        info
    }
}

#[allow(clippy::too_many_arguments)]
fn reader(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    t: &Theme,
    book: &Book,
    path: &Path,
    chapter: usize,
    size: f32,
    images: &mut ImageCache,
    pending: Option<f32>,
) -> Option<ScrollInfo> {
    let chap = book.chapters.get(chapter)?;
    let width = ui.available_width();
    let column = (size * 36.0).min(width - 48.0).max(240.0);
    let side = ((width - column) / 2.0).max(24.0);
    let viewport_h = ui.available_height();

    let mut area = ScrollArea::vertical()
        .id_source(("chapter", path, chapter))
        .auto_shrink([false, false]);
    if let Some(y) = pending {
        area = area.vertical_scroll_offset(y);
    }

    let out = area.show(ui, |ui| {
        Frame::none()
            .inner_margin(Margin { left: side, right: side, top: 36.0, bottom: 60.0 })
            .show(ui, |ui| {
                ui.set_width(column);
                for block in &chap.blocks {
                    match block {
                        Block::Heading(h) => {
                            ui.add_space(size * 0.4);
                            ui.add(Label::new(
                                RichText::new(h)
                                    .font(FontId::new(size * 1.55, reader_family()))
                                    .strong()
                                    .color(t.text),
                            ));
                            ui.add_space(size * 0.8);
                        }
                        Block::Para(p) => {
                            ui.add(Label::new(
                                RichText::new(p)
                                    .font(FontId::new(size, reader_family()))
                                    .line_height(Some(size * 1.65))
                                    .color(t.text),
                            ));
                            ui.add_space(size * 0.75);
                        }
                        Block::Image(key) => {
                            let tex = images
                                .entry((path.to_path_buf(), key.clone()))
                                .or_insert_with(|| {
                                    book.image_bytes(key).and_then(|b| decode_texture(ctx, key, &b, 1600))
                                })
                                .as_ref();
                            if let Some(tex) = tex {
                                let sz = tex.size_vec2();
                                let scale = (column / sz.x).min(viewport_h * 0.85 / sz.y).min(1.0);
                                ui.vertical_centered(|ui| {
                                    ui.add(
                                        egui::Image::new(egui::load::SizedTexture::new(tex.id(), sz * scale))
                                            .rounding(Rounding::same(4.0)),
                                    );
                                });
                                ui.add_space(size * 0.75);
                            }
                        }
                    }
                }
            });
    });

    Some(ScrollInfo {
        offset: out.state.offset.y,
        content: out.content_size.y,
        viewport: out.inner_rect.height(),
    })
}

fn decode_texture(ctx: &egui::Context, name: &str, bytes: &[u8], max: u32) -> Option<egui::TextureHandle> {
    let img = image::load_from_memory(bytes).ok()?;
    let img = if img.width() > max || img.height() > max { img.thumbnail(max, max) } else { img };
    let img = img.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    Some(ctx.load_texture(
        name,
        egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()),
        egui::TextureOptions::LINEAR,
    ))
}

fn section_label(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(12.0).strong().color(ORANGE));
    ui.add_space(4.0);
}

fn welcome(ui: &mut egui::Ui, t: &Theme, actions: &mut Vec<Action>) {
    ui.vertical_centered(|ui| {
        ui.add_space(ui.available_height() * 0.28);
        ui.label(RichText::new("Taratasy Reader").size(34.0).strong().color(t.text));
        ui.add_space(6.0);
        ui.label(RichText::new("EPUB, MOBI and AZW books, one tab each.").size(16.0).color(t.muted));
        ui.add_space(22.0);
        if ui
            .add(
                Button::new(RichText::new("Open a book").size(16.0).color(Color32::WHITE))
                    .fill(ORANGE)
                    .min_size(Vec2::new(160.0, 40.0))
                    .rounding(Rounding::same(8.0)),
            )
            .clicked()
        {
            actions.push(Action::Open);
        }
        ui.add_space(8.0);
        ui.label(RichText::new("or press Ctrl+O").size(13.0).color(t.muted));
    });
}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let candidates = [
        "/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSerif-Regular.ttf",
        "/usr/share/fonts/truetype/liberation2/LiberationSerif-Regular.ttf",
        "/usr/share/fonts/truetype/noto/NotoSerif-Regular.ttf",
        "/usr/share/fonts/TTF/DejaVuSerif.ttf",
    ];
    let mut chain: Vec<String> = Vec::new();
    for p in candidates {
        if let Ok(bytes) = std::fs::read(p) {
            fonts.font_data.insert("reader-serif".into(), egui::FontData::from_owned(bytes));
            chain.push("reader-serif".into());
            break;
        }
    }
    if let Some(list) = fonts.families.get(&FontFamily::Proportional) {
        chain.extend(list.iter().cloned());
    }
    fonts.families.insert(reader_family(), chain);
    ctx.set_fonts(fonts);
}

fn setup_style(ctx: &egui::Context, t: &Theme) {
    let mut v = if t.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = t.paper;
    v.window_fill = t.sidebar;
    v.extreme_bg_color = t.sidebar;
    v.selection.bg_fill = ORANGE;
    v.selection.stroke = Stroke::new(1.0_f32, Color32::WHITE);
    v.hyperlink_color = ORANGE;
    v.widgets.noninteractive.fg_stroke.color = t.text;
    v.widgets.noninteractive.bg_stroke.color = t.line;
    v.widgets.inactive.fg_stroke.color = t.text;
    v.widgets.inactive.bg_fill = t.button;
    v.widgets.inactive.weak_bg_fill = t.button;
    v.widgets.hovered.weak_bg_fill = t.soft;
    v.widgets.hovered.bg_fill = t.soft;
    v.widgets.active.weak_bg_fill = ORANGE;
    v.widgets.active.bg_fill = ORANGE;
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active] {
        w.rounding = Rounding::same(6.0);
    }
    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        s.spacing.item_spacing = Vec2::new(8.0, 6.0);
        s.spacing.button_padding = Vec2::new(12.0, 5.0);
    });
}
