//! Message routing logic

use std::collections::HashMap;

use crate::error::{MessageBusError, Result};
use crate::Message;

/// Topic matcher supporting wildcards
#[derive(Debug, Clone)]
pub struct TopicMatcher;

impl TopicMatcher {
    /// Check if a topic matches a pattern
    ///
    /// Supports:
    /// - Exact match: "agent/123/task" matches "agent/123/task"
    /// - Single-level wildcard (+): "agent/+/task" matches "agent/123/task"
    /// - Multi-level wildcard (#): "agent/#" matches "agent/123/task/start"
    pub fn matches(pattern: &str, topic: &str) -> bool {
        // Handle empty cases
        if pattern.is_empty() || topic.is_empty() {
            return false;
        }

        let pattern_parts: Vec<&str> = pattern.split('/').collect();
        let topic_parts: Vec<&str> = topic.split('/').collect();

        let mut p_idx = 0;
        let mut t_idx = 0;

        while p_idx < pattern_parts.len() && t_idx < topic_parts.len() {
            match pattern_parts[p_idx] {
                // Multi-level wildcard matches everything remaining
                "#" => return true,
                // Single-level wildcard matches any single segment
                "+" => {
                    p_idx += 1;
                    t_idx += 1;
                }
                // Exact match required
                part => {
                    if part != topic_parts[t_idx] {
                        return false;
                    }
                    p_idx += 1;
                    t_idx += 1;
                }
            }
        }

        // Handle trailing # wildcard
        if p_idx < pattern_parts.len() && pattern_parts[p_idx] == "#" {
            return true;
        }

        // Both should be fully consumed
        p_idx == pattern_parts.len() && t_idx == topic_parts.len()
    }

    /// Extract named parameters from a topic
    ///
    /// Example:
    /// pattern: "agent/:id/task/:action"
    /// topic: "agent/123/task/start"
    /// returns: {"id": "123", "action": "start"}
    pub fn extract_params(pattern: &str, topic: &str) -> HashMap<String, String> {
        let mut params = HashMap::new();

        let pattern_parts: Vec<&str> = pattern.split('/').collect();
        let topic_parts: Vec<&str> = topic.split('/').collect();

        for (p, t) in pattern_parts.iter().zip(topic_parts.iter()) {
            if p.starts_with(':') {
                let key = p[1..].to_string();
                params.insert(key, t.to_string());
            }
        }

        params
    }

    /// Validate a topic pattern
    pub fn validate_pattern(pattern: &str) -> Result<()> {
        if pattern.is_empty() {
            return Err(MessageBusError::InvalidTopic(
                "Pattern cannot be empty".to_string(),
            ));
        }

        // Check for invalid combinations
        if pattern.contains("#/") {
            return Err(MessageBusError::InvalidTopic(
                "# must be the last segment".to_string(),
            ));
        }

        Ok(())
    }
}

/// Routing rule for message routing
#[derive(Debug, Clone)]
pub struct RouteRule {
    /// Pattern to match
    pub pattern: String,
    /// Target topics to route to
    pub targets: Vec<String>,
    /// Optional filter function name
    pub filter: Option<String>,
    /// Priority (higher = evaluated first)
    pub priority: i32,
    /// Whether to stop evaluating further rules
    pub stop_on_match: bool,
}

impl RouteRule {
    /// Create a new routing rule
    pub fn new(pattern: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
            targets: Vec::new(),
            filter: None,
            priority: 0,
            stop_on_match: false,
        }
    }

    /// Add a target topic
    pub fn target(mut self, topic: impl Into<String>) -> Self {
        self.targets.push(topic.into());
        self
    }

    /// Add multiple targets
    pub fn targets(mut self, topics: Vec<impl Into<String>>) -> Self {
        self.targets.extend(topics.into_iter().map(|t| t.into()));
        self
    }

    /// Set filter
    pub fn with_filter(mut self, filter: impl Into<String>) -> Self {
        self.filter = Some(filter.into());
        self
    }

    /// Set priority
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Stop on match
    pub fn stop_on_match(mut self) -> Self {
        self.stop_on_match = true;
        self
    }

    /// Check if this rule matches a topic
    pub fn matches(&self, topic: &str) -> bool {
        TopicMatcher::matches(&self.pattern, topic)
    }
}

