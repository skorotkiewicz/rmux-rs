use eframe::egui::{self, Color32, TextEdit};

use crate::app::Tab;
use crate::notification::NotificationStore;

pub struct TerminalPane;

impl TerminalPane {
    pub fn show(ui: &mut egui::Ui, tab: &mut Tab, notifications: &mut NotificationStore) {
        let sample_output = [
            "Welcome to rmux - Rust Terminal Multiplexer",
            "",
            "$ pwd",
            "/home/user/projects/rmux-rs",
            "",
            "$ git status",
            "On branch main",
            "Your branch is up to date with 'origin/main'.",
            "",
            "$ cargo build",
            "   Compiling rmux-rs v0.1.0",
            "    Finished dev [unoptimized + debuginfo] target(s) in 0.45s",
            "",
        ];

        egui::ScrollArea::vertical()
            .stick_to_bottom(true)
            .show(ui, |ui| {
                ui.style_mut().visuals.extreme_bg_color = Color32::from_rgb(20, 20, 25);

                for line in &sample_output {
                    ui.label(
                        egui::RichText::new(*line)
                            .font(egui::FontId::monospace(13.0))
                            .color(Color32::from_rgb(200, 200, 200)),
                    );
                }

                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("$")
                            .font(egui::FontId::monospace(13.0))
                            .color(Color32::from_rgb(100, 255, 100)),
                    );

                    let input_id = ui.make_persistent_id(format!("terminal_input_{}", tab.id));
                    let mut input = ui.memory_mut(|mem| {
                        mem.data
                            .get_temp_mut_or_insert_with(input_id, || String::new())
                            .clone()
                    });

                    let response = ui.add(
                        TextEdit::singleline(&mut input)
                            .desired_width(f32::INFINITY)
                            .font(egui::FontId::monospace(13.0))
                            .text_color(Color32::from_rgb(220, 220, 220)),
                    );

                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        if input.contains("notify") || input.contains("claude") {
                            notifications.add_notification(
                                tab.workspace_id,
                                tab.id,
                                "AI Agent",
                                "Waiting for input",
                                input.clone(),
                            );
                            tab.has_notification = true;
                        }
                        input.clear();
                        response.request_focus();
                    }

                    ui.memory_mut(|mem| {
                        mem.data.insert_temp(input_id, input);
                    });
                });
            });
    }
}
