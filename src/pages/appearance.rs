use crate::kit::{self, Kit};
use crate::{bool_str, theme_ui, SettingsApp};
use chickenwm_core::config::{config_dir, RgbaColor};
use chickenwm_core::ipc::{send_ipc_command_blocking, IpcCommand, PIPE_NAME_DAEMON};
use eframe::egui::{self, Color32, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};

impl SettingsApp {
    pub(crate) fn theme_tools_ui(&mut self, ui: &mut egui::Ui, kit: &Kit) {
        use chickenwm_core::theme_tools;
        let mut auto_changed = false;
        kit::group(ui, kit, "Light and dark by the clock", |g| {
            g.row(
                "Switch automatically",
                "Dark from evening to morning: at set times, or from sunset to sunrise.",
                |ui| {
                    let mut mode = self.auto_dark.mode.clone();
                    let options = [
                        ("off".to_string(), "Off"),
                        ("times".to_string(), "Set times"),
                        ("sun".to_string(), "Sunset"),
                    ];
                    let opts: Vec<(&str, &str)> =
                        options.iter().map(|(v, l)| (v.as_str(), *l)).collect();
                    let mut cur = opts.iter().position(|(v, _)| *v == mode).unwrap_or(0);
                    let indices: Vec<(usize, &str)> =
                        opts.iter().enumerate().map(|(i, (_, l))| (i, *l)).collect();
                    if kit::segmented(ui, kit, &mut cur, &indices) {
                        mode = opts[cur].0.to_string();
                        self.auto_dark.mode = mode;
                        auto_changed = true;
                    }
                },
            );
            if self.auto_dark.mode == "times" {
                for (label, value) in [
                    ("Light from", &mut self.auto_dark.light_at),
                    ("Dark from", &mut self.auto_dark.dark_at),
                ] {
                    g.row(label, "24-hour time, like 07:00.", |ui| {
                        let ok = theme_tools::parse_hm(value).is_some();
                        let r = ui.add(
                            egui::TextEdit::singleline(value)
                                .desired_width(70.0)
                                .text_color_opt((!ok).then_some(Color32::from_rgb(220, 80, 70))),
                        );
                        if r.lost_focus() && theme_tools::parse_hm(value).is_some() {
                            auto_changed = true;
                        }
                    });
                }
            }
            if self.auto_dark.mode == "sun" {
                // If coordinates or timezone not set, auto-detect local system timezone:
                if self.auto_dark.latitude.is_none() || self.auto_dark.longitude.is_none() {
                    if let Some(sys_tz) = chickenwm_core::timezones::local_system_timezone() {
                        self.auto_dark.latitude = Some(sys_tz.lat);
                        self.auto_dark.longitude = Some(sys_tz.lon);
                        self.auto_dark.timezone = Some(sys_tz.id.to_string());
                        auto_changed = true;
                    }
                }

                let current_tz = self.auto_dark.timezone.as_deref()
                    .and_then(chickenwm_core::timezones::find_timezone_by_id)
                    .or_else(|| {
                        match (self.auto_dark.latitude, self.auto_dark.longitude) {
                            (Some(lat), Some(lon)) => chickenwm_core::timezones::find_timezone_by_coords(lat, lon),
                            _ => None,
                        }
                    });

                let (y, m, d, _, off) = theme_tools::local_clock();
                let hint = match (self.auto_dark.latitude, self.auto_dark.longitude) {
                    (Some(lat), Some(lon)) => match theme_tools::sun_times(lat, lon, y, m, d, off) {
                        Some((rise, set)) => format!(
                            "Today: light at {}, dark at {}.",
                            theme_tools::fmt_hm(rise),
                            theme_tools::fmt_hm(set)
                        ),
                        None => "No sunrise or sunset there today.".to_string(),
                    },
                    _ => "Select your time zone to calculate sunrise and sunset.".to_string(),
                };

                let current_display = current_tz.map(|tz| tz.display).unwrap_or("Select time zone…");

                g.row("Time zone", &hint, |ui| {
                    let btn_w = 175.0;
                    let btn_h = 26.0;
                    let (btn_rect, btn_resp) = ui.allocate_exact_size(Vec2::new(btn_w, btn_h), Sense::click());

                    let is_hovered = btn_resp.hovered() || self.tz_dropdown_open;
                    let bg_color = if self.tz_dropdown_open {
                        kit.card.lerp_to_gamma(kit.fg, 0.12)
                    } else if is_hovered {
                        kit.fg.gamma_multiply(if kit.light { 0.08 } else { 0.12 })
                    } else {
                        kit.fg.gamma_multiply(if kit.light { 0.04 } else { 0.07 })
                    };

                    ui.painter().rect_filled(btn_rect, CornerRadius::same(7), bg_color);
                    ui.painter().rect_stroke(
                        btn_rect,
                        CornerRadius::same(7),
                        Stroke::new(1.0_f32, if is_hovered { kit.accent.gamma_multiply(0.6) } else { kit.border }),
                        egui::StrokeKind::Inside,
                    );

                    // Truncate display string if needed to fit within button
                    let text_area = Rect::from_min_max(
                        btn_rect.min + Vec2::new(8.0, 0.0),
                        btn_rect.max - Vec2::new(22.0, 0.0),
                    );
                    let font = FontId::proportional(12.0);
                    let text_layout = ui.painter().layout(
                        current_display.to_string(),
                        font.clone(),
                        kit.fg,
                        text_area.width(),
                    );
                    ui.painter().with_clip_rect(text_area).galley(
                        Pos2::new(text_area.left(), btn_rect.center().y - text_layout.size().y / 2.0),
                        text_layout,
                        Color32::WHITE,
                    );

                    // Chevron icon on right
                    let chev_center = Pos2::new(btn_rect.right() - 11.0, btn_rect.center().y);
                    let chev_col = if is_hovered { kit.accent } else { kit.sub };
                    ui.painter().line_segment(
                        [chev_center + Vec2::new(-3.5, -2.0), chev_center + Vec2::new(0.0, 2.0)],
                        Stroke::new(1.5_f32, chev_col),
                    );
                    ui.painter().line_segment(
                        [chev_center + Vec2::new(0.0, 2.0), chev_center + Vec2::new(3.5, -2.0)],
                        Stroke::new(1.5_f32, chev_col),
                    );

                    if btn_resp.clicked() {
                        self.tz_dropdown_open = !self.tz_dropdown_open;
                    }

                    if self.tz_dropdown_open {
                        let popup_id = ui.id().with("tz_dropdown_popup_area");
                        let popup_pos = btn_rect.left_bottom() + Vec2::new(0.0, 4.0);
                        let popup_width = 195.0;
                        let popup_max_h = 140.0;

                        let mut picked_tz = None;
                        let mut close_dropdown = false;

                        let area_resp = egui::Area::new(popup_id)
                            .order(egui::Order::Foreground)
                            .fixed_pos(popup_pos)
                            .constrain(true)
                            .show(ui.ctx(), |ui| {
                                egui::Frame::NONE
                                    .fill(kit.card)
                                    .stroke(Stroke::new(1.0_f32, kit.border))
                                    .corner_radius(CornerRadius::same(8))
                                    .shadow(egui::epaint::Shadow {
                                        offset: [0, 6],
                                        blur: 16,
                                        spread: 0,
                                        color: Color32::from_black_alpha(100),
                                    })
                                    .inner_margin(egui::Margin::symmetric(6, 6))
                                    .show(ui, |ui| {
                                        ui.set_width(popup_width - 12.0);

                                        // Search box
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new("🔍").size(10.5).color(kit.sub));
                                            let clear_w = if self.tz_search.is_empty() { 0.0 } else { 18.0 };
                                            ui.add(
                                                egui::TextEdit::singleline(&mut self.tz_search)
                                                    .hint_text("Search…")
                                                    .desired_width(ui.available_width() - clear_w),
                                            );
                                            if !self.tz_search.is_empty() && ui.small_button("✕").clicked() {
                                                self.tz_search.clear();
                                            }
                                        });

                                        ui.add_space(2.0);
                                        ui.painter().hline(
                                            ui.available_rect_before_wrap().x_range(),
                                            ui.cursor().top(),
                                            Stroke::new(1.0_f32, kit.hairline()),
                                        );
                                        ui.add_space(3.0);

                                        let filtered = chickenwm_core::timezones::search_timezones(&self.tz_search);
                                        if filtered.is_empty() {
                                            ui.add_space(4.0);
                                            ui.label(RichText::new("No time zones found").size(11.0).color(kit.sub));
                                            ui.add_space(4.0);
                                        } else {
                                            egui::ScrollArea::vertical()
                                                .max_height(popup_max_h)
                                                .auto_shrink([false, false])
                                                .show(ui, |ui| {
                                                    for tz in filtered {
                                                        let is_selected = current_tz.map_or(false, |c| c.id == tz.id);
                                                        let (item_r, item_resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 20.0), Sense::click());
                                                        if is_selected {
                                                            ui.painter().rect_filled(item_r, CornerRadius::same(4), kit.accent.gamma_multiply(0.20));
                                                        } else if item_resp.hovered() {
                                                            ui.painter().rect_filled(item_r, CornerRadius::same(4), kit.fg.gamma_multiply(0.06));
                                                        }

                                                        let text_col = if is_selected { kit.accent } else { kit.fg };
                                                        let text_clip = Rect::from_min_max(item_r.min + Vec2::new(4.0, 0.0), item_r.max - Vec2::new(4.0, 0.0));
                                                        let layout = ui.painter().layout_no_wrap(tz.display.to_string(), FontId::proportional(11.0), text_col);
                                                        ui.painter().with_clip_rect(text_clip).galley(
                                                            Pos2::new(text_clip.left(), item_r.center().y - layout.size().y / 2.0),
                                                            layout,
                                                            Color32::WHITE,
                                                        );

                                                        if item_resp.clicked() {
                                                            picked_tz = Some(tz);
                                                        }
                                                    }
                                                });
                                        }
                                    })
                            });

                        let popup_rect = area_resp.response.rect;
                        let pointer_pos = ui.input(|i| i.pointer.hover_pos());

                        if let Some(pos) = pointer_pos {
                            // Expand hit area slightly so moving mouse smoothly between button and popup doesn't close it
                            let in_trigger = btn_rect.expand(4.0).contains(pos);
                            let in_popup = popup_rect.expand(6.0).contains(pos);
                            if !in_trigger && !in_popup {
                                close_dropdown = true;
                            }
                        }

                        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            close_dropdown = true;
                        }

                        if let Some(tz) = picked_tz {
                            self.auto_dark.timezone = Some(tz.id.to_string());
                            self.auto_dark.latitude = Some(tz.lat);
                            self.auto_dark.longitude = Some(tz.lon);
                            auto_changed = true;
                            close_dropdown = true;
                        }

                        if close_dropdown {
                            self.tz_dropdown_open = false;
                        }
                    }
                });
            }
        });
        if auto_changed {
            theme_tools::write_auto_dark_keys(&mut self.doc, &self.auto_dark);
            self.cfg.theme.auto_dark = self.auto_dark.clone();
            self.theme_editor.draft.auto_dark = self.auto_dark.clone();
            let (y, m, d, minute, off) = theme_tools::local_clock();
            match theme_tools::desired_mode(&self.auto_dark, y, m, d, minute, off) {
                Some(want) if want != self.cfg.theme.mode => {
                    self.theme_editor.draft.mode = want;
                    self.apply_system_theme();
                }
                _ => self.sync_config_and_notify_labeled("Auto-dark schedule"),
            }
        }

        let mut wallpaper_accent = false;
        let mut export = false;
        let mut import = false;
        kit::group(ui, kit, "More", |g| {
            g.row(
                "Accent from wallpaper",
                "Picks the most prominent colour of the current wallpaper image.",
                |ui| {
                    wallpaper_accent = kit::button(ui, kit, "Use wallpaper colour", false).clicked();
                },
            );
            g.row(
                "Theme file",
                "Share a theme as one .chickentheme file, or load someone else's.",
                |ui| {
                    import = kit::button(ui, kit, "Import", false).clicked();
                    export = kit::button(ui, kit, "Export", false).clicked();
                    ui.add(egui::TextEdit::singleline(&mut self.theme_file_path).desired_width(220.0));
                },
            );
        });
        if wallpaper_accent {
            let path = self
                .cfg
                .wallpapers
                .first()
                .map(|w| chickenwm_core::config::expand_home(&w.path));
            match path.as_deref().and_then(theme_tools::accent_from_image) {
                Some(c) => {
                    self.theme_editor.draft.accent = c;
                    self.theme_editor.draft.preset.clear();
                    self.apply_system_theme();
                }
                None => {
                    self.status_banner =
                        "No wallpaper image, or it has no clear colour".to_string()
                }
            }
        }
        let path = std::path::PathBuf::from(self.theme_file_path.trim().trim_matches('"'));
        if export {
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "My theme".into());
            self.status_banner =
                match std::fs::write(&path, theme_tools::export_theme(&self.cfg.theme, &name)) {
                    Ok(()) => format!("Saved {}", path.display()),
                    Err(e) => format!("Couldn't save: {e}"),
                };
        }
        if import {
            match std::fs::read_to_string(&path)
                .map_err(anyhow::Error::from)
                .and_then(|t| theme_tools::import_theme(&t, &self.cfg.theme))
            {
                Ok((name, theme)) => {
                    self.theme_editor.draft = theme.clone();
                    self.auto_dark = theme.auto_dark.clone();
                    theme_tools::write_auto_dark_keys(&mut self.doc, &self.auto_dark);
                    self.apply_system_theme();
                    self.status_banner = format!("Theme \"{name}\" applied");
                }
                Err(e) => self.status_banner = format!("Couldn't import: {e}"),
            }
        }
    }

    pub(crate) fn page_appearance(&mut self, ui: &mut egui::Ui, kit: &Kit) {
        let applied = self.cfg.theme.clone();
        if self.theme_editor.ui(ui, kit, &applied) {
            self.apply_system_theme();
        }
        if let Some((app, mode)) = theme_ui::per_app_ui(ui, kit, &self.cfg.theme) {
            match chickenwm_core::theme::set_app_mode(app, mode) {
                Ok(theme) => {
                    self.doc
                        .set_category_key("theme", "app_modes", &theme.app_modes_string());
                    self.cfg.theme.app_modes = theme.app_modes;
                    self.theme_editor.draft.app_modes = self.cfg.theme.app_modes.clone();
                    self.status_banner = format!("{app}: {}", mode.as_str());
                }
                Err(e) => self.status_banner = format!("Couldn't change {app}: {e}"),
            }
        }
        self.theme_tools_ui(ui, kit);

        let mut changed = false;
        let mut border_changed = false;
        kit::group(ui, kit, "Windows", |g| {
            g.row(
                "Active border",
                "Two colors make a gradient at the given angle.",
                |ui| {
                    let b = &mut self.cfg.general.col_active_border;
                    if ui
                        .add(
                            egui::DragValue::new(&mut b.angle_deg)
                                .range(0.0..=360.0)
                                .suffix("°")
                                .speed(1.0),
                        )
                        .changed()
                    {
                        border_changed = true;
                    }
                    let sec = b.secondary.unwrap_or(b.primary);
                    let mut c2 = [sec.r, sec.g, sec.b, sec.a];
                    if ui
                        .color_edit_button_srgba_unmultiplied(&mut c2)
                        .on_hover_text("Second color")
                        .changed()
                    {
                        b.secondary = Some(RgbaColor {
                            r: c2[0],
                            g: c2[1],
                            b: c2[2],
                            a: c2[3],
                        });
                        border_changed = true;
                    }
                    let mut c1 = [b.primary.r, b.primary.g, b.primary.b, b.primary.a];
                    if ui
                        .color_edit_button_srgba_unmultiplied(&mut c1)
                        .on_hover_text("First color")
                        .changed()
                    {
                        b.primary = RgbaColor {
                            r: c1[0],
                            g: c1[1],
                            b: c1[2],
                            a: c1[3],
                        };
                        border_changed = true;
                    }
                },
            );
            g.row(
                "Border rotation speed",
                "0 keeps the gradient still.",
                |ui| {
                    if ui
                        .add(
                            egui::Slider::new(&mut self.cfg.decoration.border_anim_speed, 0.0..=5.0)
                                .max_decimals(2),
                        )
                        .changed()
                    {
                        self.doc.set_category_key(
                            "decoration",
                            "border_anim_speed",
                            &format!("{:.2}", self.cfg.decoration.border_anim_speed),
                        );
                        changed = true;
                    }
                },
            );
            g.row("Corner rounding", "", |ui| {
                if ui
                    .add(egui::Slider::new(&mut self.cfg.decoration.rounding, 0..=28).suffix(" px"))
                    .changed()
                {
                    self.doc.set_category_key(
                        "decoration",
                        "rounding",
                        &self.cfg.decoration.rounding.to_string(),
                    );
                    changed = true;
                }
            });
            g.row("Focused window opacity", "", |ui| {
                if ui
                    .add(
                        egui::Slider::new(&mut self.cfg.decoration.active_opacity, 0.3..=1.0)
                            .max_decimals(2),
                    )
                    .changed()
                {
                    self.doc.set_category_key(
                        "decoration",
                        "active_opacity",
                        &format!("{:.2}", self.cfg.decoration.active_opacity),
                    );
                    changed = true;
                }
            });
            g.row("Other windows' opacity", "", |ui| {
                if ui
                    .add(
                        egui::Slider::new(&mut self.cfg.decoration.inactive_opacity, 0.3..=1.0)
                            .max_decimals(2),
                    )
                    .changed()
                {
                    self.doc.set_category_key(
                        "decoration",
                        "inactive_opacity",
                        &format!("{:.2}", self.cfg.decoration.inactive_opacity),
                    );
                    changed = true;
                }
            });
            g.row(
                "Backdrop blur",
                "Acrylic / Mica behind supported windows.",
                |ui| {
                    if kit::toggle(ui, kit, &mut self.cfg.decoration.blur_enabled).changed() {
                        self.doc.set_category_key(
                            "decoration",
                            "blur_enabled",
                            bool_str(self.cfg.decoration.blur_enabled),
                        );
                        changed = true;
                    }
                },
            );
        });
        if border_changed {
            let spec = self.cfg.general.col_active_border.to_hypr_string();
            self.doc
                .set_category_key("general", "col.active_border", &spec);
            changed = true;
        }
        if changed {
            self.sync_config_and_notify_labeled("Window appearance");
        }

        let mut bar_config_changed = false;
        kit::group(ui, kit, "Bar", |g| {
            g.row(
                "Floating bar",
                "Floating bar with margins and rounded corners. Off: docked to monitor edge, full width, 0px gap and corners, islands disabled.",
                |ui| {
                    if kit::toggle(ui, kit, &mut self.bar_float).clicked() {
                        bar_config_changed = true;
                        if !self.bar_float {
                            self.bar_islands = false;
                        }
                    }
                },
            );
            if self.bar_float {
                g.row(
                    "Island separation",
                    "Each group of widgets in its own pill. Off: one background and border for the whole bar, same layout.",
                    |ui| {
                        if kit::toggle(ui, kit, &mut self.bar_islands).clicked() {
                            bar_config_changed = true;
                        }
                    },
                );
            }
        });
        if bar_config_changed {
            let path = config_dir().join("roobar.css");
            let source = std::fs::read_to_string(&path).unwrap_or_default();
            let float_val = if self.bar_float { "true" } else { "false" };
            let islands_val = if self.bar_islands { "true" } else { "false" };
            let updated = chickenwm_core::theme::set_root_vars(&source, &[
                ("--float", float_val.to_owned()),
                ("--islands", islands_val.to_owned()),
            ]);
            if let Err(e) = std::fs::write(&path, updated) {
                tracing::warn!("settings: couldn't write roobar.css: {e}");
            }
        }

        let mut save_css = false;
        let mut load_css: Option<&'static str> = None;
        kit::group(ui, kit, "Advanced", |g| {
            g.row(
                "Edit stylesheets",
                "Fonts, sizes and colors of each app, in CSS. Changes show up live.",
                |ui| {
                    kit::toggle(ui, kit, &mut self.show_css_editor);
                },
            );
            if self.show_css_editor {
                g.custom(|ui| {
                    ui.horizontal(|ui| {
                        let files: [(&str, &str); 6] = [
                            ("roobar.css", "Bar"),
                            ("launcher.css", "Launcher"),
                            ("files.css", "Files"),
                            ("settings.css", "Settings"),
                            ("browser.css", "Browser"),
                            ("notifications.css", "Notifications"),
                        ];
                        let mut sel = files
                            .iter()
                            .position(|f| f.0 == self.selected_css_file)
                            .unwrap_or(0);
                        let opts: Vec<(usize, &str)> =
                            files.iter().enumerate().map(|(i, f)| (i, f.1)).collect();
                        if kit::segmented(ui, kit, &mut sel, &opts) {
                            load_css = Some(files[sel].0);
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if kit::button(ui, kit, "Save", true).clicked() {
                                save_css = true;
                            }
                        });
                    });
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                        ui.add_sized(
                            [ui.available_width(), 280.0],
                            egui::TextEdit::multiline(&mut self.css_editor_buffer)
                                .code_editor()
                                .desired_rows(14),
                        );
                    });
                });
            }
        });
        if let Some(f) = load_css {
            self.selected_css_file = f.to_string();
            self.css_editor_buffer =
                std::fs::read_to_string(config_dir().join(f)).unwrap_or_default();
        }
        if save_css {
            let path = config_dir().join(&self.selected_css_file);
            match std::fs::write(&path, &self.css_editor_buffer) {
                Ok(()) => {
                    let _ = send_ipc_command_blocking(PIPE_NAME_DAEMON, &IpcCommand::ReloadConfig);
                    self.status_banner = format!("Saved {}", self.selected_css_file);
                }
                Err(e) => {
                    self.status_banner = format!("Couldn't save {}: {e}", self.selected_css_file)
                }
            }
        }
    }
}
