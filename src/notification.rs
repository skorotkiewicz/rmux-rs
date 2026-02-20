use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Notification {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub tab_id: Uuid,
    pub title: String,
    pub subtitle: String,
    pub body: String,
    pub read: bool,
    pub timestamp: i64,
}

pub struct NotificationStore {
    notifications: HashMap<Uuid, Notification>,
    by_workspace: HashMap<Uuid, Vec<Uuid>>,
    by_tab: HashMap<Uuid, Vec<Uuid>>,
}

impl Default for NotificationStore {
    fn default() -> Self {
        Self::new()
    }
}

impl NotificationStore {
    pub fn new() -> Self {
        Self {
            notifications: HashMap::new(),
            by_workspace: HashMap::new(),
            by_tab: HashMap::new(),
        }
    }

    pub fn add_notification(
        &mut self,
        workspace_id: Uuid,
        tab_id: Uuid,
        title: impl Into<String>,
        subtitle: impl Into<String>,
        body: impl Into<String>,
    ) {
        let id = Uuid::new_v4();
        let notification = Notification {
            id,
            workspace_id,
            tab_id,
            title: title.into(),
            subtitle: subtitle.into(),
            body: body.into(),
            read: false,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
        };

        self.notifications.insert(id, notification);
        self.by_workspace.entry(workspace_id).or_default().push(id);
        self.by_tab.entry(tab_id).or_default().push(id);
    }

    pub fn has_unread_for_workspace(&self, workspace_id: Uuid) -> bool {
        self.by_workspace
            .get(&workspace_id)
            .map(|ids| {
                ids.iter()
                    .any(|id| self.notifications.get(id).map(|n| !n.read).unwrap_or(false))
            })
            .unwrap_or(false)
    }

    pub fn has_unread_for_tab(&self, tab_id: Uuid) -> bool {
        self.by_tab
            .get(&tab_id)
            .map(|ids| {
                ids.iter()
                    .any(|id| self.notifications.get(id).map(|n| !n.read).unwrap_or(false))
            })
            .unwrap_or(false)
    }

    pub fn get_notifications_for_workspace(&self, workspace_id: Uuid) -> Vec<&Notification> {
        self.by_workspace
            .get(&workspace_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.notifications.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn get_unread_count(&self) -> usize {
        self.notifications.values().filter(|n| !n.read).count()
    }

    pub fn mark_read(&mut self, notification_id: Uuid) {
        if let Some(notification) = self.notifications.get_mut(&notification_id) {
            notification.read = true;
        }
    }

    pub fn mark_read_for_tab(&mut self, tab_id: Uuid) {
        if let Some(ids) = self.by_tab.get(&tab_id).cloned() {
            for id in ids {
                if let Some(notification) = self.notifications.get_mut(&id) {
                    notification.read = true;
                }
            }
        }
    }

    pub fn mark_all_read(&mut self) {
        for notification in self.notifications.values_mut() {
            notification.read = true;
        }
    }

    pub fn clear_all(&mut self) {
        self.notifications.clear();
        self.by_workspace.clear();
        self.by_tab.clear();
    }

    pub fn all_notifications(&self) -> Vec<&Notification> {
        self.notifications.values().collect()
    }
}
