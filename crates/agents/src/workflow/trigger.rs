//! Workflow Trigger Engine
//!
//! Manages cron schedules, event subscriptions, and webhook routes
//! for triggering workflow execution.

use std::collections::HashMap;

use serde_json::Value;
use tokio::sync::mpsc;
use tracing::info;

use crate::workflow::{WorkflowDefinition, WorkflowId};

/// Trigger match result
#[derive(Debug, Clone)]
pub struct TriggerMatch {
    pub workflow_id: WorkflowId,
    pub trigger_context: Value,
    /// Optional auth token for webhook triggers
    pub auth: Option<String>,
}

/// Trigger engine managing all active triggers
#[derive(Debug, Clone, Default)]
pub struct TriggerEngine {
    /// Event source → workflow_id mappings
    event_subscriptions: HashMap<String, Vec<EventSubscription>>,
    /// Webhook path → workflow_id mappings
    webhook_routes: HashMap<String, WebhookRoute>,
    /// Manual trigger names → workflow_id
    manual_triggers: HashMap<String, WorkflowId>,
}

#[derive(Debug, Clone)]
struct EventSubscription {
    filter: Option<String>,
    workflow_id: WorkflowId,
}

#[derive(Debug, Clone)]
struct WebhookRoute {
    method: String,
    auth: Option<String>,
    workflow_id: WorkflowId,
}

impl TriggerEngine {
    /// Create new trigger engine
    pub fn new() -> Self {
        Self::default()
    }

    /// Register all triggers from a workflow definition
    pub fn register(&mut self, def: &WorkflowDefinition) {
        for trigger in &def.triggers {
            match &trigger.trigger_type {
                crate::workflow::definition::TriggerType::Cron { schedule, timezone } => {
                    // Cron scheduling is managed by tokio-cron-scheduler in the Gateway layer.
                    // TriggerEngine only tracks non-cron triggers for matching purposes.
                    info!(
                        "Registered cron trigger for workflow {}: {} ({})",
                        def.id, schedule, timezone.as_deref().unwrap_or("UTC")
                    );
                }
                crate::workflow::definition::TriggerType::Event { source, filter } => {
                    self.event_subscriptions
                        .entry(source.clone())
                        .or_default()
                        .push(EventSubscription {
                            filter: filter.clone(),
                            workflow_id: def.id.clone(),
                        });
                    info!(
                        "Registered event trigger for workflow {}: source={}",
                        def.id, source
                    );
                }
                crate::workflow::definition::TriggerType::Webhook { path, method, auth } => {
                    self.webhook_routes.insert(
                        path.clone(),
                        WebhookRoute {
                            method: method.clone(),
                            auth: auth.clone(),
                            workflow_id: def.id.clone(),
                        },
                    );
                    info!(
                        "Registered webhook trigger for workflow {}: {} {}",
                        def.id, method, path
                    );
                }
                crate::workflow::definition::TriggerType::Manual { .. } => {
                    self.manual_triggers.insert(def.name.clone(), def.id.clone());
                    self.manual_triggers.insert(def.id.clone(), def.id.clone());
                    info!("Registered manual trigger for workflow {}", def.id);
                }
            }
        }
    }

    /// Unregister all triggers for a workflow
    pub fn unregister(&mut self, workflow_id: &str) {
        self.event_subscriptions.retain(|_, subs| {
            subs.retain(|s| s.workflow_id != workflow_id);
            !subs.is_empty()
        });
        self.webhook_routes.retain(|_, route| route.workflow_id != workflow_id);
        self.manual_triggers.retain(|_, id| id != workflow_id);
    }

    /// Match a manual trigger by name or ID
    pub fn match_manual(&self, name: &str) -> Option<TriggerMatch> {
        self.manual_triggers.get(name).map(|workflow_id| TriggerMatch {
            workflow_id: workflow_id.clone(),
            trigger_context: serde_json::json!({"trigger_type": "manual", "name": name}),
            auth: None,
        })
    }

    /// Match a webhook request
    pub fn match_webhook(&self, path: &str, method: &str) -> Option<TriggerMatch> {
        self.webhook_routes.get(path).and_then(|route| {
            if route.method.eq_ignore_ascii_case(method) {
                Some(TriggerMatch {
                    workflow_id: route.workflow_id.clone(),
                    trigger_context: serde_json::json!({
                        "trigger_type": "webhook",
                        "path": path,
                        "method": method
                    }),
                    auth: route.auth.clone(),
                })
            } else {
                None
            }
        })
    }

    /// Match an event
    pub fn match_event(&self, source: &str, payload: &Value) -> Vec<TriggerMatch> {
        let mut matches = Vec::new();
        if let Some(subs) = self.event_subscriptions.get(source) {
            for sub in subs {
                let matched = if let Some(filter) = &sub.filter {
                    Self::evaluate_event_filter(filter, payload)
                } else {
                    true
                };

                if matched {
                    matches.push(TriggerMatch {
                        workflow_id: sub.workflow_id.clone(),
                        trigger_context: serde_json::json!({
                            "trigger_type": "event",
                            "source": source,
                            "payload": payload
                        }),
                        auth: None,
                    });
                }
            }
        }
        matches
    }

