//! Markdown File Storage
//!
//! Implements OpenClaw's "File is Truth" architecture where all memories are
//! stored as human-readable, editable Markdown files rather than opaque
//! databases.
//!
//! # File Structure
//!
//! ```text
//! data/workspace/
//! ├── MEMORY.md              # Long-term core memories (user preferences, config)
//! ├── USER.md                # User profile and personal information
//! ├── SOUL.md                # AI personality and behavior guidelines
//! ├── AGENTS.md              # AI operation manual and workflows
//! ├── HEARTBEAT.md           # Periodic task configuration
//! └── memory/
//!     ├── 2026-04-06.md      # Daily conversation logs
//!     ├── 2026-04-05.md
//!     └── ...
//! ```
//!
//! # Features
//! - Pure Markdown storage for transparency and portability
//! - Automatic daily log rotation
//! - Structured frontmatter for metadata
//! - File watching for external edits
//! - Atomic writes for data integrity

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::{debug, info};
use uuid::Uuid;

use crate::error::Result;

/// Default workspace directory name
pub const DEFAULT_WORKSPACE_DIR: &str = "data";
/// Memory subdirectory name
pub const MEMORY_SUBDIR: &str = "memory";
/// Core memory file name
pub const CORE_MEMORY_FILE: &str = "MEMORY.md";
/// User profile file name
pub const USER_PROFILE_FILE: &str = "USER.md";
/// AI soul/personality file name
pub const SOUL_FILE: &str = "SOUL.md";
/// Agents manual file name
pub const AGENTS_MANUAL_FILE: &str = "AGENTS.md";
/// Heartbeat/tasks file name
pub const HEARTBEAT_FILE: &str = "HEARTBEAT.md";

/// Memory entry types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryFileType {
    /// Core long-term memory
    Core,
    /// Daily conversation log
    Daily,
    /// User profile
    User,
    /// AI personality
    Soul,
    /// Operation manual
    Agents,
    /// Periodic tasks
    Heartbeat,
}

impl MemoryFileType {
    /// Get the filename for this memory type
    pub fn filename(&self, date: Option<chrono::NaiveDate>) -> String {
        match self {
            MemoryFileType::Core => CORE_MEMORY_FILE.to_string(),
            MemoryFileType::Daily => {
                let date = date.expect("Daily memory requires a date");
                format!("{}.md", date.format("%Y-%m-%d"))
            }
            MemoryFileType::User => USER_PROFILE_FILE.to_string(),
            MemoryFileType::Soul => SOUL_FILE.to_string(),
            MemoryFileType::Agents => AGENTS_MANUAL_FILE.to_string(),
            MemoryFileType::Heartbeat => HEARTBEAT_FILE.to_string(),
        }
    }

    /// Get the subdirectory (if any) for this memory type
    pub fn subdirectory(&self) -> Option<&'static str> {
        match self {
            MemoryFileType::Daily => Some(MEMORY_SUBDIR),
            _ => None,
        }
    }
}

/// Markdown memory entry with frontmatter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkdownMemoryEntry {
    /// Unique entry ID
    pub id: Uuid,
    /// Entry title/heading
    pub title: String,
    /// Main content
    pub content: String,
    /// Entry timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Entry category
    pub category: String,
    /// Importance score (0.0 - 1.0)
    pub importance: f32,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
    /// Source session ID (if applicable)
    pub session_id: Option<String>,
}

