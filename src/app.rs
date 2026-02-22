use crate::ai::llm_client::LlmClient;
use crate::notification::NotificationStore;
use crate::terminal::{TerminalPane, TerminalState};
use crate::workspace::Workspace;
use eframe::egui;
use egui_dock::{DockArea, DockState, TabViewer, tab_viewer::OnCloseResponse};
use egui_phosphor::regular;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

pub struct RmuxApp {
    dock_states: HashMap<Uuid, DockState<Tab>>,
    current_workspace: Option<Uuid>,
    workspaces: HashMap<Uuid, Workspace>,
    workspace_order: Vec<Uuid>,
    notifications: NotificationStore,
    sidebar_width: f32,
    show_sidebar: bool,
    show_notifications: bool,
    presets: Vec<String>,
    selected_preset: Option<String>,
}

#[derive(Clone)]
pub struct Tab {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub title: String,
    pub pane_type: PaneType,
    pub has_notification: bool,
    pub terminal_state: TerminalState,
    pub llm_client: Arc<LlmClient>,
    pub preset_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PaneType {
    Terminal,
    Browser,
}

impl Tab {
    pub fn new(
        workspace_id: Uuid,
        title: String,
        pane_type: PaneType,
        preset: Option<&str>,
    ) -> Self {
        let llm_client = match preset {
            Some(p) => LlmClient::with_preset(p),
            None => LlmClient::default_client(),
        };

        let client = Arc::new(llm_client);
        let client_clone = client.clone();
        tokio::spawn(async move {
            if let Err(e) = client_clone.initialize().await {
                eprintln!("[rmux] Failed to initialize tab client: {}", e);
            }
        });

        Self {
            id: Uuid::new_v4(),
            workspace_id,
            title: match preset {
                Some(p) => format!("{} ({})", title, p),
                None => title,
            },
            pane_type,
            has_notification: false,
            terminal_state: TerminalState::default(),
            llm_client: client,
            preset_name: preset.map(|s| s.to_string()),
        }
    }
}

impl RmuxApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut fonts = egui::FontDefinitions::default();
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
        cc.egui_ctx.set_fonts(fonts);

        let workspace_id = Uuid::new_v4();
        let workspace = Workspace::new(workspace_id, "Workspace 1".to_string());

        let mut workspaces = HashMap::new();
        workspaces.insert(workspace_id, workspace);

        let presets = Self::load_presets();
        let selected_preset = presets.first().map(|s| s.to_string());

        let tab = Tab::new(
            workspace_id,
            "Terminal 1".to_string(),
            PaneType::Terminal,
            selected_preset.as_deref(),
        );
        let dock_state = DockState::new(vec![tab]);

        let mut dock_states = HashMap::new();
        dock_states.insert(workspace_id, dock_state);

