use crate::kit::{self, Kit};
use crate::{bool_str, SettingsApp};
use chickenwm_core::ipc::{send_ipc_command_blocking, IpcCommand, PIPE_NAME_DAEMON};
use chickenwm_core::monitor::{enumerate_monitors, max_native_refresh_rate};
use eframe::egui::{self, RichText};

impl SettingsApp {
    pub(crate) fn page_displays(&mut self, ui: &mut egui::Ui, kit: &Kit) {
        let mut refresh = false;
        kit::group(ui, kit, "Displays", |g| {
            for m in &self.monitors {
                let hint = if m.adapter_name.is_empty() {
                    String::new()
                } else {
                    m.adapter_name.clone()
                };
                g.row(&m.name, &hint, |ui| {
                    ui.label(
                        RichText::new(format!(
                            "{} × {} · {} Hz · {:.0}%",
                            m.width,
                            m.height,
                            m.refresh_rate_hz,
                            m.dpi_scale * 100.0
                        ))
                        .color(kit.sub),
                    );
                    if m.is_primary {
                        kit::chip(ui, kit, "Main", kit.accent);
                    }
                });
            }
            g.row("", "", |ui| {
                if kit::button(ui, kit, "Detect displays", false).clicked() {
                    refresh = true;
                }
            });
        });
        if refresh {
            self.monitors = enumerate_monitors();
        }

        let native_hz = max_native_refresh_rate(&self.monitors);
        let mut follow_native = self.cfg.misc.fps_limit == 0;
        let mut fps_changed = false;
        let mut vfr_changed = false;
        kit::group(ui, kit, "Frame rate", |g| {
            g.row(
                "Match the display",
                &format!("Animations run at {native_hz} Hz, in step with the screen."),
                |ui| {
                    if kit::toggle(ui, kit, &mut follow_native).changed() {
                        self.cfg.misc.fps_limit = if follow_native { 0 } else { native_hz };
                        fps_changed = true;
                    }
                },
            );
            if !follow_native {
                g.row("Frame rate limit", "", |ui| {
                    if ui
                        .add(egui::Slider::new(&mut self.cfg.misc.fps_limit, 30..=500).suffix(" FPS"))
                        .changed()
                    {
                        fps_changed = true;
                    }
                });
                g.custom(|ui| {
                    ui.horizontal(|ui| {
                        for preset in [60u32, 120, 144, 165, 240, 360] {
                            if kit::button(
                                ui,
                                kit,
                                &format!("{preset}"),
                                self.cfg.misc.fps_limit == preset,
                            )
                            .clicked()
                            {
                                self.cfg.misc.fps_limit = preset;
                                fps_changed = true;
                            }
                        }
                    });
                });
            }
            g.row(
                "Save power when idle",
                "Stops redrawing when nothing moves (variable frame rate).",
                |ui| {
                    if kit::toggle(ui, kit, &mut self.cfg.misc.vfr).changed() {
                        vfr_changed = true;
                    }
                },
            );
        });
        if vfr_changed {
            self.doc
                .set_category_key("misc", "vfr", bool_str(self.cfg.misc.vfr));
            self.sync_config_and_notify_labeled("Variable frame rate");
        }
        if fps_changed {
            self.doc.set_category_key(
                "misc",
                "fps_limit",
                &self.cfg.misc.fps_limit.to_string(),
            );
            self.sync_config_and_notify_labeled("Frame rate limit");
            let fps = (self.cfg.misc.fps_limit != 0).then_some(self.cfg.misc.fps_limit);
            let _ = send_ipc_command_blocking(PIPE_NAME_DAEMON, &IpcCommand::SetFramerate(fps));
        }

        let mut mode_changed = false;
        // The hook is an optional part of the install; without its files it can't be turned on.
        let hook_installed = std::env::current_exe()
            .map(|exe| {
                exe.with_file_name("chickenwm-dwm-hook.exe").exists()
                    && exe.with_file_name("chickenwm_dwm_hook.dll").exists()
            })
            .unwrap_or(false);
        kit::group(ui, kit, "Compositor", |g| {
            g.row(
                "Engine",
                "Standard is safe with anti-cheat games. The DWM hook injects into Windows' compositor (experimental).",
                |ui| {
                    let mut wanted = self.cfg.misc.dwm_hook_enabled;
                    // Not installed: only "Standard" can be picked (still allowed if the config
                    // has the hook on, so it can be switched off).
                    let can_switch = hook_installed || self.cfg.misc.dwm_hook_enabled;
                    let changed = ui
                        .add_enabled_ui(can_switch, |ui| {
                            kit::segmented(ui, kit, &mut wanted, &[(false, "Standard"), (true, "DWM hook")])
                        })
                        .inner;
                    if changed && wanted && !hook_installed {
                        // Can't turn on what isn't installed.
                    } else if changed {
                        if wanted {
                            // Only after the anti-cheat warning below is confirmed.
                            self.dwm_hook_confirm = true;
                        } else {
                            self.cfg.misc.dwm_hook_enabled = false;
                            mode_changed = true;
                        }
                    }
                },
            );
            if !hook_installed {
                g.custom(|ui| {
                    let text = if self.cfg.misc.dwm_hook_enabled {
                        "The DWM hook is on in your config but isn't installed, so it can't be used. Switch to Standard to clear it, or run the installer again and tick it."
                    } else {
                        "The DWM hook isn't installed. It is an optional part of the installer (off by default because it can trip game anti-cheat); run the installer again and tick it to add it."
                    };
                    ui.add(egui::Label::new(egui::RichText::new(text).size(12.0).color(kit.sub)).wrap());
                });
            }
            if self.cfg.misc.dwm_hook_enabled {
                g.custom(|ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(
                                "\u{26a0} The DWM hook is on. Games with anti-cheat (Easy Anti-Cheat, BattlEye, Vanguard, FACEIT, ...) may refuse to start or flag your account while it is. Switch back to Standard before playing online.",
                            )
                            .size(12.0)
                            .color(kit.danger),
                        )
                        .wrap(),
                    );
                });
            }
        });
        if self.dwm_hook_confirm {
            let ctx = ui.ctx().clone();
            let mut answer = None;
            egui::Modal::new(egui::Id::new("dwm-hook-confirm")).show(&ctx, |ui| {
                ui.set_width(420.0);
                ui.label(egui::RichText::new("\u{26a0} Turn on the DWM hook?").size(16.0).color(kit.danger));
                ui.add_space(6.0);
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(
                            "The DWM hook injects code into dwm.exe, the Windows compositor. Anti-cheat software (Easy Anti-Cheat, BattlEye, Riot Vanguard, FACEIT and others) can treat that as tampering: games may refuse to start, and an account could be flagged or banned.\n\nIt is experimental and can also make the desktop unstable; if DWM crashes, the screen goes black for a moment while Windows restarts it.\n\nKeep it off if you play online games. You can switch back to Standard here at any time, or press Ctrl + Alt + Shift + Esc for the emergency restore.",
                        )
                        .size(12.0)
                        .color(kit.sub),
                    )
                    .wrap(),
                );
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if kit::button(ui, kit, "Keep Standard", true).clicked() {
                        answer = Some(false);
                    }
                    if kit::button(ui, kit, "Enable anyway", false).clicked() {
                        answer = Some(true);
                    }
                });
            });
            match answer {
                Some(true) => {
                    self.dwm_hook_confirm = false;
                    self.cfg.misc.dwm_hook_enabled = true;
                    mode_changed = true;
                }
                Some(false) => self.dwm_hook_confirm = false,
                None => {}
            }
        }
        if mode_changed {
            self.doc.set_category_key(
                "misc",
                "dwm_hook_enabled",
                bool_str(self.cfg.misc.dwm_hook_enabled),
            );
            self.sync_config_and_notify_labeled("Compositor engine");
            let _ = send_ipc_command_blocking(
                PIPE_NAME_DAEMON,
                &IpcCommand::SetCompositorMode {
                    dwm_hook_enabled: self.cfg.misc.dwm_hook_enabled,
                },
            );
            self.status_banner =
                "Compositor engine changed: ChickenWM modules restarted".to_string();
        }
    }
}
