use std::path::{Path, PathBuf};

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Frame, Margin, RichText, Stroke, StrokeKind};

use easyflp::{info, ops};

const BG: Color32 = Color32::from_rgb(0x0C, 0x0C, 0x0E);
const PANEL: Color32 = Color32::from_rgb(0x14, 0x14, 0x17);
const PANEL_LIT: Color32 = Color32::from_rgb(0x21, 0x20, 0x2A);
const ACCENT: Color32 = Color32::from_rgb(0xE8, 0x6A, 0xC4);
const VIOLET: Color32 = Color32::from_rgb(0x8A, 0x6A, 0xFF);
const BTN: Color32 = Color32::from_rgb(0x2A, 0x2A, 0x31);
const TEXT: Color32 = Color32::from_rgb(0x9C, 0x9C, 0xA8);
const DIM: Color32 = Color32::from_rgb(0x5E, 0x5E, 0x68);
const WHITE: Color32 = Color32::from_rgb(0xE9, 0xE9, 0xEF);
const OK: Color32 = Color32::from_rgb(0x45, 0xD8, 0x6A);
const ERR: Color32 = Color32::from_rgb(0xFF, 0x4D, 0x6A);

const HOVER_ANIM_SECS: f32 = 0.18;

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    egui::lerp(egui::Rgba::from(a)..=egui::Rgba::from(b), t).into()
}

pub struct App {
    loaded: Option<ops::LoadedProject>,
    done: Option<ops::ConvertDone>,
    error: Option<String>,
    show_result: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, initial: Option<PathBuf>) -> Self {
        let mut style = (*cc.egui_ctx.style()).clone();
        for font in style.text_styles.values_mut() {
            font.family = egui::FontFamily::Monospace;
        }
        style.visuals = egui::Visuals::dark();
        style.visuals.override_text_color = Some(TEXT);
        style.visuals.panel_fill = BG;
        style.visuals.widgets.inactive.weak_bg_fill = BTN;
        /* chrome text (title, version, buttons) must not be text-selectable; the info viewer opts back in */
        style.interaction.selectable_labels = false;
        cc.egui_ctx.set_style(style);

        let mut app = App { loaded: None, done: None, error: None, show_result: false };
        if let Some(p) = initial {
            app.open_path(&p);
        }
        app
    }

    fn go_home(&mut self) {
        self.loaded = None;
        self.done = None;
        self.error = None;
        self.show_result = false;
    }

    fn open_path(&mut self, path: &Path) {
        self.done = None;
        self.error = None;
        self.loaded = None;
        match ops::load(path) {
            Ok(l) => self.loaded = Some(l),
            Err(e) => self.error = Some(e),
        }
    }

    fn run_convert(&mut self) {
        self.done = None;
        self.error = None;
        let Some(l) = &self.loaded else { return };
        match ops::convert_and_write(l) {
            Ok(done) => self.done = Some(done),
            Err(e) => self.error = Some(e),
        }
        self.show_result = true;
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(path) = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .find_map(|f| f.path.clone())
        }) {
            self.open_path(&path);
        }
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
        let drop_t = ctx.animate_bool_with_time(egui::Id::new("file_hover"), hovering, HOVER_ANIM_SECS);

        self.top_bar(ctx);
        egui::CentralPanel::default()
            .frame(Frame::default().fill(BG).inner_margin(Margin::same(16)))
            .show(ctx, |ui| {
                if self.loaded.is_some() {
                    self.draw_info(ui);
                    if drop_t > 0.0 {
                        let r = ui.clip_rect();
                        ui.painter().rect_filled(r, CornerRadius::ZERO, Color32::from_white_alpha((10.0 * drop_t) as u8));
                        ui.painter().text(
                            r.center(),
                            Align2::CENTER_CENTER,
                            "drop to open",
                            FontId::monospace(20.0),
                            WHITE.gamma_multiply(drop_t),
                        );
                    }
                } else {
                    self.draw_drop_zone(ui, drop_t);
                }
            });
        self.result_modal(ctx);
    }
}

