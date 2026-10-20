//! Appearance page: the theme gallery (ChickenWM + popular presets), light/dark, accent, icon
//! theme and per-app modes. Changes apply to the whole system right away, except the accent,
//! which applies when you press Apply (so dragging the color picker doesn't rewrite every
//! stylesheet on each frame).

use crate::kit::{self, c32, Kit};
use chickenwm_core::config::RgbaColor;
use chickenwm_core::icon_theme::{available_themes, icons_root, IconRequest, IconTheme, CATEGORIES};
use std::path::PathBuf;
use chickenwm_core::theme::{css, AppMode, Palette, ThemeConfig, ThemeMode, DEFAULT_ACCENT};
use chickenwm_core::theme_presets::PRESETS;
use eframe::egui::{self, CornerRadius, FontId, Rect, Sense, Stroke, Vec2};

pub struct ThemeEditor {
    pub draft: ThemeConfig,
    hex: String,
    icon_themes: Vec<String>,
    /// (icon theme name, textures) — reloaded when the theme or palette changes.
    icons: Option<(String, Vec<(String, egui::TextureHandle)>)>,
}

impl ThemeEditor {
    pub fn new(current: &ThemeConfig) -> Self {
        Self { hex: css(current.accent), draft: current.clone(), icon_themes: available_themes(), icons: None }
    }

    pub fn invalidate_icons(&mut self) {
        self.icons = None;
        self.icon_themes = available_themes();
    }