    /// Evaluate an event filter expression against a payload.
    ///
    /// Supported filter syntax:
    /// - `$.path == "value"` — JSONPath equality
    /// - `$.path > 100`      — numeric comparison
    /// - `$.path`            — truthiness check
    /// - `plain text`        — fallback: payload string contains
    fn evaluate_event_filter(filter: &str, payload: &Value) -> bool {
        let trimmed = filter.trim();

        // Try JSONPath expression: $.path op value
        // Match operators: ==, !=, <, >, <=, >=
        let re = regex::Regex::new(r"^\$\.(?P<path>[\w.\[\]]+)\s*(?P<op>==|!=|<=|>=|<|>)\s*(?P<value>.+)$").unwrap();
        if let Some(caps) = re.captures(trimmed) {
            let path = caps.name("path").unwrap().as_str();
            let op = caps.name("op").unwrap().as_str();
            let value = caps.name("value").unwrap().as_str().trim();

            let actual = match crate::workflow::template::resolve_json_path_internal(payload, path) {
                Ok(v) => v,
                Err(_) => return false,
            };

            // Try numeric comparison first
            if let (Ok(a), Ok(v)) = (actual.parse::<f64>(), value.parse::<f64>()) {
                return match op {
                    "==" => (a - v).abs() < f64::EPSILON,
                    "!=" => (a - v).abs() >= f64::EPSILON,
                    "<" => a < v,
                    ">" => a > v,
                    "<=" => a <= v,
                    ">=" => a >= v,
                    _ => false,
                };
            }

            // String comparison
            let unquoted = value.trim_matches('"');
            return match op {
                "==" => actual == unquoted,
                "!=" => actual != unquoted,
                _ => false, // string < > not supported
            };
        }

        // Try truthiness check: $.path
        if trimmed.starts_with("$.") {
            let path = &trimmed[2..];
            return match crate::workflow::template::resolve_json_path_internal(payload, path) {
                Ok(v) => !v.is_empty() && v != "null" && v != "false",
                Err(_) => false,
            };
        }

        // Fallback: plain string contains
        payload.to_string().contains(trimmed)
    }

    /// Get all registered webhook routes (for HTTP server registration)
    pub fn webhook_paths(&self) -> Vec<(String, String, WorkflowId)> {
        self.webhook_routes
            .iter()
            .map(|(path, route)| (path.clone(), route.method.clone(), route.workflow_id.clone()))
            .collect()
    }

