use std::path::Path;
use std::time::Duration;

use chrono::{Local, Timelike};
use eframe::egui::{
    self, Align, Color32, FontId, Layout, RichText, Sense, Stroke, Vec2, ViewportCommand,
    WindowLevel,
};
use uuid::Uuid;

use crate::audio::{self, SoundPlayer};
use crate::model::{self, Alarm, AlarmStore, Repeat};
use crate::storage;

pub struct AlarumApp {
    store: AlarmStore,
    editor: Option<Alarm>,
    editor_is_new: bool,
    ringing_id: Option<Uuid>,
    player: Option<SoundPlayer>,
    status: String,
}

impl AlarumApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let mut store = storage::load();
        if store.default_snooze_minutes == 0 {
            store.default_snooze_minutes = 5;
        }
        Self {
            store,
            editor: None,
            editor_is_new: false,
            ringing_id: None,
            player: None,
            status: "Alarms are checked while this window is running.".to_string(),
        }
    }

    fn persist(&self) {
        storage::save(&self.store);
    }

    fn add_alarm(&mut self) {
        let now = Local::now();
        let mut alarm = Alarm::new(now.hour(), now.minute());
        alarm.snooze_minutes = self.store.default_snooze_minutes.max(1);
        self.editor_is_new = true;
        self.editor = Some(alarm);
    }

    fn start_ringing(&mut self, id: Uuid) {
        if self.ringing_id == Some(id) {
            return;
        }
        if let Some(alarm) = self.store.alarms.iter_mut().find(|a| a.id == id) {
            alarm.last_fired = Some(Local::now());
            alarm.snooze_until = None;
            let label = alarm.display_name();
            let sound = if alarm.sound_path.trim().is_empty() {
                None
            } else {
                Some(alarm.sound_path.clone())
            };
            self.player = Some(SoundPlayer::play(sound.as_deref().map(Path::new)));
            audio::notify("Alarm", &label);
            self.status = format!("Ringing: {label}");
            self.ringing_id = Some(id);
            self.persist();
        }
    }

    fn dismiss(&mut self) {
        if let Some(id) = self.ringing_id.take() {
            if let Some(alarm) = self.store.alarms.iter_mut().find(|a| a.id == id) {
                alarm.snooze_until = None;
                alarm.last_fired = Some(Local::now());
                if alarm.repeat == Repeat::Once {
                    alarm.enabled = false;
                }
            }
        }
        if let Some(mut player) = self.player.take() {
            player.stop();
        }
        self.status = "Alarm dismissed.".to_string();
        self.persist();
    }

    fn snooze(&mut self) {
        if let Some(id) = self.ringing_id.take() {
            if let Some(alarm) = self.store.alarms.iter_mut().find(|a| a.id == id) {
                let minutes = alarm.snooze_minutes.max(1) as i64;
                alarm.snooze_until = Some(Local::now() + chrono::Duration::minutes(minutes));
                alarm.last_fired = Some(Local::now());
                alarm.enabled = true;
                self.status = format!("Snoozed for {minutes} minutes.");
            }
        }
        if let Some(mut player) = self.player.take() {
            player.stop();
        }
        self.persist();
    }

    fn poll_alarms(&mut self) {
        if self.ringing_id.is_some() {
            return;
        }
        let now = Local::now();
        let due: Vec<Uuid> = self
            .store
            .alarms
            .iter()
            .filter(|a| a.should_fire(now))
            .map(|a| a.id)
            .collect();
        if let Some(id) = due.into_iter().next() {
            self.start_ringing(id);
        }
    }
}