impl MarkdownMemoryEntry {
    /// Create new memory entry
    pub fn new(title: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            title: title.into(),
            content: content.into(),
            timestamp: chrono::Utc::now(),
            category: "general".to_string(),
            importance: 0.5,
            metadata: HashMap::new(),
            session_id: None,
        }
    }

    /// Set category
    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = category.into();
        self
    }

    /// Set importance
    pub fn with_importance(mut self, importance: f32) -> Self {
        self.importance = importance.clamp(0.0, 1.0);
        self
    }

    /// Set session ID
    pub fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Convert to Markdown string with frontmatter
    pub fn to_markdown(&self) -> String {
        let mut output = String::new();

        // YAML frontmatter
        output.push_str("---\n");
        output.push_str(&format!("id: {}\n", self.id));
        output.push_str(&format!("timestamp: {}\n", self.timestamp.to_rfc3339()));
        output.push_str(&format!("category: {}\n", self.category));
        output.push_str(&format!("importance: {}\n", self.importance));

        if let Some(ref session_id) = self.session_id {
            output.push_str(&format!("session_id: {}\n", session_id));
        }

        if !self.metadata.is_empty() {
            output.push_str("metadata:\n");
            for (key, value) in &self.metadata {
                output.push_str(&format!("  {}: {}\n", key, value));
            }
        }

        output.push_str("---\n\n");

        // Content
        output.push_str(&format!("## {}\n\n", self.title));
        output.push_str(&self.content);
        output.push_str("\n\n");

        output
    }

    /// Parse from Markdown string
    pub fn from_markdown(text: &str) -> Result<Self> {
        let mut entry = Self::new("Untitled", "");
        let mut in_frontmatter = false;
        let mut frontmatter_end = 0;
        let mut content_start = 0;

        for (i, line) in text.lines().enumerate() {
            if line.trim() == "---" {
                if !in_frontmatter {
                    in_frontmatter = true;
                } else {
                    frontmatter_end = i;
                    content_start = i + 1;
                    break;
                }
            }
        }

        if in_frontmatter && frontmatter_end > 0 {
            // Parse frontmatter
            let frontmatter: &str = &text
                .lines()
                .take(frontmatter_end)
                .skip(1)
                .collect::<Vec<_>>()
                .join("\n");

            for line in frontmatter.lines() {
                if let Some((key, value)) = line.split_once(':') {
                    let key = key.trim();
                    let value = value.trim();

                    match key {
                        "id" => {
                            if let Ok(id) = Uuid::parse_str(value) {
                                entry.id = id;
                            }
                        }
                        "timestamp" => {
                            if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(value) {
                                entry.timestamp = ts.with_timezone(&chrono::Utc);
                            }
                        }
                        "category" => entry.category = value.to_string(),
                        "importance" => {
                            if let Ok(imp) = value.parse() {
                                entry.importance = imp;
                            }
                        }
                        "session_id" => entry.session_id = Some(value.to_string()),
                        _ => {
                            entry.metadata.insert(key.to_string(), value.to_string());
                        }
                    }
                }
            }
        }

        // Parse content
        let content_lines: Vec<_> = text.lines().skip(content_start).collect();

        // Extract title from first heading (skip empty lines)
        let mut title_line_idx = 0;
        for (idx, line) in content_lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                title_line_idx = idx + 1;
                continue;
            }
            if trimmed.starts_with("## ") {
                entry.title = trimmed[3..].trim().to_string();
                title_line_idx = idx + 1;
            }
            break;
        }

        // Join remaining lines as content (skip lines up to and including title)
        entry.content = content_lines
            .into_iter()
            .skip(title_line_idx)
            .collect::<Vec<_>>()
            .join("\n");
        entry.content = entry.content.trim().to_string();

        Ok(entry)
    }
}

/// Markdown storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkdownStorageConfig {
    /// Base workspace directory
    pub workspace_dir: PathBuf,
    /// Enable automatic daily log rotation
    pub enable_daily_rotation: bool,
    /// Maximum file size before rotation (bytes)
    pub max_file_size: usize,
    /// Enable file watching for external edits
    pub enable_file_watching: bool,
    /// Atomic write mode
    pub atomic_writes: bool,
}

impl Default for MarkdownStorageConfig {
    fn default() -> Self {
        Self {
            workspace_dir: PathBuf::from(DEFAULT_WORKSPACE_DIR).join("workspace"),
            enable_daily_rotation: true,
            max_file_size: 10 * 1024 * 1024, // 10MB
            enable_file_watching: true,
            atomic_writes: true,
        }
    }
}

/// Markdown-based memory storage
pub struct MarkdownStorage {
    config: MarkdownStorageConfig,
}

impl MarkdownStorage {
    /// Create new Markdown storage
    pub fn new(config: MarkdownStorageConfig) -> Result<Self> {
        Ok(Self { config })
    }

