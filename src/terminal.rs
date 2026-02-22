use crate::ai::llm_client::{ChatMessage, LlmClient, StreamEvent};
use crate::app::Tab;
use eframe::egui::{self, Color32, TextEdit};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

pub struct TerminalPane;

#[derive(Clone, Debug)]
pub enum UiEvent {
    Text(String),
    ToolCall {
        id: String,
        name: String,
        arguments: String,
    },
    ToolExecuting {
        name: String,
        arguments: String,
    },
    ToolResult {
        id: String,
        name: String,
        result: String,
    },
    Done,
    Error(String),
}

#[derive(Clone)]
pub struct PendingResponse {
    pub ready: Arc<AtomicBool>,
    pub conversation: Arc<RwLock<Vec<ChatMessage>>>,
    pub events: Arc<RwLock<Vec<UiEvent>>>,
    pub error: Arc<RwLock<Option<String>>>,
    pub notified: Arc<AtomicBool>,
}

impl PendingResponse {
    pub fn new() -> Self {
        Self {
            ready: Arc::new(AtomicBool::new(false)),
            conversation: Arc::new(RwLock::new(Vec::new())),
            events: Arc::new(RwLock::new(Vec::new())),
            error: Arc::new(RwLock::new(None)),
            notified: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Relaxed)
    }

    pub fn should_notify(&self) -> bool {
        self.is_ready() && !self.notified.load(Ordering::Relaxed)
    }

    pub fn mark_notified(&self) {
        self.notified.store(true, Ordering::Relaxed);
    }

    pub fn get_assistant_content(&self) -> Option<String> {
        if let Ok(conv) = self.conversation.read() {
            for msg in conv.iter().rev() {
                if msg.role == "assistant" {
                    if let Some(c) = &msg.content {
                        if !c.is_empty() {
                            return Some(c.clone());
                        }
                    }
                }
            }
        }
        None
    }

    pub fn get_error(&self) -> Option<String> {
        self.error.read().ok().and_then(|e| e.clone())
    }
}

#[derive(Clone, Default)]
pub struct TerminalState {
    pub messages: Vec<TerminalMessage>,
    pub input: String,
    pub pending: Option<PendingResponse>,
    pub history_loaded: bool,
    pub show_clear_confirm: bool,
}

#[derive(Clone)]
pub enum TerminalMessage {
    User(String),
    Assistant(String),
    ToolCall {
        id: String,
        name: String,
        arguments: String,
    },
    ToolResult {
        id: String,
        name: String,
        result: String,
    },
    Error(String),
}