impl eframe::App for AlarumApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(200));
        self.poll_alarms();

        let accent = Color32::from_rgb(232, 168, 56);
        let panel = Color32::from_rgb(28, 30, 36);
        let card = Color32::from_rgb(40, 43, 52);
        let muted = Color32::from_rgb(160, 164, 176);

        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = panel;
        visuals.window_fill = Color32::from_rgb(34, 36, 44);
        visuals.override_text_color = Some(Color32::from_rgb(236, 238, 244));
        visuals.widgets.inactive.bg_fill = Color32::from_rgb(52, 56, 68);
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(64, 70, 86);
        visuals.selection.bg_fill = accent.linear_multiply(0.45);
        ctx.set_visuals(visuals);

        if self.ringing_id.is_some() {
            ctx.send_viewport_cmd(ViewportCommand::Focus);
            ctx.send_viewport_cmd(ViewportCommand::WindowLevel(WindowLevel::AlwaysOnTop));
        } else {
            ctx.send_viewport_cmd(ViewportCommand::WindowLevel(WindowLevel::Normal));
        }

        egui::TopBottomPanel::top("header")
            .exact_height(64.0)
            .show(ctx, |ui| {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Alarum").size(22.0).color(accent).strong());
                        ui.label(RichText::new("Keep this window open so alarms can ring.").size(12.0).color(muted));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(12.0);
                        let clock = model::format_datetime(Local::now(), self.store.use_12_hour);
                        ui.label(RichText::new(clock).font(FontId::monospace(20.0)).color(Color32::WHITE));
                    });
                });
            });

        egui::TopBottomPanel::bottom("footer")
            .exact_height(36.0)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.label(RichText::new(&self.status).size(13.0).color(muted));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(12.0);
                        ui.label(
                            RichText::new(format!("{} alarm(s)", self.store.alarms.len()))
                                .size(13.0)
                                .color(muted),
                        );
                    });
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(id) = self.ringing_id {
                self.ringing_ui(ui, id, accent);
                return;
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Button::new(RichText::new("  +  Add alarm  ").size(16.0).color(Color32::BLACK)).fill(accent))
                    .clicked()
                {
                    self.add_alarm();
                }
                ui.add_space(16.0);
                ui.label("Default snooze:");
                let mut snooze = self.store.default_snooze_minutes.max(1);
                if ui.add(egui::DragValue::new(&mut snooze).suffix(" min").clamp_range(1..=60)).changed() {
                    self.store.default_snooze_minutes = snooze;
                    self.persist();
                }
                ui.add_space(16.0);
                ui.label("Time format:");
                let mut twelve = self.store.use_12_hour;
                if ui.selectable_label(!twelve, "24-hour").clicked() {
                    twelve = false;
                }
                if ui.selectable_label(twelve, "12-hour").clicked() {
                    twelve = true;
                }
                if twelve != self.store.use_12_hour {
                    self.store.use_12_hour = twelve;
                    self.persist();
                }
            });
            ui.add_space(10.0);

            if self.store.alarms.is_empty() {
                ui.add_space(48.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("No alarms yet").size(20.0).color(muted));
                    ui.label(RichText::new("Add one and leave Alarum running.").size(14.0).color(muted));
                });
                return;
            }

            let mut edit_id = None;
            let mut delete_id = None;
            let mut persist = false;

            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for alarm in &mut self.store.alarms {
                    let use_12 = self.store.use_12_hour;
                    let next = alarm
                        .next_trigger(Local::now())
                        .map(|t| model::format_next(t, use_12))
                        .unwrap_or_else(|| "—".to_string());

                    let (rect, _resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 78.0), Sense::hover());
                    ui.painter().rect_filled(rect, 8.0, card);
                    ui.painter().rect_stroke(rect, 8.0, Stroke::new(1.0_f32, Color32::from_rgb(58, 62, 74)));

                    let mut child = ui.child_ui(rect.shrink2(Vec2::new(12.0, 8.0)), *ui.layout());
                    child.horizontal(|ui| {
                        if ui.add(egui::Checkbox::without_text(&mut alarm.enabled)).changed() {
                            persist = true;
                        }
                        ui.add_space(8.0);
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(alarm.time_label_for(use_12))
                                        .font(FontId::monospace(28.0))
                                        .color(if alarm.enabled { Color32::WHITE } else { muted })
                                        .strong(),
                                );
                                ui.add_space(10.0);
                                ui.vertical(|ui| {
                                    ui.add_space(4.0);
                                    ui.label(
                                        RichText::new(alarm.display_name())
                                            .size(16.0)
                                            .color(if alarm.enabled { Color32::from_rgb(230, 232, 240) } else { muted }),
                                    );
                                    ui.label(
                                        RichText::new(format!("{}  ·  next {}", alarm.repeat.label(), next))
                                            .size(12.0)
                                            .color(muted),
                                    );
                                });
                            });
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.button("Delete").clicked() {
                                delete_id = Some(alarm.id);
                            }
                            if ui.button("Edit").clicked() {
                                edit_id = Some(alarm.id);
                            }
                        });
                    });
                    ui.add_space(8.0);
                }
            });

            if persist {
                self.persist();
            }
            if let Some(id) = edit_id {
                if let Some(alarm) = self.store.alarms.iter().find(|a| a.id == id).cloned() {
                    self.editor_is_new = false;
                    self.editor = Some(alarm);
                }
            }
            if let Some(id) = delete_id {
                self.store.alarms.retain(|a| a.id != id);
                if self.ringing_id == Some(id) {
                    self.dismiss();
                }
                self.persist();
            }
        });

        self.editor_window(ctx, accent);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Some(mut player) = self.player.take() {
            player.stop();
        }
        self.persist();
    }
}