    /// Create with default configuration
    pub fn default() -> Result<Self> {
        Self::new(MarkdownStorageConfig::default())
    }

    /// Initialize workspace directories
    pub async fn initialize_workspace(&self) -> Result<()> {
        // Create main workspace
        fs::create_dir_all(&self.config.workspace_dir)
            .await
            .map_err(|e| {
                crate::error::AgentError::storage(format!(
                    "Failed to create workspace directory: {}",
                    e
                ))
            })?;

        // Create memory subdirectory
        let memory_dir = self.config.workspace_dir.join(MEMORY_SUBDIR);
        fs::create_dir_all(&memory_dir).await.map_err(|e| {
            crate::error::AgentError::storage(format!("Failed to create memory directory: {}", e))
        })?;

        // Initialize default files if they don't exist
        self.initialize_default_files().await?;

        info!(
            "Markdown storage workspace initialized at: {:?}",
            self.config.workspace_dir
        );
        Ok(())
    }

    /// Initialize default memory files with templates
    async fn initialize_default_files(&self) -> Result<()> {
        let files = [
            (CORE_MEMORY_FILE, self.core_memory_template()),
            (USER_PROFILE_FILE, self.user_profile_template()),
            (SOUL_FILE, self.soul_template()),
            (AGENTS_MANUAL_FILE, self.agents_manual_template()),
            (HEARTBEAT_FILE, self.heartbeat_template()),
        ];

        for (filename, template) in files {
            let path = self.config.workspace_dir.join(filename);
            if !path.exists() {
                info!("📝 Creating default memory file: {:?}", path);
                self.write_file_atomic(&path, &template).await?;
            } else {
                info!("📂 Memory file exists: {:?}", path);
            }
        }

        Ok(())
    }

    /// Write file atomically (write to temp, then rename)
    async fn write_file_atomic(&self, path: &Path, content: &str) -> Result<()> {
        if self.config.atomic_writes {
            let temp_path = path.with_extension("tmp");

            // Write to temp file
            let mut file = fs::File::create(&temp_path).await.map_err(|e| {
                crate::error::AgentError::storage(format!("Failed to create temp file: {}", e))
            })?;

            file.write_all(content.as_bytes()).await.map_err(|e| {
                crate::error::AgentError::storage(format!("Failed to write temp file: {}", e))
            })?;

            file.flush().await.map_err(|e| {
                crate::error::AgentError::storage(format!("Failed to flush temp file: {}", e))
            })?;

            drop(file);

            // Atomic rename
            fs::rename(&temp_path, path).await.map_err(|e| {
                crate::error::AgentError::storage(format!("Failed to rename temp file: {}", e))
            })?;
        } else {
            fs::write(path, content).await.map_err(|e| {
                crate::error::AgentError::storage(format!("Failed to write file: {}", e))
            })?;
        }

        Ok(())
    }

    /// Append entry to memory file
    pub async fn append_entry(
        &self,
        file_type: MemoryFileType,
        entry: &MarkdownMemoryEntry,
        date: Option<chrono::NaiveDate>,
    ) -> Result<()> {
        let path = self.get_file_path(file_type, date)?;

        // Ensure directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await.ok();
        }

        let markdown = entry.to_markdown();

        // Check file size for rotation
        if path.exists() {
            let metadata = fs::metadata(&path).await?;
            if metadata.len() as usize > self.config.max_file_size {
                self.rotate_file(&path).await?;
            }
        }

        // Append to file
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await
            .map_err(|e| {
                crate::error::AgentError::storage(format!("Failed to open memory file: {}", e))
            })?;

        file.write_all(markdown.as_bytes()).await.map_err(|e| {
            crate::error::AgentError::storage(format!("Failed to append to memory file: {}", e))
        })?;