impl TerminalPane {
    pub fn show(
        ui: &mut egui::Ui,
        tab: &mut Tab,
        llm_client: Arc<LlmClient>,
    ) {
        ui.horizontal(|ui| {
            if ui.small_button("🗑").clicked() {
                tab.terminal_state.show_clear_confirm = !tab.terminal_state.show_clear_confirm;
            }
            if tab.terminal_state.show_clear_confirm {
                ui.label(egui::RichText::new("Clear history?").small());
                if ui.small_button("Yes").clicked() {
                    let client = llm_client.clone();
                    tokio::spawn(async move {
                        client.clear_conversation().await;
                    });
                    tab.terminal_state.messages.clear();
                    tab.terminal_state.history_loaded = true;
                    tab.terminal_state.show_clear_confirm = false;
                }
                if ui.small_button("No").clicked() {
                    tab.terminal_state.show_clear_confirm = false;
                }
            }
        });

        if !tab.terminal_state.history_loaded {
            tab.terminal_state.history_loaded = true;
            let rt = tokio::runtime::Handle::current();
            let conv = rt.block_on(async { llm_client.get_conversation().await });
            for msg in &conv {
                match msg.role.as_str() {
                    "user" => {
                        if let Some(content) = &msg.content {
                            tab.terminal_state
                                .messages
                                .push(TerminalMessage::User(content.clone()));
                        }
                    }
                    "assistant" => {
                        if let Some(content) = &msg.content {
                            if !content.is_empty() {
                                tab.terminal_state
                                    .messages
                                    .push(TerminalMessage::Assistant(content.clone()));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        egui::ScrollArea::vertical()
            .stick_to_bottom(true)
            .show(ui, |ui| {
                ui.style_mut().visuals.extreme_bg_color = Color32::from_rgb(20, 20, 25);

                if tab.terminal_state.messages.is_empty() {
                    ui.label(
                        egui::RichText::new("Terminal with AI. Type message and press Enter.")
                            .font(egui::FontId::monospace(12.0))
                            .color(Color32::from_rgb(150, 150, 150))
                            .italics(),
                    );
                }

                for msg in &tab.terminal_state.messages {
                    match msg {
                        TerminalMessage::User(content) => {
                            render_message_header(ui, "You", Color32::from_rgb(100, 200, 255));
                            render_content(ui, content, Color32::from_rgb(220, 220, 220));
                            ui.add_space(8.0);
                        }
                        TerminalMessage::Assistant(content) => {
                            if !content.is_empty() {
                                render_message_header(
                                    ui,
                                    "Assistant",
                                    Color32::from_rgb(100, 255, 150),
                                );
                                render_content(ui, content, Color32::from_rgb(220, 220, 220));
                                ui.add_space(8.0);
                            }
                        }
                        TerminalMessage::ToolCall {
                            id,
                            name,
                            arguments,
                        } => {
                            let title = if id.is_empty() {
                                format!("Tool: {}", name)
                            } else {
                                format!("Tool: {} [{}]", name, &id[..id.len().min(8)])
                            };
                            render_box(
                                ui,
                                &title,
                                arguments,
                                Color32::from_rgb(60, 60, 80),
                                Color32::from_rgb(255, 200, 100),
                            );
                            ui.add_space(4.0);
                        }
                        TerminalMessage::ToolResult { id, name, result } => {
                            let truncated = if result.len() > 500 {
                                format!(
                                    "{}...\n[truncated, {} bytes total]",
                                    &result[..500],
                                    result.len()
                                )
                            } else {
                                result.clone()
                            };
                            let title = if id.is_empty() {
                                format!("Result: {}", name)
                            } else {
                                format!("Result: {} [{}]", name, &id[..id.len().min(8)])
                            };
                            render_box(
                                ui,
                                &title,
                                &truncated,
                                Color32::from_rgb(40, 60, 40),
                                Color32::from_rgb(150, 255, 150),
                            );
                            ui.add_space(8.0);
                        }
                        TerminalMessage::Error(content) => {
                            render_box(
                                ui,
                                "Error",
                                content,
                                Color32::from_rgb(80, 40, 40),
                                Color32::from_rgb(255, 100, 100),
                            );
                            ui.add_space(8.0);
                        }
                    }
                }

                if let Some(pending) = &tab.terminal_state.pending {
                    if let Ok(events) = pending.events.read() {
                        for event in events.iter() {
                            match event {
                                UiEvent::Text(text) => {
                                    if let Some(last) = tab.terminal_state.messages.last_mut() {
                                        if let TerminalMessage::Assistant(content) = last {
                                            content.push_str(text);
                                            continue;
                                        }
                                    }
                                    tab.terminal_state
                                        .messages
                                        .push(TerminalMessage::Assistant(text.clone()));
                                }
                                UiEvent::ToolCall {
                                    id,
                                    name,
                                    arguments,
                                } => {
                                    tab.terminal_state.messages.push(TerminalMessage::ToolCall {
                                        id: id.clone(),
                                        name: name.clone(),
                                        arguments: arguments.clone(),
                                    });
                                }
                                UiEvent::ToolResult { id, name, result } => {
                                    tab.terminal_state
                                        .messages
                                        .push(TerminalMessage::ToolResult {
                                            id: id.clone(),
                                            name: name.clone(),
                                            result: result.clone(),
                                        });
                                }
                                UiEvent::ToolExecuting { name, arguments } => {
                                    tab.terminal_state.messages.push(TerminalMessage::ToolCall {
                                        id: String::new(),
                                        name: name.clone(),
                                        arguments: arguments.clone(),
                                    });
                                }
                                UiEvent::Error(err) => {
                                    tab.terminal_state
                                        .messages
                                        .push(TerminalMessage::Error(err.clone()));
                                }
                                UiEvent::Done => {}
                            }
                        }
                    }

                    if let Ok(mut events) = pending.events.write() {
                        events.clear();
                    }

                    if pending.ready.load(Ordering::Relaxed) {
                        if let Ok(error) = pending.error.read() {
                            if let Some(err) = error.as_ref() {
                                tab.terminal_state
                                    .messages
                                    .push(TerminalMessage::Error(err.clone()));
                            }
                        }
                        tab.terminal_state.pending = None;
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
                        TextEdit::singleline(&mut tab.terminal_state.input)
                            .desired_width(f32::INFINITY)
                            .font(egui::FontId::monospace(13.0))
                            .text_color(Color32::from_rgb(220, 220, 220)),
                    );

                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        if !tab.terminal_state.input.trim().is_empty()
                            && tab.terminal_state.pending.is_none()
                        {
                            let user_msg = tab.terminal_state.input.trim().to_string();
                            tab.terminal_state
                                .messages
                                .push(TerminalMessage::User(user_msg.clone()));
                            tab.terminal_state.input.clear();

                            let pending = PendingResponse::new();

                            let ready = pending.ready.clone();
                            let conversation_arc = pending.conversation.clone();
                            let events_arc = pending.events.clone();
                            let error_arc = pending.error.clone();
                            let client = llm_client.clone();

                            tokio::spawn(async move {
                                let result = client
                                    .send_message_stream(&user_msg, |event| {
                                        let ui_event = match event {
                                            StreamEvent::Text(t) => UiEvent::Text(t),
                                            StreamEvent::ToolCallStart { id, name } => {
                                                UiEvent::ToolCall {
                                                    id,
                                                    name,
                                                    arguments: String::new(),
                                                }
                                            }
                                            StreamEvent::ToolCallDelta(args) => {
                                                UiEvent::Text(format!(" {}", args))
                                            }
                                            StreamEvent::ToolCallEnd => UiEvent::Done,
                                            StreamEvent::ToolExecuting { name, arguments } => {
                                                UiEvent::ToolExecuting { name, arguments }
                                            }
                                            StreamEvent::ToolResult { id, name, result } => {
                                                UiEvent::ToolResult { id, name, result }
                                            }
                                            StreamEvent::Done => UiEvent::Done,
                                            StreamEvent::Error(e) => UiEvent::Error(e),
                                        };

                                        if let Ok(mut events) = events_arc.write() {
                                            events.push(ui_event);
                                        }
                                    })
                                    .await;

                                match result {
                                    Ok(_) => {
                                        let conv = client.get_conversation().await;
                                        if let Ok(mut guard) = conversation_arc.write() {
                                            *guard = conv;
                                        }
                                    }
                                    Err(e) => {
                                        if let Ok(mut guard) = error_arc.write() {
                                            *guard = Some(e.to_string());
                                        }
                                    }
                                }
                                ready.store(true, Ordering::Relaxed);
                            });

                            tab.terminal_state.pending = Some(pending);
                        }
                        response.request_focus();
                    }
                });
            });

        ui.ctx().request_repaint();
    }
}

fn render_message_header(ui: &mut egui::Ui, label: &str, color: Color32) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("{}:", label))
                .font(egui::FontId::monospace(13.0))
                .color(color)
                .strong(),
        );
    });
}

fn render_content(ui: &mut egui::Ui, content: &str, color: Color32) {
    for line in content.lines() {
        ui.label(
            egui::RichText::new(line)
                .font(egui::FontId::monospace(12.0))
                .color(color),
        );
    }
}

fn render_box(
    ui: &mut egui::Ui,
    title: &str,
    content: &str,
    bg_color: Color32,
    title_color: Color32,
) {
    egui::Frame::new()
        .fill(bg_color)
        .inner_margin(egui::vec2(8.0, 6.0))
        .corner_radius(egui::CornerRadius::same(4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(title)
                        .font(egui::FontId::monospace(11.0))
                        .color(title_color)
                        .strong(),
                );
            });

            for line in content.lines().take(20) {
                ui.label(
                    egui::RichText::new(line)
                        .font(egui::FontId::monospace(11.0))
                        .color(Color32::from_rgb(200, 200, 200)),
                );
            }

            if content.lines().count() > 20 {
                ui.label(
                    egui::RichText::new(format!(
                        "... ({} more lines)",
                        content.lines().count() - 20
                    ))
                    .font(egui::FontId::monospace(10.0))
                    .color(Color32::from_rgb(150, 150, 150))
                    .italics(),
                );
            }
        });
}
