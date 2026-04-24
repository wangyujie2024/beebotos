//! Cognitive Module Tests
//!
//! Tests for working memory, goals, and cognitive state management.

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::*;

    // =============================================================================
    // WorkingMemory Tests
    // =============================================================================

    #[test]
    fn test_working_memory_creation() {
        let wm = WorkingMemory::new(10);
        assert_eq!(wm.items().len(), 0);
    }

    #[test]
    fn test_working_memory_add() {
        let mut wm = WorkingMemory::new(10);
        let item = MemoryItem {
            key: "test".to_string(),
            value: json!("value"),
            activation: 0.8,
            timestamp: 1000,
        };

        wm.add(item);
        assert_eq!(wm.items().len(), 1);
        assert!(wm.get("test").is_some());
    }

    #[test]
    fn test_working_memory_capacity_limit() {
        let mut wm = WorkingMemory::new(3);

        // Add 3 items
        for i in 0..3 {
            wm.add(MemoryItem {
                key: format!("key{}", i),
                value: json!(i),
                activation: 0.5 + (i as f32 * 0.1),
                timestamp: i as u64 * 100,
            });
        }

        assert_eq!(wm.items().len(), 3);

        // Add 4th item - should evict lowest activation
        wm.add(MemoryItem {
            key: "key3".to_string(),
            value: json!(3),
            activation: 0.9, // High activation
            timestamp: 400,
        });

        assert_eq!(wm.items().len(), 3);
        // Lowest activation item (key0 with 0.5) should be evicted
        assert!(wm.get("key0").is_none());
        assert!(wm.get("key3").is_some());
    }

    #[test]
    fn test_working_memory_get_nonexistent() {
        let wm = WorkingMemory::new(10);
        assert!(wm.get("nonexistent").is_none());
    }

    #[test]
    fn test_working_memory_decay() {
        let mut wm = WorkingMemory::new(10);
        wm.add(MemoryItem {
            key: "test".to_string(),
            value: json!("value"),
            activation: 0.11, // Low activation that will drop below 0.1 after decay
            timestamp: 1000,
        });

        wm.decay();

        // Item should be removed after decay (activation < 0.1 threshold)
        assert!(wm.get("test").is_none());
    }

    #[test]
    fn test_working_memory_decay_retention() {
        let mut wm = WorkingMemory::new(10);
        wm.add(MemoryItem {
            key: "test".to_string(),
            value: json!("value"),
            activation: 1.0, // High activation
            timestamp: 1000,
        });

        wm.decay();

        // High activation item should survive decay
        assert!(wm.get("test").is_some());
    }

    // =============================================================================
    // Goal Tests
    // =============================================================================

    #[test]
    fn test_goal_creation() {
        let goal = Goal::new("Test goal", 0.8);
        assert_eq!(goal.description, "Test goal");
        assert!((goal.priority - 0.8).abs() < 0.001);
        assert_eq!(goal.status, GoalStatus::Active);
        assert!(goal.subgoals.is_empty());
    }

    #[test]
    fn test_goal_with_deadline() {
        let goal = Goal::new("Test goal", 0.8).with_deadline(1234567890);
        assert_eq!(goal.deadline, Some(1234567890));
    }

    #[test]
    fn test_goal_default_no_deadline() {
        let goal = Goal::new("Test goal", 0.8);
        assert_eq!(goal.deadline, None);
    }

    // =============================================================================
    // CognitiveState Tests
    // =============================================================================

    #[test]
    fn test_cognitive_state_creation() {
        let state = CognitiveState::new();
        assert!(state.attention_focus.is_empty());
        assert!(state.goals.is_empty());
        assert!(state.current_intention.is_none());
    }

    #[test]
    fn test_cognitive_state_set_goal() {
        let mut state = CognitiveState::new();
        let goal = Goal::new("Test goal", 0.8);

        state.set_goal(goal);

        assert_eq!(state.goals.len(), 1);
        assert_eq!(state.goals[0].description, "Test goal");
    }

    #[test]
    fn test_cognitive_state_goals_sorted_by_priority() {
        let mut state = CognitiveState::new();

        state.set_goal(Goal::new("Low priority", 0.3));
        state.set_goal(Goal::new("High priority", 0.9));
        state.set_goal(Goal::new("Medium priority", 0.6));

        assert_eq!(state.goals[0].priority, 0.9);
        assert_eq!(state.goals[1].priority, 0.6);
        assert_eq!(state.goals[2].priority, 0.3);
    }

    #[test]
    fn test_cognitive_state_form_intention() {
        let mut state = CognitiveState::new();
        let goal = Goal::new("Test goal", 0.8);
        let goal_id = goal.id.clone();

        state.set_goal(goal);
        let intention = state.form_intention();

        assert!(intention.is_some());
        let intention = intention.unwrap();
        assert_eq!(intention.goal_id, goal_id);
        assert_eq!(intention.status, IntentionStatus::Formed);
        assert!(state.current_intention.is_some());
    }

    #[test]
    fn test_cognitive_state_form_intention_no_goals() {
        let mut state = CognitiveState::new();
        let intention = state.form_intention();

        assert!(intention.is_none());
        assert!(state.current_intention.is_none());
    }

    #[test]
    fn test_cognitive_state_memorize() {
        let mut state = CognitiveState::new();
        let item = MemoryItem {
            key: "test".to_string(),
            value: json!("value"),
            activation: 0.8,
            timestamp: 1000,
        };

        state.memorize(item);

        assert!(state.working_memory.get("test").is_some());
    }

    // =============================================================================
    // MemoryItem Tests
    // =============================================================================

    #[test]
    fn test_memory_item_creation() {
        let item = MemoryItem {
            key: "test_key".to_string(),
            value: json!({"field": "value"}),
            activation: 0.75,
            timestamp: 1234567890,
        };

        assert_eq!(item.key, "test_key");
        assert_eq!(item.value, json!({"field": "value"}));
        assert!((item.activation - 0.75).abs() < 0.001);
        assert_eq!(item.timestamp, 1234567890);
    }

    // =============================================================================
    // Action Tests
    // =============================================================================

    #[test]
    fn test_action_creation() {
        let action = Action::new("test_action");
        assert_eq!(action.name, "test_action");
        assert!(action.params.is_empty());
        assert!(action.preconditions.is_empty());
        assert!(action.effects.is_empty());
    }

    #[test]
    fn test_action_with_param() {
        let action = Action::new("test_action")
            .with_param("key1", "value1")
            .with_param("key2", 42);

        assert_eq!(action.params.get("key1"), Some(&json!("value1")));
        assert_eq!(action.params.get("key2"), Some(&json!(42)));
    }

    // =============================================================================
    // Belief Tests
    // =============================================================================

    #[test]
    fn test_belief_creation() {
        let belief = Belief::new("It is raining", 0.9);

        assert_eq!(belief.proposition, "It is raining");
        assert!((belief.confidence - 0.9).abs() < 0.001);
        assert_eq!(belief.source, BeliefSource::Inference);
        assert!(belief.timestamp > 0);
    }

    #[test]
    fn test_belief_with_different_confidence() {
        let belief_low = Belief::new("Something uncertain", 0.3);
        let belief_high = Belief::new("Something certain", 0.99);

        assert!((belief_low.confidence - 0.3).abs() < 0.001);
        assert!((belief_high.confidence - 0.99).abs() < 0.001);
    }

    // =============================================================================
    // CognitiveError Tests
    // =============================================================================

    #[test]
    fn test_cognitive_error_display() {
        let error = CognitiveError::InvalidState("test state".to_string());
        let display = format!("{}", error);
        assert!(display.contains("test state"));
        assert!(display.contains("Invalid state"));
    }

    #[test]
    fn test_cognitive_error_planning_failed() {
        let error = CognitiveError::PlanningFailed("no path found".to_string());
        let display = format!("{}", error);
        assert!(display.contains("no path found"));
    }

    #[test]
    fn test_cognitive_error_memory_full() {
        let error = CognitiveError::MemoryFull;
        let display = format!("{}", error);
        assert!(display.contains("Working memory full"));
    }

    #[test]
    fn test_cognitive_error_goal_conflict() {
        let error = CognitiveError::GoalConflict("conflicting goals".to_string());
        let display = format!("{}", error);
        assert!(display.contains("conflicting goals"));
    }

    // =============================================================================
    // Integration Tests (within module)
    // =============================================================================

    #[test]
    fn test_full_cognitive_workflow() {
        let mut state = CognitiveState::new();

        // 1. Set goals
        state.set_goal(Goal::new("Complete task", 0.9));
        state.set_goal(Goal::new("Review code", 0.7));

        // 2. Memorize information
        state.memorize(MemoryItem {
            key: "task_info".to_string(),
            value: json!({"id": 123, "name": "Important task"}),
            activation: 0.8,
            timestamp: 1000,
        });

        // 3. Form intention from top goal
        let intention = state.form_intention();
        assert!(intention.is_some());
        assert_eq!(state.goals[0].description, "Complete task");

        // 4. Verify working memory still has item
        assert!(state.working_memory.get("task_info").is_some());
    }

    #[test]
    fn test_working_memory_priority_eviction() {
        let mut wm = WorkingMemory::new(2);

        // Add two items with different activations
        wm.add(MemoryItem {
            key: "high_priority".to_string(),
            value: json!("important"),
            activation: 0.9,
            timestamp: 1000,
        });

        wm.add(MemoryItem {
            key: "low_priority".to_string(),
            value: json!("less important"),
            activation: 0.1,
            timestamp: 2000,
        });

        // Add third item - should evict low_priority
        wm.add(MemoryItem {
            key: "medium_priority".to_string(),
            value: json!("medium"),
            activation: 0.5,
            timestamp: 3000,
        });

        assert!(wm.get("high_priority").is_some());
        assert!(wm.get("low_priority").is_none()); // Evicted
        assert!(wm.get("medium_priority").is_some());
    }
}