impl App {
    fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top")
            .frame(Frame::default().fill(PANEL).inner_margin(Margin::symmetric(14, 10)))
            .show(ctx, |ui| {
                let drag = ui.interact(
                    ui.max_rect(),
                    egui::Id::new("title_bar"),
                    egui::Sense::click_and_drag(),
                );
                if drag.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                if drag.double_clicked() {
                    let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                }
                ui.horizontal(|ui| {
                    let logo = ui
                        .add(
                            egui::Label::new(RichText::new("easyflp").color(ACCENT).strong().size(19.0))
                                .sense(egui::Sense::click()),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    if logo.hovered() {
                        ui.painter()
                            .hline(logo.rect.x_range(), logo.rect.bottom() - 1.0, Stroke::new(1.5f32, ACCENT));
                    }
                    if logo.clicked() {
                        self.go_home();
                    }
                    let convertible = self
                        .loaded
                        .as_ref()
                        .map(|l| l.roundtrip_ok && l.info.major > 20)
                        .unwrap_or(false);
                    if ui
                        .add_enabled(
                            convertible,
                            egui::Button::new(RichText::new("convert to v20 project").color(ACCENT))
                                .fill(BTN),
                        )
                        .clicked()
                    {
                        self.run_convert();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(egui::Button::new(RichText::new("×").color(WHITE).size(20.0)).frame(false))
                            .clicked()
                        {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if ui
                            .add(egui::Button::new(RichText::new("–").color(WHITE).size(20.0)).frame(false))
                            .clicked()
                        {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                        }
                        ui.label(RichText::new(concat!("v", env!("CARGO_PKG_VERSION"))).color(DIM));
                        if let Some(l) = &self.loaded {
                            let name = l
                                .path
                                .file_name()
                                .map(|s| s.to_string_lossy().into_owned())
                                .unwrap_or_default();
                            ui.add_space(14.0);
                            ui.label(
                                RichText::new(format!("{name}  |  v{}", l.info.version))
                                    .color(DIM)
                                    .monospace(),
                            );
                        }
                    });
                });
            });
    }