    /// Draws the theme groups. Returns `true` when the draft should be applied now.
    pub fn ui(&mut self, ui: &mut egui::Ui, kit: &Kit, applied: &ThemeConfig) -> bool {
        let mut apply = false;
        // Keep the draft in step with changes made elsewhere (per-app modes, other apps).
        if self.draft.app_modes != applied.app_modes {
            self.draft.app_modes = applied.app_modes.clone();
        }

        kit::group(ui, kit, "Theme", |g| {
            g.custom(|ui| {
                let avail_w = ui.available_width() - 12.0; // leave room for scrollbar
                let spacing = 12.0;
                let tile_w = ((avail_w - spacing * 2.0) / 3.0).floor().max(140.0);
                let tile_h = (tile_w * 0.68).round().max(105.0);
                let grid_h = tile_h * 3.0 + spacing * 2.0 + 8.0;

                egui::ScrollArea::vertical()
                    .min_scrolled_height(grid_h)
                    .max_height(grid_h)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        egui::Grid::new("theme_presets_grid")
                            .num_columns(3)
                            .spacing(Vec2::new(spacing, spacing))
                            .show(ui, |ui| {
                                let mode = self.draft.mode;
                                let mut col = 0;
                                let mut chosen = None;
                                let tile_size = Vec2::new(tile_w, tile_h);

                                let generated = Palette::new(self.draft.accent, mode);
                                if theme_tile(ui, kit, "ChickenWM", &generated, self.draft.preset.is_empty(), tile_size) {
                                    chosen = Some(String::new());
                                }
                                col += 1;
                                if col % 3 == 0 {
                                    ui.end_row();
                                }

                                for p in &PRESETS {
                                    let name = match mode {
                                        ThemeMode::Light => p.light_name,
                                        ThemeMode::Dark => p.dark_name,
                                    };
                                    let label = if name == p.name { p.name.to_string() } else { format!("{} · {name}", p.name) };
                                    if theme_tile(ui, kit, &label, &p.palette(mode), self.draft.preset == p.id, tile_size) {
                                        chosen = Some(p.id.to_string());
                                    }
                                    col += 1;
                                    if col % 3 == 0 {
                                        ui.end_row();
                                    }
                                }

                                if col % 3 != 0 {
                                    ui.end_row();
                                }

                                if let Some(p) = chosen {
                                    self.draft.preset = p;
                                    apply = true;
                                }
                            });
                    });
            });
            g.row("Appearance", "Light or dark for every app that follows the system.", |ui| {
                if kit::segmented(ui, kit, &mut self.draft.mode, &[(ThemeMode::Light, "☀ Light"), (ThemeMode::Dark, "🌙 Dark")]) {
                    apply = true;
                }
            });
            if self.draft.preset.is_empty() {
                g.row("Accent color", "The ChickenWM theme is generated from this one color.", |ui| {
                    let dirty = self.draft.accent != applied.accent;
                    if dirty && kit::button(ui, kit, "Apply", true).clicked() {
                        apply = true;
                    }
                    if ui.small_button("Reset").on_hover_text("#242321").clicked() {
                        self.draft.accent = DEFAULT_ACCENT;
                        self.hex = css(DEFAULT_ACCENT);
                    }
                    let resp = ui.add(egui::TextEdit::singleline(&mut self.hex).desired_width(72.0).font(egui::TextStyle::Monospace));
                    if resp.changed() {
                        if let Some(c) = RgbaColor::parse(self.hex.trim()) {
                            self.draft.accent = RgbaColor { a: 255, ..c };
                        }
                    }
                    let mut rgb = [self.draft.accent.r, self.draft.accent.g, self.draft.accent.b];
                    if ui.color_edit_button_srgb(&mut rgb).changed() {
                        self.draft.accent = RgbaColor { r: rgb[0], g: rgb[1], b: rgb[2], a: 255 };
                        self.hex = css(self.draft.accent);
                    }
                });
            }
            g.row("Icons", "Icons in the file explorer and launcher.", |ui| {
                let before = self.draft.icon_theme.clone();
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("icon_theme")
                        .selected_text(icon_theme_label(&self.draft.icon_theme))
                        .width(170.0)
                        .show_ui(ui, |ui| {
                            for t in self.icon_themes.clone() {
                                let label = icon_theme_label(&t);
                                ui.selectable_value(&mut self.draft.icon_theme, t, label);
                            }
                        });
                    if ui.button("Import…").on_hover_text("Import a Linux icon theme from a folder or .zip / .tar.gz archive").clicked() {
                        if let Some(path) = pick_theme_archive() {
                            if let Ok(imported_name) = chickenwm_core::icon_theme::import_theme_from_path(&path) {
                                self.draft.icon_theme = imported_name;
                                self.icon_themes = available_themes();
                                self.icons = None;
                                apply = true;
                            }
                        }
                    }
                    if ui.button("Folder").on_hover_text("Open ~/.config/chickenwm/icons in File Explorer").clicked() {
                        let _ = std::process::Command::new("explorer").arg(icons_root()).spawn();
                    }
                });
                if self.draft.icon_theme != before {
                    apply = true;
                }
            });
            // Icon preview (the built-in set is redrawn from the palette on apply).
            let theme_name = self.draft.icon_theme.clone();
            if self.icons.as_ref().map(|(n, _)| n != &theme_name).unwrap_or(true) {
                self.icons = Some((theme_name.clone(), load_icon_previews(g.ui.ctx(), &theme_name)));
            }
            if let Some((_, icons)) = &self.icons {
                if !icons.is_empty() {
                    g.custom(|ui| {
                        ui.horizontal_wrapped(|ui| {
                            for (name, tex) in icons {
                                ui.add(egui::Image::new(tex).fit_to_exact_size(Vec2::splat(28.0))).on_hover_text(name);
                            }
                        });
                    });
                }
            }
        });
        apply
    }
}

fn icon_theme_label(t: &str) -> String {
    match t {
        "system" => "Windows icons".to_string(),
        "chickenwm" => "ChickenWM (follows theme)".to_string(),
        "lucide" => "Lucide (macOS Finder style)".to_string(),
        "font-awesome" => "Font Awesome (Universal)".to_string(),
        "papirus" => "Papirus (Linux icon theme)".to_string(),
        _ => t.to_string(),
    }
}

