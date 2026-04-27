//! Progress indicators for BeeBotOS CLI
//!
//! Provides visual feedback for long-running operations.

#![allow(dead_code)]

use std::time::Duration;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

/// Create a spinner for indeterminate progress
pub fn create_spinner(message: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
    );
    pb.set_message(message.to_string());
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}

/// Create a progress bar for determinate operations
pub fn create_progress_bar(total: u64, message: &str) -> ProgressBar {
    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("#>-"),
    );
    pb.set_message(message.to_string());
    pb
}

/// Create a multi-progress bar for parallel operations
pub fn create_multi_progress() -> MultiProgress {
    MultiProgress::new()
}

/// Task progress tracker
pub struct TaskProgress {
    pb: ProgressBar,
    name: String,
}

impl TaskProgress {
    /// Create a new task progress tracker
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        let pb = create_spinner(&format!("{}...", name));
        Self { pb, name }
    }

    /// Mark task as completed
    pub fn finish_success(self, message: Option<&str>) {
        let msg = message.unwrap_or("done");
        self.pb
            .finish_with_message(format!("✓ {} ({})", self.name, msg));
    }

    /// Mark task as failed
    pub fn finish_error(self, message: &str) {
        self.pb
            .finish_with_message(format!("✗ {} ({})", self.name, message));
    }

    /// Set progress message
    pub fn set_message(&self, message: &str) {
        self.pb.set_message(format!("{}: {}", self.name, message));
    }

    /// Increment progress
    pub fn inc(&self, delta: u64) {
        self.pb.inc(delta);
    }

    /// Set position
    pub fn set_position(&self, pos: u64) {
        self.pb.set_position(pos);
    }
}

impl Drop for TaskProgress {
    fn drop(&mut self) {
        if !self.pb.is_finished() {
            self.pb.finish_and_clear();
        }
    }
}

/// Execute a function with progress indication
pub async fn with_spinner<T, F, Fut>(message: &str, f: F) -> T
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    let pb = create_spinner(message);
    let result = f().await;
    pb.finish_and_clear();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spinner_creation() {
        let pb = create_spinner("Testing");
        assert!(!pb.is_finished());
        pb.finish_and_clear();
    }

    #[test]
    fn test_progress_bar_creation() {
        let pb = create_progress_bar(100, "Testing");
        assert_eq!(pb.length(), Some(100));
        pb.finish_and_clear();
    }

    #[test]
    fn test_task_progress() {
        let task = TaskProgress::new("Test Task");
        task.set_message("working");
        task.finish_success(Some("completed"));
    }
}