/// Message router
#[derive(Debug, Default)]
pub struct Router {
    rules: Vec<RouteRule>,
}

impl Router {
    /// Create a new router
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// Add a routing rule
    pub fn add_rule(&mut self, rule: RouteRule) {
        self.rules.push(rule);
        // Sort by priority (higher first)
        self.rules.sort_by(|a, b| b.priority.cmp(&a.priority));
    }

    /// Route a message to target topics
    ///
    /// Returns a list of target topics the message should be delivered to
    pub fn route(&self, topic: &str, _message: &Message) -> Vec<String> {
        let mut targets = Vec::new();
        let mut stop = false;

        for rule in &self.rules {
            if stop {
                break;
            }

            if rule.matches(topic) {
                targets.extend(rule.targets.clone());
                stop = rule.stop_on_match;
            }
        }

        // Remove duplicates while preserving order
        let mut seen = std::collections::HashSet::new();
        targets.retain(|t| seen.insert(t.clone()));

        targets
    }

    /// Get all rules
    pub fn rules(&self) -> &[RouteRule] {
        &self.rules
    }

    /// Clear all rules
    pub fn clear(&mut self) {
        self.rules.clear();
    }

    /// Remove rules matching a pattern
    pub fn remove_rules(&mut self, pattern: &str) {
        self.rules.retain(|r| r.pattern != pattern);
    }
}

/// Content-based router
pub struct ContentRouter {
    content_rules: Vec<ContentRouteRule>,
}

/// Content-based routing rule
#[derive(Debug, Clone)]
pub struct ContentRouteRule {
    /// Field to match
    pub field: String,
    /// Expected value
    pub value: String,
    /// Target topic
    pub target: String,
}

impl ContentRouter {
    /// Create a new content router
    pub fn new() -> Self {
        Self {
            content_rules: Vec::new(),
        }
    }

    /// Add a content-based rule
    pub fn add_rule(
        &mut self,
        field: impl Into<String>,
        value: impl Into<String>,
        target: impl Into<String>,
    ) {
        self.content_rules.push(ContentRouteRule {
            field: field.into(),
            value: value.into(),
            target: target.into(),
        });
    }

    /// Route based on message content
    pub fn route(&self, message: &Message) -> Vec<String> {
        let mut targets = Vec::new();

        for rule in &self.content_rules {
            // Check headers
            if let Some(header_value) = message.metadata.get_header(&rule.field) {
                if header_value == rule.value {
                    targets.push(rule.target.clone());
                }
            }
        }

        targets
    }
}