        debug!("Appended entry to {:?}", path);
        Ok(())
    }

    /// Read all entries from a memory file
    pub async fn read_entries(
        &self,
        file_type: MemoryFileType,
        date: Option<chrono::NaiveDate>,
    ) -> Result<Vec<MarkdownMemoryEntry>> {
        let path = self.get_file_path(file_type, date)?;

        if !path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&path).await.map_err(|e| {
            crate::error::AgentError::storage(format!("Failed to read memory file: {}", e))
        })?;

        // Parse entries by looking for valid frontmatter + title patterns
        let mut entries = Vec::new();
        let lines: Vec<_> = content.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            // Look for frontmatter start (---)
            if lines[i].trim() == "---" {
                // Try to find a complete entry starting here
                if let Some((entry, next_idx)) = Self::try_parse_entry(&lines, i) {
                    entries.push(entry);
                    i = next_idx;
                    continue;
                }
            }
            i += 1;
        }

        Ok(entries)
    }

    /// Get file path for memory type
    fn get_file_path(
        &self,
        file_type: MemoryFileType,
        date: Option<chrono::NaiveDate>,
    ) -> Result<PathBuf> {
        let filename = file_type.filename(date);

        let path = match file_type.subdirectory() {
            Some(subdir) => self.config.workspace_dir.join(subdir).join(filename),
            None => self.config.workspace_dir.join(filename),
        };

        Ok(path)
    }

    /// Try to parse a single entry starting at given line index
    fn try_parse_entry(lines: &[&str], start_idx: usize) -> Option<(MarkdownMemoryEntry, usize)> {
        // Must start with ---
        if lines[start_idx].trim() != "---" {
            return None;
        }

        // Find frontmatter end (next ---)
        let mut frontmatter_end = None;
        let mut i = start_idx + 1;
        while i < lines.len() {
            if lines[i].trim() == "---" {
                frontmatter_end = Some(i);
                break;
            }
            i += 1;
        }

        let fm_end = frontmatter_end?;
        if fm_end == start_idx + 1 {
            // Empty frontmatter, not a valid entry
            return None;
        }

        // Find title (## Title)
        let mut title_line = None;
        i = fm_end + 1;
        while i < lines.len() {
            let trimmed = lines[i].trim();
            if trimmed.is_empty() {
                i += 1;
                continue;
            }
            if trimmed.starts_with("## ") {
                title_line = Some(i);
                break;
            }
            // If we hit another frontmatter start or end of content, stop
            if trimmed == "---" || !trimmed.is_empty() && !trimmed.starts_with("#") {
                break;
            }
            i += 1;
        }

        let title_idx = title_line?;

        // Reconstruct the entry markdown
        let mut entry_text = String::new();
        for j in start_idx..=fm_end {
            entry_text.push_str(lines[j]);
            entry_text.push('\n');
        }
        entry_text.push('\n');

        // Add title and content
        for j in title_idx..lines.len() {
            let trimmed = lines[j].trim();
            // Stop at next entry start
            if trimmed == "---" && j > title_idx {
                break;
            }
            entry_text.push_str(lines[j]);
            entry_text.push('\n');
        }
        entry_text.push('\n');

        match MarkdownMemoryEntry::from_markdown(&entry_text) {
            Ok(entry) => Some((entry, fm_end + 1)),
            Err(_) => None,
        }
    }

    /// Rotate a file (rename with timestamp)
    async fn rotate_file(&self, path: &Path) -> Result<()> {
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("memory");
        let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("md");

        let rotated_name = format!("{}_{}.{}", stem, timestamp, extension);
        let rotated_path = path.with_file_name(rotated_name);

        fs::rename(path, rotated_path).await.map_err(|e| {
            crate::error::AgentError::storage(format!("Failed to rotate file: {}", e))
        })?;

        info!("Rotated memory file: {:?}", path);
        Ok(())
    }

    /// Get recent daily logs
    pub async fn get_recent_daily_logs(
        &self,
        days: usize,
    ) -> Result<Vec<(chrono::NaiveDate, Vec<MarkdownMemoryEntry>)>> {
        let mut results = Vec::new();
        let today = chrono::Local::now().date_naive();

        for i in 0..days {
            let date = today - chrono::Duration::days(i as i64);
            let entries = self.read_entries(MemoryFileType::Daily, Some(date)).await?;
            if !entries.is_empty() {
                results.push((date, entries));
            }
        }

        Ok(results)
    }

    /// Search across all memory files
    pub async fn search(&self, query: &str) -> Result<Vec<SearchMatch>> {
        let mut matches = Vec::new();
        let lower_query = query.to_lowercase();

        // Search core memory
        if let Ok(entries) = self.read_entries(MemoryFileType::Core, None).await {
            for entry in entries {
                if let Some(score) = self.calculate_match_score(&entry, &lower_query) {
                    matches.push(SearchMatch {
                        entry,
                        file_type: MemoryFileType::Core,
                        relevance_score: score,
                    });
                }
            }
        }

        // Search user profile
        if let Ok(entries) = self.read_entries(MemoryFileType::User, None).await {
            for entry in entries {
                if let Some(score) = self.calculate_match_score(&entry, &lower_query) {
                    matches.push(SearchMatch {
                        entry,
                        file_type: MemoryFileType::User,
                        relevance_score: score,
                    });
                }
            }
        }

        // Search recent daily logs (last 7 days)
        for i in 0..7 {
            let date = chrono::Local::now().date_naive() - chrono::Duration::days(i);
            if let Ok(entries) = self.read_entries(MemoryFileType::Daily, Some(date)).await {
                for entry in entries {
                    if let Some(score) = self.calculate_match_score(&entry, &lower_query) {
                        matches.push(SearchMatch {
                            entry,
                            file_type: MemoryFileType::Daily,
                            relevance_score: score,
                        });
                    }
                }
            }
        }

        // Sort by relevance
        matches.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(matches)
    }

    /// Calculate match score for search
    fn calculate_match_score(&self, entry: &MarkdownMemoryEntry, query: &str) -> Option<f32> {
        let content_lower = entry.content.to_lowercase();
        let title_lower = entry.title.to_lowercase();
        let category_lower = entry.category.to_lowercase();

        let mut score = 0.0;

        // Title match (highest weight)
        if title_lower.contains(query) {
            score += 0.5;
        }

        // Category match
        if category_lower.contains(query) {
            score += 0.3;
        }

        // Content match
        if content_lower.contains(query) {
            score += 0.2;
            // Boost for multiple occurrences
            let occurrences = content_lower.matches(query).count();
            score += (occurrences as f32 * 0.05).min(0.2);
        }

        // Boost by importance
        score *= 0.5 + (entry.importance * 0.5);

        if score > 0.0 {
            Some(score.min(1.0))
        } else {
            None
        }
    }

    /// Get workspace directory
    pub fn workspace_dir(&self) -> &Path {
        &self.config.workspace_dir
    }

    // Template methods
    fn core_memory_template(&self) -> String {
        format!(
            r#"# Core Memory

This file contains permanent memories that the AI should always remember.

## Instructions

- Add facts about user preferences
- Store important configuration decisions
- Record critical project information
- One entry per section with frontmatter

---

id: {}
timestamp: {}
category: system
importance: 1.0
---

## Getting Started

This is your core memory file. Important information will be automatically saved here.

"#,
            Uuid::new_v4(),
            chrono::Utc::now().to_rfc3339()
        )
    }

    fn user_profile_template(&self) -> String {
        format!(
            r#"# User Profile

Personal information and preferences about the user.

---

id: {}
timestamp: {}
category: profile
importance: 1.0
---

## Basic Information

*To be filled as we learn about each other*

- Name: 
- Preferred language: 
- Timezone: 

## Preferences

- Communication style: 
- Notification preferences: 

## Interests & Expertise

- Professional background: 
- Technical skills: 
- Hobbies: 

"#,
            Uuid::new_v4(),
            chrono::Utc::now().to_rfc3339()
        )
    }

    fn soul_template(&self) -> String {
        format!(
            r#"# AI Soul

Personality configuration and behavior guidelines.

---

id: {}
timestamp: {}
category: personality
importance: 1.0
---

## Personality

- Helpful and friendly
- Professional but approachable
- Detail-oriented

## Communication Style

- Clear and concise
- Use examples when helpful
- Ask clarifying questions when needed

## Boundaries

- Respect user privacy
- Decline harmful requests
- Be honest about limitations

"#,
            Uuid::new_v4(),
            chrono::Utc::now().to_rfc3339()
        )
    }

    fn agents_manual_template(&self) -> String {
        format!(
            r#"# Agents Manual

Operation manual for AI workflows and procedures.

---

id: {}
timestamp: {}
category: manual
importance: 1.0
---

## Memory Management

### When to Save to Core Memory

- User explicitly says "remember"
- Important preferences are stated
- Critical configuration decisions

### Daily Log Format

Conversations are automatically logged to memory/YYYY-MM-DD.md

## Task Procedures

### Information Gathering

1. Identify what information is needed
2. Ask targeted questions
3. Confirm understanding

### Code Assistance

1. Understand the problem
2. Provide solution with explanation
3. Offer to explain further

"#,
            Uuid::new_v4(),
            chrono::Utc::now().to_rfc3339()
        )
    }

    fn heartbeat_template(&self) -> String {
        format!(
            r#"# Heartbeat Tasks

Periodic tasks and scheduled checks.

---

id: {}
timestamp: {}
category: tasks
importance: 0.8
---

## Daily Tasks

- [ ] Check for unread messages
- [ ] Review scheduled reminders

## Weekly Tasks

- [ ] Memory maintenance
- [ ] Archive old logs

## Custom Tasks

*Add your own periodic tasks here*

"#,
            Uuid::new_v4(),
            chrono::Utc::now().to_rfc3339()
        )
    }
}

