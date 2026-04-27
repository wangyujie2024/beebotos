//! Notification state management

use std::collections::VecDeque;

use leptos::prelude::*;

/// Notification state
#[derive(Clone, Debug)]
pub struct NotificationState {
    /// Active notifications (limited queue)
    pub notifications: RwSignal<VecDeque<Notification>>,
    /// Maximum number of notifications to keep
    pub max_notifications: usize,
    /// Notification auto-dismiss duration in seconds
    pub auto_dismiss_secs: RwSignal<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Notification {
    pub id: String,
    pub notification_type: NotificationType,
    pub title: String,
    pub message: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub read: bool,
    pub persistent: bool, // If true, won't auto-dismiss
}

#[derive(Clone, Debug, PartialEq)]
pub enum NotificationType {
    Info,
    Success,
    Warning,
    Error,
}

impl NotificationType {
    pub fn icon(&self) -> &'static str {
        match self {
            NotificationType::Info => "ℹ️",
            NotificationType::Success => "✅",
            NotificationType::Warning => "⚠️",
            NotificationType::Error => "❌",
        }
    }

    pub fn css_class(&self) -> &'static str {
        match self {
            NotificationType::Info => "notification-info",
            NotificationType::Success => "notification-success",
            NotificationType::Warning => "notification-warning",
            NotificationType::Error => "notification-error",
        }
    }
}

impl NotificationState {
    pub fn new() -> Self {
        Self {
            notifications: RwSignal::new(VecDeque::new()),
            max_notifications: 50,
            auto_dismiss_secs: RwSignal::new(5),
        }
    }

    /// Add a notification
    pub fn add(
        &self,
        notification_type: NotificationType,
        title: impl Into<String>,
        message: impl Into<String>,
    ) {
        let notification = Notification {
            id: uuid::Uuid::new_v4().to_string(),
            notification_type,
            title: title.into(),
            message: message.into(),
            timestamp: chrono::Utc::now(),
            read: false,
            persistent: false,
        };

        self.notifications.update(|n| {
            n.push_back(notification);
            // Keep only max_notifications
            while n.len() > self.max_notifications {
                n.pop_front();
            }
        });
    }

    /// Add a persistent notification (won't auto-dismiss)
    pub fn add_persistent(
        &self,
        notification_type: NotificationType,
        title: impl Into<String>,
        message: impl Into<String>,
    ) {
        let notification = Notification {
            id: uuid::Uuid::new_v4().to_string(),
            notification_type,
            title: title.into(),
            message: message.into(),
            timestamp: chrono::Utc::now(),
            read: false,
            persistent: true,
        };

        self.notifications.update(|n| {
            n.push_back(notification);
            while n.len() > self.max_notifications {
                n.pop_front();
            }
        });
    }

    /// Convenience methods
    pub fn info(&self, title: impl Into<String>, message: impl Into<String>) {
        self.add(NotificationType::Info, title, message);
    }

    pub fn success(&self, title: impl Into<String>, message: impl Into<String>) {
        self.add(NotificationType::Success, title, message);
    }

    pub fn warning(&self, title: impl Into<String>, message: impl Into<String>) {
        self.add(NotificationType::Warning, title, message);
    }

    pub fn error(&self, title: impl Into<String>, message: impl Into<String>) {
        self.add(NotificationType::Error, title, message);
    }

    /// Mark notification as read
    pub fn mark_read(&self, id: &str) {
        self.notifications.update(|notifications| {
            if let Some(n) = notifications.iter_mut().find(|n| n.id == id) {
                n.read = true;
            }
        });
    }

    /// Dismiss (remove) a notification
    pub fn dismiss(&self, id: &str) {
        self.notifications.update(|notifications| {
            notifications.retain(|n| n.id != id);
        });
    }

    /// Clear all notifications
    pub fn clear_all(&self) {
        self.notifications.set(VecDeque::new());
    }

    /// Get unread count
    pub fn unread_count(&self) -> usize {
        self.notifications
            .with(|n| n.iter().filter(|n| !n.read).count())
    }

    /// Get active (non-dismissed) notifications
    pub fn get_active(&self) -> Vec<Notification> {
        self.notifications.get().into_iter().collect()
    }

    /// Get unread notifications
    pub fn get_unread(&self) -> Vec<Notification> {
        self.notifications
            .with(|n| n.iter().filter(|notif| !notif.read).cloned().collect())
    }

    /// Get notifications by type
    pub fn get_by_type(&self, notification_type: NotificationType) -> Vec<Notification> {
        self.notifications.with(|n| {
            n.iter()
                .filter(|notif| notif.notification_type == notification_type)
                .cloned()
                .collect()
        })
    }

    /// Set auto-dismiss duration
    pub fn set_auto_dismiss_secs(&self, secs: u64) {
        self.auto_dismiss_secs.set(secs);
    }

    /// Mark all as read
    pub fn mark_all_read(&self) {
        self.notifications.update(|notifications| {
            for n in notifications.iter_mut() {
                n.read = true;
            }
        });
    }

    /// Clear read notifications
    pub fn clear_read(&self) {
        self.notifications.update(|notifications| {
            notifications.retain(|n| !n.read);
        });
    }

    /// Check if notification should auto-dismiss
    pub fn should_auto_dismiss(&self, notification: &Notification) -> bool {
        if notification.persistent {
            return false;
        }

        let elapsed = chrono::Utc::now() - notification.timestamp;
        let auto_dismiss = self.auto_dismiss_secs.get();

        elapsed.num_seconds() as u64 >= auto_dismiss
    }
}

impl Default for NotificationState {
    fn default() -> Self {
        Self::new()
    }
}

/// Provide notification state
pub fn provide_notification_state() {
    provide_context(NotificationState::new());
}

/// Use notification state
pub fn use_notification_state() -> NotificationState {
    use_context::<NotificationState>().expect("NotificationState not provided")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_notification() {
        let state = NotificationState::new();
        state.info("Test", "Message");

        assert_eq!(state.notifications.with(|n| n.len()), 1);
        assert_eq!(state.unread_count(), 1);
    }

    #[test]
    fn test_dismiss_notification() {
        let state = NotificationState::new();
        state.info("Test", "Message");

        let id = state.notifications.with(|n| n[0].id.clone());
        state.dismiss(&id);

        assert_eq!(state.notifications.with(|n| n.len()), 0);
    }

    #[test]
    fn test_max_notifications() {
        let state = NotificationState::new();
        for i in 0..60 {
            state.info(format!("Test {}", i), "Message");
        }

        assert_eq!(state.notifications.with(|n| n.len()), 50);
    }
}