impl Default for ContentRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topic_matcher_exact() {
        assert!(TopicMatcher::matches("agent/123/task", "agent/123/task"));
        assert!(!TopicMatcher::matches("agent/123/task", "agent/123/other"));
        assert!(!TopicMatcher::matches("agent/123/task", "agent/456/task"));
    }

    #[test]
    fn test_topic_matcher_single_wildcard() {
        assert!(TopicMatcher::matches("agent/+/task", "agent/123/task"));
        assert!(TopicMatcher::matches("agent/+/task", "agent/abc/task"));
        assert!(!TopicMatcher::matches("agent/+/task", "agent/123/other"));
        assert!(!TopicMatcher::matches(
            "agent/+/task",
            "agent/123/task/extra"
        ));
    }

    #[test]
    fn test_topic_matcher_multi_wildcard() {
        assert!(TopicMatcher::matches("agent/#", "agent/123"));
        assert!(TopicMatcher::matches("agent/#", "agent/123/task"));
        assert!(TopicMatcher::matches("agent/#", "agent/123/task/start"));
        assert!(!TopicMatcher::matches("agent/#", "other/123"));
        assert!(!TopicMatcher::matches("agent/#", "prefix/agent/123"));
    }

    #[test]
    fn test_topic_matcher_mixed() {
        assert!(TopicMatcher::matches(
            "agent/+/task/#",
            "agent/123/task/start"
        ));
        assert!(TopicMatcher::matches(
            "agent/+/task/#",
            "agent/123/task/start/progress"
        ));
        assert!(!TopicMatcher::matches(
            "agent/+/task/#",
            "agent/123/other/start"
        ));
    }

    #[test]
    fn test_extract_params() {
        let params = TopicMatcher::extract_params("agent/:id/task/:action", "agent/123/task/start");
        assert_eq!(params.get("id"), Some(&"123".to_string()));
        assert_eq!(params.get("action"), Some(&"start".to_string()));
    }

    #[test]
    fn test_route_rule() {
        let rule = RouteRule::new("agent/+/task/+")
            .target("worker/tasks")
            .with_priority(10)
            .stop_on_match();

        assert!(rule.matches("agent/123/task/start"));
        assert!(!rule.matches("agent/123/other/start"));
        assert_eq!(rule.priority, 10);
        assert!(rule.stop_on_match);
    }

    #[test]
    fn test_router() {
        let mut router = Router::new();

        // Add rules
        router.add_rule(
            RouteRule::new("agent/+/task/+")
                .target("worker/tasks")
                .with_priority(10),
        );
        router.add_rule(
            RouteRule::new("agent/#")
                .target("agent/logger")
                .with_priority(5),
        );
        router.add_rule(
            RouteRule::new("system/#")
                .target("system/monitor")
                .with_priority(20),
        );

        let message = Message::new("test", vec![]);

        // Test routing
        let targets = router.route("agent/123/task/start", &message);
        assert!(targets.contains(&"worker/tasks".to_string()));
        assert!(targets.contains(&"agent/logger".to_string()));

        let targets = router.route("system/metric/cpu", &message);
        assert!(targets.contains(&"system/monitor".to_string()));

        let targets = router.route("other/topic", &message);
        assert!(targets.is_empty());
    }

    #[test]
    fn test_router_deduplication() {
        let mut router = Router::new();

        // Add overlapping rules
        router.add_rule(
            RouteRule::new("agent/#")
                .target("target1")
                .with_priority(10),
        );
        router.add_rule(
            RouteRule::new("agent/+/task")
                .target("target1")
                .with_priority(5),
        );

        let message = Message::new("test", vec![]);
        let targets = router.route("agent/123/task", &message);

        // target1 should only appear once
        assert_eq!(targets.iter().filter(|&t| t == "target1").count(), 1);
    }

    #[test]
    fn test_router_stop_on_match() {
        let mut router = Router::new();

        router.add_rule(
            RouteRule::new("agent/+/task/+")
                .target("worker/tasks")
                .with_priority(10)
                .stop_on_match(),
        );
        router.add_rule(
            RouteRule::new("agent/#")
                .target("agent/logger")
                .with_priority(5),
        );

        let message = Message::new("test", vec![]);
        let targets = router.route("agent/123/task/start", &message);

        // Should only get worker/tasks because of stop_on_match
        assert_eq!(targets, vec!["worker/tasks"]);
    }

    #[test]
    fn test_content_router() {
        let mut router = ContentRouter::new();
        router.add_rule("x-priority", "high", "urgent/queue");
        router.add_rule("x-type", "metric", "metrics/collector");

        let mut msg1 = Message::new("test", vec![]);
        msg1.metadata
            .headers
            .insert("x-priority".to_string(), "high".to_string());

        let targets = router.route(&msg1);
        assert!(targets.contains(&"urgent/queue".to_string()));

        let mut msg2 = Message::new("test", vec![]);
        msg2.metadata
            .headers
            .insert("x-type".to_string(), "metric".to_string());

        let targets = router.route(&msg2);
        assert!(targets.contains(&"metrics/collector".to_string()));
    }

    #[test]
    fn test_validate_pattern() {
        assert!(TopicMatcher::validate_pattern("agent/+/task").is_ok());
        assert!(TopicMatcher::validate_pattern("agent/#").is_ok());
        assert!(TopicMatcher::validate_pattern("").is_err());
        assert!(TopicMatcher::validate_pattern("agent/#/task").is_err());
    }
}