    /// Start an async event listener that receives events from the AgentEventBus
    /// and returns trigger matches for workflows that should be executed.
    ///
    /// The caller should `.await` on the returned receiver and execute matched workflows.
    pub async fn listen_events(
        &self,
        event_bus: crate::events::AgentEventBus,
    ) -> mpsc::UnboundedReceiver<Vec<TriggerMatch>> {
        let mut rx = event_bus.subscribe("workflow_trigger_engine").await;
        let (tx, result_rx) = mpsc::unbounded_channel();
        let engine = self.clone();

        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                let (source, payload) = engine.event_to_source_payload(&event);
                let matches = engine.match_event(&source, &payload);
                if !matches.is_empty() {
                    if tx.send(matches).is_err() {
                        break;
                    }
                }
            }
        });

        result_rx
    }

    /// Convert a core Event to (source, payload) for match_event
    fn event_to_source_payload(&self, event: &beebotos_core::event::Event) -> (String, Value) {
        match event {
            beebotos_core::event::Event::AgentLifecycle { .. } => {
                ("agent.lifecycle".to_string(), serde_json::to_value(event).unwrap_or_default())
            }
            beebotos_core::event::Event::AgentSpawned { .. } => {
                ("agent.spawned".to_string(), serde_json::to_value(event).unwrap_or_default())
            }
            beebotos_core::event::Event::MemoryConsolidated { .. } => {
                ("memory.consolidated".to_string(), serde_json::to_value(event).unwrap_or_default())
            }
            beebotos_core::event::Event::BlockchainTx { .. } => {
                ("blockchain.tx".to_string(), serde_json::to_value(event).unwrap_or_default())
            }
            beebotos_core::event::Event::DaoProposalCreated { .. } => {
                ("dao.proposal_created".to_string(), serde_json::to_value(event).unwrap_or_default())
            }
            beebotos_core::event::Event::DaoVoteCast { .. } => {
                ("dao.vote_cast".to_string(), serde_json::to_value(event).unwrap_or_default())
            }
            beebotos_core::event::Event::SkillExecuted { .. } => {
                ("skill.executed".to_string(), serde_json::to_value(event).unwrap_or_default())
            }
            beebotos_core::event::Event::Metric { .. } => {
                ("system.metric".to_string(), serde_json::to_value(event).unwrap_or_default())
            }
            beebotos_core::event::Event::TaskStarted { .. } => {
                ("task.started".to_string(), serde_json::to_value(event).unwrap_or_default())
            }
            beebotos_core::event::Event::TaskCompleted { .. } => {
                ("task.completed".to_string(), serde_json::to_value(event).unwrap_or_default())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_def() -> WorkflowDefinition {
        WorkflowDefinition {
            id: "test_wf".to_string(),
            name: "Test Workflow".to_string(),
            description: "Test".to_string(),
            version: "1.0".to_string(),
            author: None,
            tags: vec![],
            triggers: vec![
                crate::workflow::definition::TriggerDefinition {
                    trigger_type: crate::workflow::definition::TriggerType::Manual,
                },
                crate::workflow::definition::TriggerDefinition {
                    trigger_type: crate::workflow::definition::TriggerType::Webhook {
                        path: "/webhook/test".to_string(),
                        method: "POST".to_string(),
                        auth: None,
                    },
                },
            ],
            config: crate::workflow::definition::WorkflowGlobalConfig::default(),
            steps: vec![],
        }
    }

    #[test]
    fn test_manual_trigger() {
        let mut engine = TriggerEngine::new();
        engine.register(&sample_def());

        let matched = engine.match_manual("Test Workflow");
        assert!(matched.is_some());
        assert_eq!(matched.unwrap().workflow_id, "test_wf");

        let matched = engine.match_manual("test_wf");
        assert!(matched.is_some());
    }

    #[test]
    fn test_webhook_trigger() {
        let mut engine = TriggerEngine::new();
        engine.register(&sample_def());

        let matched = engine.match_webhook("/webhook/test", "POST");
        assert!(matched.is_some());

        let not_matched = engine.match_webhook("/webhook/test", "GET");
        assert!(not_matched.is_none());
    }

    #[test]
    fn test_event_filter_jsonpath_equality() {
        let mut engine = TriggerEngine::new();
        let mut def = sample_def();
        def.triggers.push(crate::workflow::definition::TriggerDefinition {
            trigger_type: crate::workflow::definition::TriggerType::Event {
                source: "skill.executed".to_string(),
                filter: Some("$.status == \"completed\"".to_string()),
            },
        });
        engine.register(&def);

        let payload = serde_json::json!({"status": "completed", "skill": "test"});
        let matches = engine.match_event("skill.executed", &payload);
        assert_eq!(matches.len(), 1);

        let payload2 = serde_json::json!({"status": "failed", "skill": "test"});
        let matches2 = engine.match_event("skill.executed", &payload2);
        assert!(matches2.is_empty());
    }

    #[test]
    fn test_event_filter_jsonpath_numeric() {
        let mut engine = TriggerEngine::new();
        let mut def = sample_def();
        def.triggers.push(crate::workflow::definition::TriggerDefinition {
            trigger_type: crate::workflow::definition::TriggerType::Event {
                source: "system.metric".to_string(),
                filter: Some("$.value > 100".to_string()),
            },
        });
        engine.register(&def);

        let payload = serde_json::json!({"value": 150, "name": "cpu"});
        assert_eq!(engine.match_event("system.metric", &payload).len(), 1);

        let payload2 = serde_json::json!({"value": 50, "name": "cpu"});
        assert!(engine.match_event("system.metric", &payload2).is_empty());
    }

    #[test]
    fn test_event_filter_jsonpath_truthiness() {
        let mut engine = TriggerEngine::new();
        let mut def = sample_def();
        def.triggers.push(crate::workflow::definition::TriggerDefinition {
            trigger_type: crate::workflow::definition::TriggerType::Event {
                source: "agent.lifecycle".to_string(),
                filter: Some("$.enabled".to_string()),
            },
        });
        engine.register(&def);

        let payload = serde_json::json!({"enabled": true});
        assert_eq!(engine.match_event("agent.lifecycle", &payload).len(), 1);

        let payload2 = serde_json::json!({"enabled": false});
        assert!(engine.match_event("agent.lifecycle", &payload2).is_empty());
    }

    #[test]
    fn test_event_filter_plain_contains() {
        let mut engine = TriggerEngine::new();
        let mut def = sample_def();
        def.triggers.push(crate::workflow::definition::TriggerDefinition {
            trigger_type: crate::workflow::definition::TriggerType::Event {
                source: "test.source".to_string(),
                filter: Some("alert".to_string()),
            },
        });
        engine.register(&def);

        let payload = serde_json::json!({"message": "critical alert triggered"});
        assert_eq!(engine.match_event("test.source", &payload).len(), 1);

        let payload2 = serde_json::json!({"message": "all good"});
        assert!(engine.match_event("test.source", &payload2).is_empty());
    }
}