impl AlarumApp {
    fn ringing_ui(&mut self, ui: &mut egui::Ui, id: Uuid, accent: Color32) {
        let alarm = self.store.alarms.iter().find(|a| a.id == id);
        let title = alarm.map(|a| a.display_name()).unwrap_or_else(|| "Alarm".to_string());
        let time = alarm
            .map(|a| a.time_label_for(self.store.use_12_hour))
            .unwrap_or_default();
        let snooze_min = alarm.map(|a| a.snooze_minutes.max(1)).unwrap_or(5);

        ui.vertical_centered(|ui| {
            ui.add_space(36.0);
            ui.label(RichText::new("ALARM").size(14.0).color(accent).strong());
            ui.add_space(8.0);
            ui.label(RichText::new(time).font(FontId::monospace(64.0)).color(Color32::WHITE).strong());
            ui.add_space(12.0);
            ui.label(RichText::new(&title).size(28.0).color(Color32::from_rgb(240, 242, 248)));
            ui.add_space(28.0);
            ui.horizontal(|ui| {
                // Center the two buttons by using a dummy layout.
                ui.add_space((ui.available_width() - 360.0).max(0.0) / 2.0);
                if ui
                    .add_sized(
                        [170.0, 48.0],
                        egui::Button::new(RichText::new(format!("Snooze  {snooze_min} min")).size(18.0).color(Color32::BLACK))
                            .fill(accent),
                    )
                    .clicked()
                {
                    self.snooze();
                }
                ui.add_space(16.0);
                if ui
                    .add_sized([170.0, 48.0], egui::Button::new(RichText::new("Dismiss").size(18.0)))
                    .clicked()
                {
                    self.dismiss();
                }
            });
        });
    }