        Self {
            dock_states,
            current_workspace: Some(workspace_id),
            workspaces,
            workspace_order: vec![workspace_id],
            notifications: NotificationStore::new(),
            sidebar_width: 200.0,
            show_sidebar: true,
            show_notifications: false,
            presets,
            selected_preset,
        }
    }

    fn load_presets() -> Vec<String> {
        let config_paths: Vec<&str> = vec!["config.yml", "config.yaml"];

        for path in &config_paths {
            if let Ok(content) = std::fs::read_to_string(path) {
                if let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(&content) {
                    if let Some(presets) = value.get("presets").and_then(|p| p.as_mapping()) {
                        return presets
                            .keys()
                            .filter_map(|k| k.as_str().map(|s| s.to_string()))
                            .collect();
                    }
                }
            }
        }

        Vec::new()
    }

    fn switch_preset(&mut self, preset: &str) {
        self.selected_preset = Some(preset.to_string());
    }

    fn count_tabs(&self) -> usize {
        if let Some(workspace_id) = self.current_workspace {
            if let Some(dock_state) = self.dock_states.get(&workspace_id) {
                let mut count = 0;
                for (_surface, node) in dock_state.iter_all_nodes() {
                    if let Some(tabs) = node.tabs() {
                        count += tabs.len();
                    }
                }
                return count;
            }
        }
        0
    }

    fn add_workspace(&mut self) {
        let id = Uuid::new_v4();
        let num = self.workspaces.len() + 1;
        let workspace = Workspace::new(id, format!("Workspace {}", num));

        let tab = Tab::new(
            id,
            "Terminal 1".to_string(),
            PaneType::Terminal,
            self.selected_preset.as_deref(),
        );
        let new_dock = DockState::new(vec![tab]);

        self.workspaces.insert(id, workspace);
        self.workspace_order.push(id);
        self.dock_states.insert(id, new_dock);
        self.current_workspace = Some(id);
    }

    fn add_terminal(&mut self) {
        if let Some(workspace_id) = self.current_workspace {
            let num = self.count_tabs() + 1;
            if let Some(dock_state) = self.dock_states.get_mut(&workspace_id) {
                let tab = Tab::new(
                    workspace_id,
                    format!("Terminal {}", num),
                    PaneType::Terminal,
                    self.selected_preset.as_deref(),
                );
                dock_state.push_to_focused_leaf(tab);
            }
        }
    }

    fn add_browser(&mut self) {
        if let Some(workspace_id) = self.current_workspace {
            let num = self.count_tabs() + 1;
            if let Some(dock_state) = self.dock_states.get_mut(&workspace_id) {
                let tab = Tab {
                    id: uuid::Uuid::new_v4(),
                    workspace_id,
                    title: format!("Browser {}", num),
                    pane_type: PaneType::Browser,
                    has_notification: false,
                    terminal_state: TerminalState::default(),
                    llm_client: std::sync::Arc::new(
                        crate::ai::llm_client::LlmClient::default_client(),
                    ),
                    preset_name: None,
                };
                dock_state.push_to_focused_leaf(tab);
            }
        }
    }

    fn split_right(&mut self) {
        if let Some(workspace_id) = self.current_workspace {
            let num = self.count_tabs() + 1;
            if let Some(dock_state) = self.dock_states.get_mut(&workspace_id) {
                if let Some((surface, node)) = dock_state.focused_leaf() {
                    let new_tab = Tab::new(
                        workspace_id,
                        format!("Terminal {}", num),
                        PaneType::Terminal,
                        self.selected_preset.as_deref(),
                    );
                    dock_state[surface].split_tabs(
                        node,
                        egui_dock::Split::Right,
                        0.5,
                        vec![new_tab],
                    );
                }
            }
        }
    }

    fn split_down(&mut self) {
        if let Some(workspace_id) = self.current_workspace {
            let num = self.count_tabs() + 1;
            if let Some(dock_state) = self.dock_states.get_mut(&workspace_id) {
                if let Some((surface, node)) = dock_state.focused_leaf() {
                    let new_tab = Tab::new(
                        workspace_id,
                        format!("Terminal {}", num),
                        PaneType::Terminal,
                        self.selected_preset.as_deref(),
                    );
                    dock_state[surface].split_tabs(
                        node,
                        egui_dock::Split::Below,
                        0.5,
                        vec![new_tab],
                    );
                }
            }
        }
    }

    fn select_workspace(&mut self, id: Uuid) {
        if self.workspaces.contains_key(&id) {
            self.current_workspace = Some(id);
        }
    }

    fn toggle_sidebar(&mut self) {
        self.show_sidebar = !self.show_sidebar;
    }

    fn check_pending_notifications(&mut self) {
        let current_workspace = self.current_workspace;

        for (workspace_id, dock_state) in self.dock_states.iter_mut() {
            let is_current_workspace = current_workspace == Some(*workspace_id);
            let focused_tab_id = dock_state.find_active_focused().map(|(_, t)| t.id);

            for (_surface, node) in dock_state.iter_all_nodes_mut() {
                if let Some(tabs) = node.tabs_mut() {
                    for tab in tabs {
                        if let Some(ref pending) = tab.terminal_state.pending {
                            if pending.should_notify() {
                                let is_current_tab =
                                    is_current_workspace && focused_tab_id == Some(tab.id);

                                if !is_current_tab {
                                    if let Some(content) = pending.get_assistant_content() {
                                        self.notifications.add_notification(
                                            tab.workspace_id,
                                            tab.id,
                                            "AI Response",
                                            "AI replied",
                                            content.chars().take(100).collect::<String>(),
                                        );
                                        tab.has_notification = true;
                                    } else if let Some(err) = pending.get_error() {
                                        self.notifications.add_notification(
                                            tab.workspace_id,
                                            tab.id,
                                            "Error",
                                            "AI encountered an error",
                                            err.chars().take(100).collect::<String>(),
                                        );
                                        tab.has_notification = true;
                                    }
                                }
                                pending.mark_notified();
                            }
                        }
                    }
                }
            }
        }
    }

    fn clear_all_notifications(&mut self) {
        self.notifications.mark_all_read();
        for dock_state in self.dock_states.values_mut() {
            for (_surface, node) in dock_state.iter_all_nodes_mut() {
                if let Some(tabs) = node.tabs_mut() {
                    for tab in tabs {
                        tab.has_notification = false;
                    }
                }
            }
        }
    }

    fn render_sidebar(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Presets");
            ui.separator();

            if self.presets.is_empty() {
                ui.label(egui::RichText::new("No presets in config").italics().weak());
            } else {
                for preset in &self.presets.clone() {
                    let is_selected = self.selected_preset.as_ref() == Some(preset);
                    if ui.selectable_label(is_selected, preset).clicked() {
                        self.switch_preset(preset);
                    }
                }
            }

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.heading("Workspaces");
                let unread = self.notifications.get_unread_count();
                if unread > 0 {
                    ui.label(
                        egui::RichText::new(format!("({})", unread))
                            .color(egui::Color32::from_rgb(255, 100, 100)),
                    );
                }
            });
            ui.separator();

            let workspace_ids: Vec<Uuid> = self.workspace_order.clone();
            let mut select_id: Option<Uuid> = None;
            for id in workspace_ids {
                if let Some(workspace) = self.workspaces.get_mut(&id) {
                    let is_selected = self.current_workspace == Some(id);
                    let has_notifications = self.notifications.has_unread_for_workspace(id);

                    if let Some(dock_state) = self.dock_states.get(&id) {
                        for (_surface, node) in dock_state.iter_all_nodes() {
                            if let Some(tabs) = node.tabs() {
                                if let Some(tab) = tabs.first() {
                                    if let Some(ws_path) = tab.llm_client.get_workspace() {
                                        workspace.update_from_cwd(&ws_path.to_string_lossy());
                                    }
                                }
                            }
                        }
                    }

                    let text = if has_notifications {
                        egui::RichText::new(&workspace.name)
                            .color(egui::Color32::from_rgb(100, 149, 237))
                    } else {
                        egui::RichText::new(&workspace.name)
                    };

                    if ui.selectable_label(is_selected, text).clicked() {
                        select_id = Some(id);
                    }

                    if self.current_workspace == Some(id) {
                        if let Some(ref cwd) = workspace.working_directory {
                            let truncated_cwd = if cwd.len() > 30 {
                                format!("...{}", &cwd[cwd.len() - 27..])
                            } else {
                                cwd.clone()
                            };
                            ui.label(egui::RichText::new(&truncated_cwd).small().weak());
                            if cwd.contains("/") || cwd.contains("\\") {
                                let path = std::path::Path::new(cwd);
                                if path.join(".git").exists() {
                                    if let Ok(output) = std::process::Command::new("git")
                                        .args(["branch", "--show-current"])
                                        .current_dir(path)
                                        .output()
                                    {
                                        if output.status.success() {
                                            let branch = String::from_utf8_lossy(&output.stdout)
                                                .trim()
                                                .to_string();
                                            if !branch.is_empty() {
                                                workspace.set_git_branch(&branch);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if let Some(ref branch) = workspace.git_branch {
                            ui.label(
                                egui::RichText::new(format!("{} {}", regular::GIT_BRANCH, branch))
                                    .small()
                                    .color(egui::Color32::from_rgb(100, 200, 100)),
                            );
                        }

                        let workspace_notifications: Vec<(Uuid, String, String, String, bool)> =
                            self.notifications
                                .get_notifications_for_workspace(id)
                                .iter()
                                .map(|n| {
                                    (
                                        n.id,
                                        n.title.clone(),
                                        n.subtitle.clone(),
                                        n.body.clone(),
                                        n.read,
                                    )
                                })
                                .collect();
                        for (notif_id, title, subtitle, body, read) in workspace_notifications {
                            let color = if read {
                                egui::Color32::from_rgb(150, 150, 150)
                            } else {
                                egui::Color32::from_rgb(255, 255, 255)
                            };
                            egui::Frame::new()
                                .fill(egui::Color32::from_rgb(25, 25, 30))
                                .inner_margin(egui::vec2(4.0, 2.0))
                                .corner_radius(egui::CornerRadius::same(2))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        if ui.small_button(regular::CHECK).clicked() {
                                            self.notifications.mark_read(notif_id);
                                        }
                                        ui.vertical(|ui| {
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    egui::RichText::new(&title)
                                                        .color(color)
                                                        .small()
                                                        .strong(),
                                                );
                                            });
                                            if !subtitle.is_empty() {
                                                ui.label(
                                                    egui::RichText::new(&subtitle)
                                                        .color(egui::Color32::from_rgb(
                                                            150, 150, 150,
                                                        ))
                                                        .small(),
                                                );
                                            }
                                            if !body.is_empty() {
                                                ui.label(
                                                    egui::RichText::new(
                                                        &body.chars().take(50).collect::<String>(),
                                                    )
                                                    .small()
                                                    .weak(),
                                                );
                                            }
                                        });
                                    });
                                });
                            ui.add_space(2.0);
                        }
                    }
                }
            }

            if let Some(id) = select_id {
                self.select_workspace(id);
                self.notifications.mark_read_for_tab(id);
            }

            ui.separator();
            if ui.button(format!("{} New Workspace", regular::FOLDER_PLUS)).clicked() {
                self.add_workspace();
            }

            ui.add_space(10.0);
            ui.heading("Tools");
            ui.separator();

            if let Some(workspace_id) = self.current_workspace {
                if let Some(dock_state) = self.dock_states.get(&workspace_id) {
                    for (_surface, node) in dock_state.iter_all_nodes() {
                        if let Some(tabs) = node.tabs() {
                            if let Some(tab) = tabs.first() {
                                if let Some(ref preset) = tab.preset_name {
                                    ui.label(
                                        egui::RichText::new(format!("Preset: {}", preset))
                                            .small()
                                            .weak(),
                                    );
                                }
                                let tools = tab.llm_client.get_tools();
                                let rt = tokio::runtime::Handle::current();
                                let tools_guard = rt.block_on(async { tools.read().await.clone() });
                                if tools_guard.is_empty() {
                                    ui.label(
                                        egui::RichText::new("No tools enabled").italics().weak(),
                                    );
                                } else {
                                    for tool in tools_guard.iter().take(10) {
                                        ui.label(
                                            egui::RichText::new(&tool.function.name)
                                                .small()
                                                .monospace(),
                                        );
                                    }
                                    if tools_guard.len() > 10 {
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "+{} more",
                                                tools_guard.len() - 10
                                            ))
                                            .weak()
                                            .small(),
                                        );
                                    }
                                }

                                ui.add_space(5.0);
                                ui.label(egui::RichText::new("MCP Servers:").small().weak());
                                let mcp_status =
                                    rt.block_on(async { tab.llm_client.get_mcp_status().await });
                                if mcp_status.is_empty() {
                                    ui.label(
                                        egui::RichText::new("None connected")
                                            .italics()
                                            .small()
                                            .weak(),
                                    );
                                } else {
                                    for (name, running) in mcp_status {
                                        let status = if running {
                                            regular::CHECK_CIRCLE
                                        } else {
                                            regular::X_CIRCLE
                                        };
                                        let status_color = if running {
                                            egui::Color32::from_rgb(100, 200, 100)
                                        } else {
                                            egui::Color32::from_rgb(200, 100, 100)
                                        };
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                egui::RichText::new(format!("{} {}", status, name))
                                                    .small()
                                                    .color(status_color),
                                            );
                                            let client = tab.llm_client.clone();
                                            let name_clone = name.clone();
                                            if ui.small_button(regular::X).clicked() {
                                                rt.block_on(async {
                                                    client.disconnect_mcp(&name_clone).await;
                                                });
                                            }
                                        });
                                    }
                                }
                            }
                            break;
                        }
                    }
                }
            }

            ui.add_space(10.0);
            ui.separator();
            if ui.button(format!("{} All Notifications", regular::CLIPBOARD_TEXT)).clicked() {
                self.show_notifications = !self.show_notifications;
            }
        });
    }

    fn render_notifications_panel(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Notifications");
                let unread = self.notifications.get_unread_count();
                if unread > 0 {
                    ui.label(
                        egui::RichText::new(format!("({} unread)", unread))
                            .color(egui::Color32::from_rgb(255, 100, 100)),
                    );
                }
            });
            ui.separator();

            let all: Vec<(Uuid, String, String, String, bool)> = self
                .notifications
                .all_notifications()
                .iter()
                .map(|n| {
                    (
                        n.id,
                        n.title.clone(),
                        n.subtitle.clone(),
                        n.body.clone(),
                        n.read,
                    )
                })
                .collect();
            if all.is_empty() {
                ui.label(egui::RichText::new("No notifications").italics().weak());
            } else {
                for (notif_id, title, subtitle, body, read) in all {
                    let bg = if read {
                        egui::Color32::from_rgb(30, 30, 35)
                    } else {
                        egui::Color32::from_rgb(40, 40, 50)
                    };
                    egui::Frame::new()
                        .fill(bg)
                        .inner_margin(egui::vec2(8.0, 6.0))
                        .corner_radius(egui::CornerRadius::same(4))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                if !read {
                                    if ui.small_button(regular::CHECK).clicked() {
                                        self.notifications.mark_read(notif_id);
                                    }
                                }
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(egui::RichText::new(&title).strong());
                                        ui.label(egui::RichText::new(&subtitle).weak().small());
                                    });
                                    ui.label(egui::RichText::new(&body).small());
                                });
                            });
                        });
                    ui.add_space(4.0);
                }

                ui.separator();
                if ui.button(format!("{} Mark All Read", regular::CHECKS)).clicked() {
                    self.notifications.mark_all_read();
                    for dock_state in self.dock_states.values_mut() {
                        for (_surface, node) in dock_state.iter_all_nodes_mut() {
                            if let Some(tabs) = node.tabs_mut() {
                                for tab in tabs {
                                    tab.has_notification = false;
                                }
                            }
                        }
                    }
                }
                if ui.button(format!("{} Clear All", regular::TRASH)).clicked() {
                    self.notifications.clear_all();
                }
            }
        });
    }
}

