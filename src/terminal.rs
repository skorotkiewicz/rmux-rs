use crate::ai::llm_client::{LlmClient, StreamEvent};
use crate::app::Tab;
use crate::notification::NotificationStore;
use eframe::egui::{self, Color32, TextEdit};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct TerminalPane;

#[derive(Clone)]
struct PendingResponse {
    ready: Arc<AtomicBool>,
    response: Arc<RwLock<Option<String>>>,
    error: Arc<RwLock<Option<String>>>,
}

#[derive(Clone, Default)]
pub struct TerminalState {
    messages: Vec<(String, String)>,
    input: String,
    pending: Option<PendingResponse>,
}

impl TerminalPane {
    pub fn show(
        ui: &mut egui::Ui,
        tab: &mut Tab,
        notifications: &mut NotificationStore,
        llm_client: Arc<LlmClient>,
    ) {
        let state_id = ui.make_persistent_id(format!("terminal_state_{}", tab.id));

        let state = ui.memory_mut(|mem| {
            mem.data
                .get_temp_mut_or_insert_with(state_id, TerminalState::default)
                .clone()
        });

        let mut new_state = state;

        egui::ScrollArea::vertical()
            .stick_to_bottom(true)
            .show(ui, |ui| {
                ui.style_mut().visuals.extreme_bg_color = Color32::from_rgb(20, 20, 25);

                if new_state.messages.is_empty() {
                    ui.label(
                        egui::RichText::new("Terminal with AI. Type message and press Enter.")
                            .font(egui::FontId::monospace(12.0))
                            .color(Color32::from_rgb(150, 150, 150))
                            .italics(),
                    );
                }

                for (role, content) in &new_state.messages {
                    let (prefix, color) = match role.as_str() {
                        "user" => ("You", Color32::from_rgb(100, 200, 255)),
                        "assistant" => ("AI", Color32::from_rgb(100, 255, 150)),
                        "tool" => ("Tool", Color32::from_rgb(255, 200, 100)),
                        _ => ("?", Color32::GRAY),
                    };

                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!("{}:", prefix))
                                .font(egui::FontId::monospace(13.0))
                                .color(color)
                                .strong(),
                        );
                    });

                    for line in content.lines() {
                        ui.label(
                            egui::RichText::new(line)
                                .font(egui::FontId::monospace(12.0))
                                .color(Color32::from_rgb(220, 220, 220)),
                        );
                    }
                    ui.add_space(8.0);
                }

                if let Some(pending) = &new_state.pending {
                    if pending.ready.load(Ordering::Relaxed) {
                        let rt = tokio::runtime::Handle::current();
                        let response_guard = rt.block_on(pending.response.read());
                        let error_guard = rt.block_on(pending.error.read());

                        let response_to_add = response_guard.as_ref().cloned();
                        let error_to_add = error_guard.as_ref().cloned();

                        drop(response_guard);
                        drop(error_guard);

                        if let Some(response) = response_to_add {
                            new_state
                                .messages
                                .push(("assistant".to_string(), response.clone()));

                            notifications.add_notification(
                                tab.workspace_id,
                                tab.id,
                                "AI Response",
                                "AI replied",
                                response.chars().take(100).collect::<String>(),
                            );
                            tab.has_notification = true;
                        }

                        if let Some(error) = error_to_add {
                            new_state
                                .messages
                                .push(("assistant".to_string(), format!("Error: {}", error)));
                        }
                        new_state.pending = None;
                    } else {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(
                                egui::RichText::new("Thinking...")
                                    .font(egui::FontId::monospace(12.0))
                                    .color(Color32::from_rgb(150, 150, 150))
                                    .italics(),
                            );
                        });
                    }
                }

                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(">")
                            .font(egui::FontId::monospace(13.0))
                            .color(Color32::from_rgb(100, 255, 100)),
                    );

                    let response = ui.add(
                        TextEdit::singleline(&mut new_state.input)
                            .desired_width(f32::INFINITY)
                            .font(egui::FontId::monospace(13.0))
                            .text_color(Color32::from_rgb(220, 220, 220)),
                    );

                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        if !new_state.input.trim().is_empty() && new_state.pending.is_none() {
                            let user_msg = new_state.input.trim().to_string();
                            new_state.messages.push(("user".to_string(), user_msg.clone()));
                            new_state.input.clear();

                            let pending = PendingResponse {
                                ready: Arc::new(AtomicBool::new(false)),
                                response: Arc::new(RwLock::new(None)),
                                error: Arc::new(RwLock::new(None)),
                            };

                            let ready = pending.ready.clone();
                            let response_arc = pending.response.clone();
                            let error_arc = pending.error.clone();
                            let client = llm_client.clone();

                            tokio::spawn(async move {
                                let mut full_response = String::new();

                                match client
                                    .send_message_stream(&user_msg, |event| match event {
                                        StreamEvent::Text(text) => {
                                            full_response.push_str(&text);
                                        }
                                        StreamEvent::ToolCallStart { name, .. } => {
                                            full_response
                                                .push_str(&format!("\n[Calling tool: {}]\n", name));
                                        }
                                        StreamEvent::ToolCallDelta(_) => {}
                                        StreamEvent::ToolCallEnd => {}
                                        StreamEvent::Done => {}
                                        StreamEvent::Error(e) => {
                                            full_response.push_str(&format!("\nError: {}", e));
                                        }
                                    })
                                    .await
                                {
                                    Ok(_) => {
                                        *response_arc.write().await = Some(full_response);
                                    }
                                    Err(e) => {
                                        *error_arc.write().await = Some(e.to_string());
                                    }
                                }
                                ready.store(true, Ordering::Relaxed);
                            });

                            new_state.pending = Some(pending);
                        }
                        response.request_focus();
                    }
                });
            });

        ui.memory_mut(|mem| {
            mem.data.insert_temp(state_id, new_state);
        });

        ui.ctx().request_repaint();
    }
}
