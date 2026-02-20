use eframe::egui;
use egui_dock::{tab_viewer::OnCloseResponse, DockArea, DockState, TabViewer};
use std::collections::HashMap;
use uuid::Uuid;

use crate::notification::NotificationStore;
use crate::terminal::TerminalPane;
use crate::workspace::Workspace;

pub struct RmuxApp {
    dock_state: DockState<Tab>,
    workspaces: HashMap<Uuid, Workspace>,
    workspace_order: Vec<Uuid>,
    selected_workspace: Option<Uuid>,
    notifications: NotificationStore,
    sidebar_width: f32,
    show_sidebar: bool,
}

#[derive(Clone, Hash, PartialEq, Eq)]
pub struct Tab {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub title: String,
    pub pane_type: PaneType,
    pub has_notification: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PaneType {
    Terminal,
    Browser,
}

impl Tab {
    pub fn new(workspace_id: Uuid, title: String, pane_type: PaneType) -> Self {
        Self {
            id: Uuid::new_v4(),
            workspace_id,
            title,
            pane_type,
            has_notification: false,
        }
    }
}

impl RmuxApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let workspace_id = Uuid::new_v4();
        let workspace = Workspace::new(workspace_id, "Workspace 1".to_string());

        let mut workspaces = HashMap::new();
        workspaces.insert(workspace_id, workspace);

        let tab = Tab::new(workspace_id, "Terminal 1".to_string(), PaneType::Terminal);
        let dock_state = DockState::new(vec![tab]);

        Self {
            dock_state,
            workspaces,
            workspace_order: vec![workspace_id],
            selected_workspace: Some(workspace_id),
            notifications: NotificationStore::new(),
            sidebar_width: 200.0,
            show_sidebar: true,
        }
    }

    fn count_tabs(&self) -> usize {
        let mut count = 0;
        for (_surface, node) in self.dock_state.iter_all_nodes() {
            if let Some(tabs) = node.tabs() {
                count += tabs.len();
            }
        }
        count
    }

    fn add_workspace(&mut self) {
        let id = Uuid::new_v4();
        let num = self.workspaces.len() + 1;
        let workspace = Workspace::new(id, format!("Workspace {}", num));

        let tab = Tab::new(id, "Terminal 1".to_string(), PaneType::Terminal);
        let new_dock = DockState::new(vec![tab]);

        self.workspaces.insert(id, workspace);
        self.workspace_order.push(id);
        self.selected_workspace = Some(id);
        self.dock_state = new_dock;
    }

    fn add_terminal(&mut self) {
        if let Some(workspace_id) = self.selected_workspace {
            let num = self.count_tabs() + 1;
            let tab = Tab::new(
                workspace_id,
                format!("Terminal {}", num),
                PaneType::Terminal,
            );
            self.dock_state.push_to_focused_leaf(tab);
        }
    }

    fn split_right(&mut self) {
        if let Some(workspace_id) = self.selected_workspace {
            if let Some((surface, node)) = self.dock_state.focused_leaf() {
                let num = self.count_tabs() + 1;
                let new_tab = Tab::new(
                    workspace_id,
                    format!("Terminal {}", num),
                    PaneType::Terminal,
                );
                self.dock_state[surface].split_tabs(
                    node,
                    egui_dock::Split::Right,
                    0.5,
                    vec![new_tab],
                );
            }
        }
    }

    fn split_down(&mut self) {
        if let Some(workspace_id) = self.selected_workspace {
            if let Some((surface, node)) = self.dock_state.focused_leaf() {
                let num = self.count_tabs() + 1;
                let new_tab = Tab::new(
                    workspace_id,
                    format!("Terminal {}", num),
                    PaneType::Terminal,
                );
                self.dock_state[surface].split_tabs(
                    node,
                    egui_dock::Split::Below,
                    0.5,
                    vec![new_tab],
                );
            }
        }
    }

    fn select_workspace(&mut self, id: Uuid) {
        if self.workspaces.contains_key(&id) {
            self.selected_workspace = Some(id);
            let tab = Tab::new(id, "Terminal 1".to_string(), PaneType::Terminal);
            self.dock_state = DockState::new(vec![tab]);
        }
    }

    fn toggle_sidebar(&mut self) {
        self.show_sidebar = !self.show_sidebar;
    }

    fn clear_all_notifications(&mut self) {
        self.notifications.clear_all();
        for (_surface, node) in self.dock_state.iter_all_nodes_mut() {
            if let Some(tabs) = node.tabs_mut() {
                for tab in tabs {
                    tab.has_notification = false;
                }
            }
        }
    }

    fn render_sidebar(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Workspaces");
            ui.separator();

            let workspace_ids: Vec<Uuid> = self.workspace_order.clone();
            for id in workspace_ids {
                if let Some(workspace) = self.workspaces.get(&id) {
                    let is_selected = self.selected_workspace == Some(id);
                    let has_notifications = self.notifications.has_unread_for_workspace(id);

                    let text = if has_notifications {
                        egui::RichText::new(&workspace.name)
                            .color(egui::Color32::from_rgb(100, 149, 237))
                    } else {
                        egui::RichText::new(&workspace.name)
                    };

                    if ui.selectable_label(is_selected, text).clicked() {
                        self.select_workspace(id);
                    }
                }
            }

            ui.separator();
            if ui.button("+ New Workspace").clicked() {
                self.add_workspace();
            }
        });
    }
}

impl eframe::App for RmuxApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.show_sidebar {
            egui::SidePanel::left("sidebar")
                .default_width(self.sidebar_width)
                .resizable(true)
                .show(ctx, |ui| {
                    self.render_sidebar(ui);
                });
        }

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("☰").clicked() {
                    self.toggle_sidebar();
                }
                ui.separator();
                if ui.button("+ Terminal").clicked() {
                    self.add_terminal();
                }
                if ui.button("Split →").clicked() {
                    self.split_right();
                }
                if ui.button("Split ↓").clicked() {
                    self.split_down();
                }
                ui.separator();
                if ui.button("Clear Notifications").clicked() {
                    self.clear_all_notifications();
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            DockArea::new(&mut self.dock_state).show_inside(
                ui,
                &mut TabViewerImpl {
                    notifications: &mut self.notifications,
                },
            );
        });
    }
}

struct TabViewerImpl<'a> {
    notifications: &'a mut NotificationStore,
}

impl TabViewer for TabViewerImpl<'_> {
    type Tab = Tab;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        if tab.has_notification {
            format!("🔔 {}", tab.title).into()
        } else {
            tab.title.clone().into()
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        match tab.pane_type {
            PaneType::Terminal => {
                TerminalPane::show(ui, tab, self.notifications);
            }
            PaneType::Browser => {
                ui.vertical_centered(|ui| {
                    ui.heading("Browser Panel");
                    ui.label("Browser functionality - placeholder");
                });
            }
        }
    }

    fn on_close(&mut self, _tab: &mut Self::Tab) -> OnCloseResponse {
        OnCloseResponse::Close
    }
}