    fn editor_window(&mut self, ctx: &egui::Context, accent: Color32) {
        let mut open = self.editor.is_some();
        if !open {
            return;
        }

        let title = if self.editor_is_new { "New alarm" } else { "Edit alarm" };
        let mut save = false;
        let mut cancel = false;
        let mut browse = false;
        let mut preview = false;

        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                if let Some(alarm) = self.editor.as_mut() {
                    ui.add_space(6.0);
                    egui::Grid::new("editor_grid").num_columns(2).spacing([16.0, 10.0]).show(ui, |ui| {
                        ui.label("Time");
                        ui.horizontal(|ui| {
                            if self.store.use_12_hour {
                                let (mut hour12, mut is_pm) = model::to_12_hour(alarm.hour);
                                ui.add(
                                    egui::DragValue::new(&mut hour12)
                                        .clamp_range(1..=12)
                                        .custom_formatter(|n, _| format!("{}", n as u32)),
                                );
                                ui.label(":");
                                ui.add(
                                    egui::DragValue::new(&mut alarm.minute)
                                        .clamp_range(0..=59)
                                        .custom_formatter(|n, _| format!("{:02}", n as u32)),
                                );
                                egui::ComboBox::from_id_source("ampm")
                                    .selected_text(if is_pm { "PM" } else { "AM" })
                                    .width(52.0)
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut is_pm, false, "AM");
                                        ui.selectable_value(&mut is_pm, true, "PM");
                                    });
                                alarm.hour = model::to_24_hour(hour12, is_pm);
                            } else {
                                ui.add(
                                    egui::DragValue::new(&mut alarm.hour)
                                        .clamp_range(0..=23)
                                        .custom_formatter(|n, _| format!("{:02}", n as u32)),
                                );
                                ui.label(":");
                                ui.add(
                                    egui::DragValue::new(&mut alarm.minute)
                                        .clamp_range(0..=59)
                                        .custom_formatter(|n, _| format!("{:02}", n as u32)),
                                );
                            }
                        });
                        ui.end_row();

                        ui.label("Message");
                        ui.add(egui::TextEdit::singleline(&mut alarm.label).desired_width(280.0).hint_text("Pick up kids from school"));
                        ui.end_row();

                        ui.label("Repeat");
                        egui::ComboBox::from_id_source("repeat")
                            .selected_text(alarm.repeat.label())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut alarm.repeat, Repeat::Once, Repeat::Once.label());
                                ui.selectable_value(&mut alarm.repeat, Repeat::Daily, Repeat::Daily.label());
                                ui.selectable_value(&mut alarm.repeat, Repeat::Weekdays, Repeat::Weekdays.label());
                                ui.selectable_value(&mut alarm.repeat, Repeat::Weekends, Repeat::Weekends.label());
                            });
                        ui.end_row();

                        ui.label("Snooze");
                        ui.add(egui::DragValue::new(&mut alarm.snooze_minutes).suffix(" minutes").clamp_range(1..=60));
                        ui.end_row();

                        ui.label("Sound");
                        ui.vertical(|ui| {
                            let mut use_default = alarm.sound_path.trim().is_empty();
                            if ui.radio_value(&mut use_default, true, "Built-in default").changed() && use_default {
                                alarm.sound_path.clear();
                            }
                            if ui.radio_value(&mut use_default, false, "Custom file").changed() && use_default {
                                // switched back to default
                                alarm.sound_path.clear();
                            }
                            ui.horizontal(|ui| {
                                ui.add_enabled(
                                    !use_default,
                                    egui::TextEdit::singleline(&mut alarm.sound_path)
                                        .desired_width(220.0)
                                        .hint_text("/path/to/sound.ogg"),
                                );
                                if ui.add_enabled(!use_default, egui::Button::new("Browse…")).clicked() {
                                    browse = true;
                                }
                            });
                            if ui.button("Test sound").clicked() {
                                preview = true;
                            }
                        });
                        ui.end_row();

                        ui.label("Enabled");
                        ui.checkbox(&mut alarm.enabled, "");
                        ui.end_row();
                    });

                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add(egui::Button::new(RichText::new("Save").color(Color32::BLACK)).fill(accent))
                            .clicked()
                        {
                            save = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                }
            });

        if browse {
            if let Some(path) = audio::pick_audio_file() {
                if let Some(alarm) = self.editor.as_mut() {
                    alarm.sound_path = path.to_string_lossy().to_string();
                }
            }
        }
        if preview {
            if let Some(alarm) = self.editor.as_ref() {
                let path = if alarm.sound_path.trim().is_empty() {
                    None
                } else {
                    Some(Path::new(alarm.sound_path.trim()))
                };
                audio::preview_sound(path);
            }
        }
        if save {
            if let Some(mut alarm) = self.editor.take() {
                alarm.hour = alarm.hour.min(23);
                alarm.minute = alarm.minute.min(59);
                alarm.snooze_minutes = alarm.snooze_minutes.max(1);
                alarm.snooze_until = None;
                if let Some(existing) = self.store.alarms.iter_mut().find(|a| a.id == alarm.id) {
                    *existing = alarm;
                } else {
                    self.store.alarms.push(alarm);
                    self.store.alarms.sort_by_key(|a| (a.hour, a.minute));
                }
                self.persist();
                self.status = "Alarm saved.".to_string();
            }
        }
        if cancel || !open {
            self.editor = None;
        }
    }
}
