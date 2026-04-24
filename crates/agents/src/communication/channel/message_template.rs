//! Message Template System - Dynamic Message Generation
//!
//! 🔧 P1 FIX: Template system for WeChat and other channels

use std::collections::HashMap;

use regex::Regex;

use crate::error::{AgentError, Result};

/// Message template with variable substitution
#[derive(Debug, Clone)]
pub struct MessageTemplate {
    pub id: String,
    pub content: String,
    pub description: Option<String>,
    pub defaults: HashMap<String, String>,
}

impl MessageTemplate {
    pub fn new(id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            content: content.into(),
            description: None,
            defaults: HashMap::new(),
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn with_default(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.defaults.insert(key.into(), value.into());
        self
    }

    /// Render template with variables
    pub fn render(&self, vars: &HashMap<String, String>) -> String {
        let mut result = self.content.clone();
        let mut merged = self.defaults.clone();
        merged.extend(vars.clone());

        // Replace variables {{variable}} or {{variable:default}}
        let var_regex = Regex::new(r"\{\{(\w+)(?::([^}]+))?\}\}").unwrap();
        result = var_regex
            .replace_all(&result, |caps: &regex::Captures| {
                let var_name = &caps[1];
                let default_value = caps.get(2).map(|m| m.as_str()).unwrap_or("");
                merged
                    .get(var_name)
                    .cloned()
                    .unwrap_or_else(|| default_value.to_string())
            })
            .to_string();

        self.process_functions(&result)
    }

    fn process_functions(&self, content: &str) -> String {
        let mut result = content.to_string();

        // {{date}} - Current date
        let date_regex = Regex::new(r"\{\{date\}\}").unwrap();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        result = date_regex.replace_all(&result, &today).to_string();

        // {{time}} - Current time
        let time_regex = Regex::new(r"\{\{time\}\}").unwrap();
        let now = chrono::Local::now().format("%H:%M:%S").to_string();
        result = time_regex.replace_all(&result, &now).to_string();

        // {{datetime}} - Current date and time
        let datetime_regex = Regex::new(r"\{\{datetime\}\}").unwrap();
        let datetime = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        result = datetime_regex.replace_all(&result, &datetime).to_string();

        result.replace("{{newline}}", "\n")
    }

    /// Get required variables (without defaults)
    pub fn required_variables(&self) -> Vec<String> {
        let var_regex = Regex::new(r"\{\{(\w+)(?::[^}]+)?\}\}").unwrap();
        let mut required = Vec::new();

        for caps in var_regex.captures_iter(&self.content) {
            let var_name = caps[1].to_string();
            let has_default_in_template = caps.get(2).is_some();
            let has_default = self.defaults.contains_key(&var_name);

            if !has_default_in_template && !has_default {
                required.push(var_name);
            }
        }

        required.sort();
        required.dedup();
        required
    }

    /// Extract all variable names
    pub fn variables(&self) -> Vec<String> {
        let var_regex = Regex::new(r"\{\{(\w+)(?::[^}]+)?\}\}").unwrap();
        let mut vars = Vec::new();

        for caps in var_regex.captures_iter(&self.content) {
            vars.push(caps[1].to_string());
        }

        vars.sort();
        vars.dedup();
        vars
    }
}

/// Template manager for organizing templates
pub struct TemplateManager {
    templates: HashMap<String, MessageTemplate>,
    global_defaults: HashMap<String, String>,
}

impl TemplateManager {
    pub fn new() -> Self {
        Self {
            templates: HashMap::new(),
            global_defaults: HashMap::new(),
        }
    }

    pub fn with_defaults() -> Self {
        let mut manager = Self::new();
        manager.register_defaults();
        manager
    }

    pub fn register(&mut self, template: MessageTemplate) {
        self.templates.insert(template.id.clone(), template);
    }

    pub fn get(&self, id: &str) -> Option<&MessageTemplate> {
        self.templates.get(id)
    }

    pub fn render(&self, template_id: &str, vars: &HashMap<String, String>) -> Result<String> {
        let template = self.templates.get(template_id).ok_or_else(|| {
            AgentError::not_found(format!("Template '{}' not found", template_id))
        })?;

        let mut merged = self.global_defaults.clone();
        merged.extend(vars.clone());

        Ok(template.render(&merged))
    }

    pub fn set_global(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.global_defaults.insert(key.into(), value.into());
    }

    pub fn list_templates(&self) -> Vec<&String> {
        self.templates.keys().collect()
    }

    /// Register default templates
    fn register_defaults(&mut self) {
        self.register(
            MessageTemplate::new(
                "welcome",
                "Welcome to {{service_name}}!\n\nHello {{user_name}}, I'm your AI assistant.\n\nI \
                 can help you:\n• Answer questions\n• Summarize links\n• Execute tasks\n\nSend \
                 /help for available commands.",
            )
            .with_default("service_name", "BeeBotOS"),
        );

        self.register(MessageTemplate::new(
            "task_complete",
            "Task Complete: {{task_name}}\nStatus: {{status}}\nTime: {{datetime}}",
        ));

        self.register(MessageTemplate::new(
            "status_report",
            "System Status Report\nTime: {{datetime}}\nAgents: {{agent_count}}\nTasks: \
             {{task_count}}",
        ));

        self.register(MessageTemplate::new(
            "link_summary",
            "{{title}}\n\nSummary:\n{{summary}}\n\nURL: {{url}}",
        ));
    }
}

impl Default for TemplateManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Template builder for fluent API
pub struct TemplateBuilder {
    id: String,
    content: String,
    description: Option<String>,
    defaults: HashMap<String, String>,
}

impl TemplateBuilder {
    /// Start building a template
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            content: String::new(),
            description: None,
            defaults: HashMap::new(),
        }
    }

    /// Set template content
    pub fn content(mut self, content: impl Into<String>) -> Self {
        self.content = content.into();
        self
    }

    /// Set description
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Add default variable
    pub fn default(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.defaults.insert(key.into(), value.into());
        self
    }

    /// Build the template
    pub fn build(self) -> MessageTemplate {
        MessageTemplate {
            id: self.id,
            content: self.content,
            description: self.description,
            defaults: self.defaults,
        }
    }
}