/// Search match result
#[derive(Debug, Clone)]
pub struct SearchMatch {
    pub entry: MarkdownMemoryEntry,
    pub file_type: MemoryFileType,
    pub relevance_score: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_file_type_filename() {
        assert_eq!(MemoryFileType::Core.filename(None), "MEMORY.md");
        assert_eq!(MemoryFileType::User.filename(None), "USER.md");

        let date = chrono::NaiveDate::from_ymd_opt(2026, 4, 6).unwrap();
        assert_eq!(MemoryFileType::Daily.filename(Some(date)), "2026-04-06.md");
    }

    #[test]
    fn test_markdown_memory_entry_to_markdown() {
        let entry = MarkdownMemoryEntry::new("Test Title", "Test content")
            .with_category("test")
            .with_importance(0.8)
            .with_session_id("session-123")
            .with_metadata("key", "value");

        let markdown = entry.to_markdown();

        assert!(markdown.contains("id:"));
        assert!(markdown.contains("category: test"));
        assert!(markdown.contains("importance: 0.8"));
        assert!(markdown.contains("session_id: session-123"));
        assert!(markdown.contains("## Test Title"));
        assert!(markdown.contains("Test content"));
    }

    #[test]
    fn test_markdown_memory_entry_from_markdown() {
        let original = MarkdownMemoryEntry::new("Test Title", "Test content")
            .with_category("test")
            .with_importance(0.8);

        let markdown = original.to_markdown();
        let parsed = MarkdownMemoryEntry::from_markdown(&markdown).unwrap();

        assert_eq!(original.id, parsed.id);
        assert_eq!(original.title, parsed.title);
        assert_eq!(original.content, parsed.content);
        assert_eq!(original.category, parsed.category);
        assert_eq!(original.importance, parsed.importance);
    }

    #[test]
    fn test_markdown_storage_config_default() {
        let config = MarkdownStorageConfig::default();
        assert!(config.enable_daily_rotation);
        assert!(config.atomic_writes);
        assert_eq!(config.max_file_size, 10 * 1024 * 1024);
    }

    #[tokio::test]
    async fn test_markdown_storage_append_and_read() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config = MarkdownStorageConfig {
            workspace_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };

        let storage = MarkdownStorage::new(config).unwrap();
        storage.initialize_workspace().await.unwrap();

        let entry = MarkdownMemoryEntry::new("Test", "Content");
        storage
            .append_entry(MemoryFileType::Core, &entry, None)
            .await
            .unwrap();

        let entries = storage
            .read_entries(MemoryFileType::Core, None)
            .await
            .unwrap();

        // Filter to find our test entry (workspace has default template entries too)
        let test_entries: Vec<_> = entries
            .iter()
            .filter(|e| e.title == "Test" && e.content == "Content")
            .collect();

        assert!(
            !test_entries.is_empty(),
            "Test entry not found in {:?}",
            entries.iter().map(|e| &e.title).collect::<Vec<_>>()
        );
    }
}