impl eframe::App for RmuxApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.check_pending_notifications();

        if self.show_sidebar {
            egui::SidePanel::left("sidebar")
                .default_width(self.sidebar_width)
                .resizable(true)
                .show(ctx, |ui| {
                    self.render_sidebar(ui);
                });
        }

        if self.show_notifications {
            egui::SidePanel::right("notifications")
                .default_width(250.0)
                .resizable(true)
                .show(ctx, |ui| {
                    self.render_notifications_panel(ui);
                });
        }

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button(regular::LIST).clicked() {
                    self.toggle_sidebar();
                }
                ui.separator();
                if ui.button(format!("{} Terminal", regular::PLUS)).clicked() {
                    self.add_terminal();
                }
                if ui.button(format!("{} Browser", regular::GLOBE)).clicked() {
                    self.add_browser();
                }
                if ui.button(format!("Split {}", regular::ARROW_RIGHT)).clicked() {
                    self.split_right();
                }
                if ui.button(format!("Split {}", regular::ARROW_DOWN)).clicked() {
                    self.split_down();
                }
                ui.separator();
                if ui.button(format!("{} Clear Notifications", regular::TRASH)).clicked() {
                    self.clear_all_notifications();
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let unread = self.notifications.get_unread_count();
                    if unread > 0 {
                        if ui.button(format!("{} {}", regular::BELL, unread)).clicked() {
                            self.show_notifications = !self.show_notifications;
                        }
                    }
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(workspace_id) = self.current_workspace {
                if let Some(dock_state) = self.dock_states.get_mut(&workspace_id) {
                    DockArea::new(dock_state).show_inside(
                        ui,
                        &mut TabViewerImpl {
                            notifications: &mut self.notifications,
                        },
                    );
                }
            }
        });
    }
}