fn pick_theme_archive() -> Option<PathBuf> {
    let script = r#"
Add-Type -AssemblyName System.Windows.Forms
$d = New-Object System.Windows.Forms.OpenFileDialog
$d.Title = "Select Icon Theme Archive (.zip, .tar.gz, .tgz, .tar) or Folder"
$d.Filter = "Icon Theme Archives (*.zip;*.tar.gz;*.tgz;*.tar)|*.zip;*.tar.gz;*.tgz;*.tar|All Files (*.*)|*.*"
if ($d.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
    Write-Output $d.FileName
}
"#;
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", script])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(PathBuf::from(s))
    }
}

/// One gallery tile: a tiny window mock-up in the theme's colors. Returns `true` when clicked.
fn theme_tile(ui: &mut egui::Ui, kit: &Kit, name: &str, p: &Palette, selected: bool, size: Vec2) -> bool {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter();
    let preview_h = (size.y - 24.0).max(40.0);
    let preview = Rect::from_min_size(rect.min, Vec2::new(size.x, preview_h));
    let t = ui.ctx().animate_bool_with_time(resp.id, selected || resp.hovered(), 0.12);
    // Selection ring.
    if selected {
        painter.rect_stroke(preview.expand(3.0), CornerRadius::same(10), Stroke::new(2.5_f32, kit.accent), egui::StrokeKind::Inside);
    } else if t > 0.0 {
        painter.rect_stroke(preview.expand(3.0), CornerRadius::same(10), Stroke::new(2.0_f32, kit.border.gamma_multiply(t * 2.0)), egui::StrokeKind::Inside);
    }
    let win = preview.shrink(if selected { 4.0 } else { 1.0 });
    painter.rect(win, CornerRadius::same(7), c32(p.base), Stroke::new(1.0_f32, kit.border), egui::StrokeKind::Inside);
    // Sidebar strip with a selected item.
    let side_w = (win.width() * 0.32).clamp(32.0, 60.0);
    let side = Rect::from_min_max(win.min, egui::pos2(win.left() + side_w, win.bottom()));
    painter.rect_filled(side, CornerRadius { nw: 7, sw: 7, ne: 0, se: 0 }, c32(p.mantle));
    // Border-right on preview sidebar:
    painter.vline(side.right(), side.y_range(), Stroke::new(1.0_f32, c32(p.border)));

    let line_count = 3;
    let line_spacing = ((side.height() - 20.0) / line_count as f32).clamp(8.0, 16.0);
    for i in 0..line_count {
        let y = side.top() + 10.0 + i as f32 * line_spacing;
        let line = Rect::from_min_size(egui::pos2(side.left() + 5.0, y), Vec2::new((side.width() - 10.0).max(4.0), 5.0));
        let fill = if i == 1 { c32(p.accent) } else { c32(p.fg).gamma_multiply(0.25) };
        painter.rect_filled(line, CornerRadius::same(2), fill);
    }
    // Content card.
    let card = Rect::from_min_max(egui::pos2(side.right() + 6.0, win.top() + 7.0), egui::pos2(win.right() - 6.0, win.bottom() - 7.0));
    painter.rect(card, CornerRadius::same(5), c32(p.surface), Stroke::new(1.0_f32, c32(p.border)), egui::StrokeKind::Inside);
    let text = Rect::from_min_size(card.min + Vec2::new(6.0, 6.0), Vec2::new((card.width() * 0.6).max(10.0), 5.0));
    painter.rect_filled(text, CornerRadius::same(2), c32(p.fg).gamma_multiply(0.85));
    painter.rect_filled(text.translate(Vec2::new(0.0, 8.0)).scale_from_center2(Vec2::new(0.75, 1.0)), CornerRadius::same(2), c32(p.subtext).gamma_multiply(0.7));
    // Highlight dots.
    let dots = [p.dir, p.exec, p.archive, p.media, p.danger];
    let dot_spacing = ((card.width() - 36.0) / dots.len() as f32).clamp(8.0, 14.0);
    for (i, c) in dots.into_iter().enumerate() {
        painter.circle_filled(egui::pos2(card.left() + 8.0 + i as f32 * dot_spacing, card.bottom() - 8.0), 3.0, c32(c));
    }
    // Accent button.
    let btn = Rect::from_min_size(egui::pos2(card.right() - 24.0, card.bottom() - 14.0), Vec2::new(18.0, 9.0));
    painter.rect_filled(btn, CornerRadius::same(4), c32(p.accent));
    painter.text(
        egui::pos2(rect.center().x, preview.bottom() + 13.0),
        egui::Align2::CENTER_CENTER,
        name,
        FontId::proportional(12.0),
        if selected { kit.fg } else { kit.sub },
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// Per-app light/dark rows. Returns the app whose choice the user changed.
pub fn per_app_ui(ui: &mut egui::Ui, kit: &Kit, theme: &ThemeConfig) -> Option<(&'static str, AppMode)> {
    use chickenwm_core::theme::app_mode_table;
    let mut changed = None;
    kit::group(ui, kit, "Per-app appearance", |g| {
        for (id, name, choice, effective) in app_mode_table(theme) {
            let hint = if choice == AppMode::System { format!("Follows the system ({})", effective.as_str()) } else { String::new() };
            g.row(name, &hint, |ui| {
                let mut sel = choice;
                if kit::segmented(ui, kit, &mut sel, &[(AppMode::System, "Auto"), (AppMode::Light, "Light"), (AppMode::Dark, "Dark")]) {
                    changed = Some((id, sel));
                }
            });
        }
    });
    changed
}

fn load_icon_previews(ctx: &egui::Context, theme: &str) -> Vec<(String, egui::TextureHandle)> {
    let t = IconTheme::load(theme);
    if t.is_system() {
        return Vec::new();
    }
    let mut requests: Vec<(&str, IconRequest)> = vec![
        ("folder", IconRequest::Folder),
        ("home", IconRequest::SpecialFolder("home".into())),
        ("downloads", IconRequest::SpecialFolder("downloads".into())),
        ("documents", IconRequest::SpecialFolder("documents".into())),
        ("pictures", IconRequest::SpecialFolder("pictures".into())),
        ("music", IconRequest::SpecialFolder("music".into())),
        ("videos", IconRequest::SpecialFolder("videos".into())),
        ("drive", IconRequest::Drive),
    ];
    for cat in CATEGORIES {
        requests.push((cat, IconRequest::File(cat.to_string())));
    }
    requests
        .into_iter()
        .filter_map(|(label, req)| {
            let path = t.resolve(&req)?;
            let (w, h, rgba) = chickenwm_core::icon_theme::load_icon_image(&path, 32)?;
            let tex = ctx.load_texture(
                format!("theme-icon-{theme}-{label}"),
                egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba),
                egui::TextureOptions::LINEAR,
            );
            Some((label.to_string(), tex))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drawing the page without any input must never apply a theme.
    #[test]
    fn idle_frames_do_not_apply() {
        let ctx = egui::Context::default();
        let applied = ThemeConfig { app_modes: vec![("files".into(), AppMode::Dark)], ..ThemeConfig::default() };
        let mut editor = ThemeEditor::new(&applied);
        let kit = Kit::from_css(&chickenwm_core::css::CssStyleSheet::default());
        for frame in 0..8 {
            let mut fired = false;
            let mut per_app = None;
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    fired = editor.ui(ui, &kit, &applied);
                    per_app = per_app_ui(ui, &kit, &applied);
                });
            });
            assert!(!fired, "theme applied on idle frame {frame}");
            assert!(per_app.is_none(), "per-app mode changed on idle frame {frame}");
        }
        assert_eq!(editor.draft, applied);
    }
}