    fn result_modal(&mut self, ctx: &egui::Context) {
        if !self.show_result {
            return;
        }
        let out = self.done.as_ref().map(|d| d.out.clone());
        let warnings = self.done.as_ref().map(|d| d.warnings.len()).unwrap_or(0);
        let error = self.error.clone();
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::new("convert_result"))
            .frame(Frame::default().fill(PANEL).inner_margin(Margin::same(20)).corner_radius(CornerRadius::same(10)))
            .show(ctx, |ui| {
                ui.set_max_width(560.0);
                match (&out, &error) {
                    (Some(out), _) => {
                        ui.label(RichText::new("conversion complete").color(OK).strong().size(16.0));
                        ui.add_space(10.0);
                        ui.label(RichText::new(format!("wrote {}", out.display())).color(WHITE).monospace());
                        if warnings > 0 {
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(format!("{warnings} warning(s) — see the conversion section for details"))
                                    .color(VIOLET),
                            );
                        }
                        ui.add_space(16.0);
                        ui.horizontal(|ui| {
                            if ui.add(egui::Button::new(RichText::new("OK").color(WHITE)).fill(BTN)).clicked() {
                                close = true;
                            }
                            if ui
                                .add(egui::Button::new(RichText::new(REVEAL_LABEL).color(ACCENT)).fill(BTN))
                                .clicked()
                            {
                                show_in_file_manager(out);
                                close = true;
                            }
                        });
                    }
                    (None, Some(err)) => {
                        ui.label(RichText::new("conversion failed").color(ERR).strong().size(16.0));
                        ui.add_space(10.0);
                        ui.label(RichText::new(err).color(WHITE));
                        ui.add_space(16.0);
                        if ui.add(egui::Button::new(RichText::new("OK").color(WHITE)).fill(BTN)).clicked() {
                            close = true;
                        }
                    }
                    (None, None) => close = true,
                }
            });
        if close || modal.should_close() || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.show_result = false;
        }
    }

    fn draw_drop_zone(&mut self, ui: &mut egui::Ui, drop_t: f32) {
        let rect = ui.available_rect_before_wrap().shrink(48.0);
        let click = ui
            .allocate_rect(rect, egui::Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if click.clicked() {
            if let Some(p) = rfd::FileDialog::new()
                .add_filter("project", &["flp", "zip"])
                .pick_file()
            {
                self.open_path(&p);
            }
        }
        let mouse_t = ui
            .ctx()
            .animate_bool_with_time(egui::Id::new("zone_hover"), click.hovered(), HOVER_ANIM_SECS);
        let t = drop_t.max(mouse_t);
        ui.painter().rect_filled(rect, CornerRadius::same(14), mix(PANEL, PANEL_LIT, t));
        ui.painter()
            .rect_stroke(rect, CornerRadius::same(14), Stroke::new(1.0f32, mix(DIM, TEXT, t)), StrokeKind::Inside);
        let c = rect.center();
        ui.painter().text(
            c - egui::vec2(0.0, 18.0),
            Align2::CENTER_CENTER,
            "drop a .flp or .zip file here",
            FontId::monospace(24.0),
            WHITE,
        );
        ui.painter().text(
            c + egui::vec2(0.0, 16.0),
            Align2::CENTER_CENTER,
            "or click to open file explorer",
            FontId::monospace(14.0),
            mix(DIM, TEXT, t),
        );
        if let Some(err) = &self.error {
            ui.painter().text(
                c + egui::vec2(0.0, 52.0),
                Align2::CENTER_CENTER,
                err,
                FontId::monospace(14.0),
                ERR,
            );
        }
    }

    fn draw_info(&mut self, ui: &mut egui::Ui) {
        let Some(l) = &self.loaded else { return };

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.style_mut().interaction.selectable_labels = true;
                ui.label(RichText::new("project").color(ACCENT).strong());
                ui.add_space(4.0);
                egui::Grid::new("project").num_columns(2).spacing([28.0, 5.0]).show(ui, |ui| {
                    let mut row = |k: &str, v: String| {
                        ui.label(RichText::new(k).color(DIM));
                        ui.label(RichText::new(v).color(WHITE).monospace());
                        ui.end_row();
                    };
                    row("file", format!("{} ({} KB)", l.path.display(), l.file_size / 1024));
                    if let Some(entry) = &l.zip_entry {
                        row("zip entry", entry.clone());
                    }
                    row(
                        "version",
                        format!(
                            "{}{}",
                            l.info.version,
                            l.info.build.map(|b| format!("  build {b}")).unwrap_or_default()
                        ),
                    );
                    if !l.info.title.is_empty() {
                        row("title", l.info.title.clone());
                    }
                    row("tempo", format!("{:.3} bpm", l.info.tempo));
                    row("time signature", format!("{}/{}", l.info.timesig.0, l.info.timesig.1));
                    row("ppq", format!("{}", l.info.ppq));
                    row("events", format!("{}", l.info.event_count));
                });

                section(ui, "format profile");
                egui::Grid::new("profile").num_columns(2).spacing([28.0, 5.0]).show(ui, |ui| {
                    let mut row = |k: &str, v: String| {
                        ui.label(RichText::new(k).color(DIM));
                        ui.label(RichText::new(v).color(WHITE).monospace());
                        ui.end_row();
                    };
                    if let Some(m) = l.info.wrapper_marker {
                        row("wrapper marker", format!("{m}  ({})", if m >= 12 { "v24/25" } else { "v20/21" }));
                    }
                    if let Some(s) = l.info.clip_record_size {
                        row(
                            "clip record",
                            format!(
                                "{s} bytes  ({})",
                                match s {
                                    80 => "v25+",
                                    60 => "v21-24",
                                    _ => "v20",
                                }
                            ),
                        );
                    }
                    if let Some(s) = l.info.lane_record_size {
                        row("lane record", format!("{s} bytes  ({})", if s >= 70 { "v24/25" } else { "v20/21" }));
                    }
                    if let Some(s) = l.info.route_table_size {
                        row("route table", format!("{s} bytes  ({})", if s < 127 { "v25" } else { "v20-24" }));
                    }
                    row("channel routing", l.info.route_style.to_string());
                    if let Some(b) = l.info.mixer_param_base {
                        row("mixer param base", format!("0x{b:04X}  ({})", if b == 0x7000 { "v25" } else { "v20-24" }));
                    }
                });

                if !l.info.channels.is_empty() {
                    section(ui, &format!("channels ({})", l.info.channels.len()));
                    for c in &l.info.channels {
                        let mut line = format!("{:>3}  {:<11} {}", c.iid, info::kind_name(c.kind), c.name);
                        if !c.detail.is_empty() && c.detail != c.name {
                            line.push_str(&format!("  —  {}", c.detail));
                        }
                        ui.label(RichText::new(line).color(TEXT).monospace());
                    }
                }

                let used: Vec<_> = l
                    .info
                    .patterns
                    .iter()
                    .filter(|p| p.notes > 0 || p.name.is_some())
                    .collect();
                if !used.is_empty() {
                    section(ui, &format!("patterns ({})", used.len()));
                    for p in used {
                        ui.label(
                            RichText::new(format!(
                                "{:>3}  {:<24} {} notes",
                                p.id,
                                p.name.clone().unwrap_or_else(|| format!("Pattern {}", p.id)),
                                p.notes
                            ))
                            .color(TEXT)
                            .monospace(),
                        );
                    }
                }

                section(ui, "playlist");
                ui.label(
                    RichText::new(format!(
                        "{} pattern clips, {} audio clips on {} lanes",
                        l.info.pattern_clips, l.info.audio_clips, l.info.lanes_used
                    ))
                    .color(TEXT)
                    .monospace(),
                );

                if !l.info.effects.is_empty() {
                    section(ui, &format!("mixer effects ({})", l.info.effects.len()));
                    for e in &l.info.effects {
                        ui.label(
                            RichText::new(format!("insert {:>3}  slot {}  {}", e.insert, e.slot, e.name))
                                .color(TEXT)
                                .monospace(),
                        );
                    }
                }

                ui.add_space(10.0);
                egui::CollapsingHeader::new(RichText::new("event histogram").color(ACCENT))
                    .show(ui, |ui| {
                        for (op, ty, n) in &l.info.op_histogram {
                            ui.label(
                                RichText::new(format!("0x{op:02X}  {ty:<7} x{n}"))
                                    .color(DIM)
                                    .monospace(),
                            );
                        }
                    });

                if !l.roundtrip_ok {
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new(
                            "parser cannot reproduce this file byte-exact — conversion disabled for safety",
                        )
                        .color(ERR),
                    );
                }
                if l.info.major <= 20 && l.roundtrip_ok {
                    ui.add_space(12.0);
                    ui.label(RichText::new("nothing to convert").color(OK));
                }

                if let Some(done) = &self.done {
                    section(ui, "conversion");
                    ui.label(
                        RichText::new(format!("wrote {}", done.out.display()))
                            .color(OK)
                            .monospace(),
                    );
                    for n in &done.notes {
                        ui.label(RichText::new(format!("- {n}")).color(TEXT).monospace());
                    }
                    for w in &done.warnings {
                        ui.label(RichText::new(format!("! {w}")).color(VIOLET).monospace());
                    }
                }
                if let Some(err) = &self.error {
                    ui.add_space(12.0);
                    ui.label(RichText::new(err).color(ERR));
                }
                ui.add_space(16.0);
            });
    }
}

const REVEAL_LABEL: &str = if cfg!(target_os = "macos") {
    "Show in Finder"
} else if cfg!(windows) {
    "Show in Explorer"
} else {
    "Show in Folder"
};

fn show_in_file_manager(path: &Path) {
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer")
        .arg(format!("/select,{}", path.display().to_string().replace('/', "\\")))
        .spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg("-R").arg(path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open")
        .arg(path.parent().unwrap_or(Path::new(".")))
        .spawn();
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(12.0);
    ui.label(RichText::new(title).color(ACCENT).strong());
    ui.add_space(4.0);
}