struct TabViewerImpl<'a> {
    notifications: &'a mut NotificationStore,
}

impl TabViewer for TabViewerImpl<'_> {
    type Tab = Tab;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        let has_unread = self.notifications.has_unread_for_tab(tab.id);
        if has_unread || tab.has_notification {
            format!("{} {}", regular::BELL, tab.title).into()
        } else {
            tab.title.clone().into()
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        if tab.has_notification {
            self.notifications.mark_read_for_tab(tab.id);
            tab.has_notification = false;
        }
        match tab.pane_type {
            PaneType::Terminal => {
                TerminalPane::show(ui, tab, tab.llm_client.clone());
            }
            PaneType::Browser => {
                ui.vertical(|ui| {
                    ui.heading("Browser");
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("URL:");
                        ui.text_edit_singleline(&mut tab.terminal_state.input);
                    });
                    ui.separator();
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.label(egui::RichText::new("Browser pane for viewing tool results, markdown, or web content.").weak());
                        ui.label(egui::RichText::new("(Future: integrate with egui_extras::SyntectViewer for code highlighting)").italics().small().weak());
                    });
                });
            }
        }
    }

    fn on_close(&mut self, _tab: &mut Self::Tab) -> OnCloseResponse {
        OnCloseResponse::Close
    }
}
