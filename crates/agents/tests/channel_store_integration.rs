//! SQLite store integration tests for UserChannel and AgentChannel stores.

use std::sync::Arc;

use beebotos_agents::communication::agent_channel::{AgentChannelBinding, RoutingRules};
use beebotos_agents::communication::user_channel::{ChannelBindingStatus, UserChannelBinding};
use beebotos_agents::communication::PlatformType;
use beebotos_agents::services::{
    AgentChannelBindingStore, SqliteAgentChannelBindingStore, SqliteUserChannelStore,
    UserChannelStore,
};
use sqlx::SqlitePool;

async fn setup_db() -> SqlitePool {
    let db = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("connect to in-memory sqlite");

    sqlx::migrate!("../../migrations_sqlite")
        .run(&db)
        .await
        .expect("run migrations");

    // Seed a dummy user and agent to satisfy FK constraints when enabled
    sqlx::query("INSERT INTO users (id, username, email, password_hash) VALUES (?1, ?2, ?3, ?4)")
        .bind("user-1")
        .bind("testuser")
        .bind("test@example.com")
        .bind("secret")
        .execute(&db)
        .await
        .unwrap();

    sqlx::query("INSERT INTO agents (id, name) VALUES (?1, ?2)")
        .bind("agent-1")
        .bind("Test Agent")
        .execute(&db)
        .await
        .unwrap();

    db
}

#[tokio::test]
async fn test_user_channel_store_lifecycle() {
    let db = setup_db().await;
    let store: Arc<dyn UserChannelStore> = Arc::new(SqliteUserChannelStore::new(db));

    let binding = UserChannelBinding {
        id: "uc-1".to_string(),
        user_id: "user-1".to_string(),
        platform: PlatformType::Lark,
        instance_name: "default".to_string(),
        platform_user_id: Some("tenant-abc".to_string()),
        status: ChannelBindingStatus::Active,
        webhook_path: Some("/webhook/lark/inst-1".to_string()),
    };

    // Create
    store.create(&binding, "encrypted-config-42").await.unwrap();

    // Get
    let found = store
        .get("uc-1")
        .await
        .unwrap()
        .expect("binding should exist");
    assert_eq!(found.id, "uc-1");
    assert_eq!(found.user_id, "user-1");
    assert_eq!(found.platform, PlatformType::Lark);
    assert_eq!(found.platform_user_id, Some("tenant-abc".to_string()));

    // Find by platform user
    let by_platform = store
        .find_by_platform_user(PlatformType::Lark, "tenant-abc")
        .await
        .unwrap()
        .expect("should find by platform user");
    assert_eq!(by_platform.id, "uc-1");

    // List by user
    let list = store.list_by_user("user-1").await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, "uc-1");

    // Update status
    store
        .update_status("uc-1", ChannelBindingStatus::Paused)
        .await
        .unwrap();
    let updated = store.get("uc-1").await.unwrap().unwrap();
    assert_eq!(updated.status, ChannelBindingStatus::Paused);

    // Delete
    store.delete("uc-1").await.unwrap();
    let deleted = store.get("uc-1").await.unwrap();
    assert!(deleted.is_none());
}

#[tokio::test]
async fn test_agent_channel_binding_store_lifecycle() {
    let db = setup_db().await;
    let user_store: Arc<dyn UserChannelStore> = Arc::new(SqliteUserChannelStore::new(db.clone()));
    let agent_store: Arc<dyn AgentChannelBindingStore> =
        Arc::new(SqliteAgentChannelBindingStore::new(db));

    // Create a user channel first
    let user_channel = UserChannelBinding {
        id: "uc-lark-1".to_string(),
        user_id: "user-1".to_string(),
        platform: PlatformType::Lark,
        instance_name: "main".to_string(),
        platform_user_id: Some("tenant-xyz".to_string()),
        status: ChannelBindingStatus::Active,
        webhook_path: Some("/webhook/lark/inst-x".to_string()),
    };
    user_store.create(&user_channel, "cfg").await.unwrap();

    // Bind agent to channel
    let binding = AgentChannelBinding {
        id: "acb-1".to_string(),
        agent_id: "agent-1".to_string(),
        user_channel_id: "uc-lark-1".to_string(),
        binding_name: Some("lark-main".to_string()),
        is_default: false,
        priority: 10,
        routing_rules: RoutingRules {
            keyword_filters: vec!["hello".to_string()],
            ..Default::default()
        },
    };
    agent_store.bind(&binding).await.unwrap();

    // List by agent
    let by_agent = agent_store.list_by_agent("agent-1").await.unwrap();
    assert_eq!(by_agent.len(), 1);
    assert_eq!(by_agent[0].user_channel_id, "uc-lark-1");

    // List by user channel
    let by_channel = agent_store.list_by_user_channel("uc-lark-1").await.unwrap();
    assert_eq!(by_channel.len(), 1);
    assert_eq!(by_channel[0].agent_id, "agent-1");

    // Set default
    agent_store
        .set_default("uc-lark-1", "agent-1")
        .await
        .unwrap();
    let defaults = agent_store.list_by_user_channel("uc-lark-1").await.unwrap();
    assert!(defaults
        .iter()
        .all(|b| b.is_default == (b.agent_id == "agent-1")));

    // Unbind
    agent_store.unbind("agent-1", "uc-lark-1").await.unwrap();
    let after_unbind = agent_store.list_by_agent("agent-1").await.unwrap();
    assert!(after_unbind.is_empty());
}

#[tokio::test]
async fn test_agent_channel_default_only_one_per_channel() {
    let db = setup_db().await;

    // Seed a second agent directly via SQL
    sqlx::query("INSERT INTO agents (id, name) VALUES (?1, ?2)")
        .bind("agent-2")
        .bind("Agent Two")
        .execute(&db)
        .await
        .unwrap();

    let user_store: Arc<dyn UserChannelStore> = Arc::new(SqliteUserChannelStore::new(db.clone()));
    let agent_store: Arc<dyn AgentChannelBindingStore> =
        Arc::new(SqliteAgentChannelBindingStore::new(db));

    let uc = UserChannelBinding {
        id: "uc-shared".to_string(),
        user_id: "user-1".to_string(),
        platform: PlatformType::Slack,
        instance_name: "default".to_string(),
        platform_user_id: Some("team-1".to_string()),
        status: ChannelBindingStatus::Active,
        webhook_path: Some("/webhook/slack/inst-s".to_string()),
    };
    user_store.create(&uc, "cfg").await.unwrap();

    let b1 = AgentChannelBinding {
        id: "acb-a1".to_string(),
        agent_id: "agent-1".to_string(),
        user_channel_id: "uc-shared".to_string(),
        binding_name: None,
        is_default: false,
        priority: 0,
        routing_rules: RoutingRules::default(),
    };
    let b2 = AgentChannelBinding {
        id: "acb-a2".to_string(),
        agent_id: "agent-2".to_string(),
        user_channel_id: "uc-shared".to_string(),
        binding_name: None,
        is_default: false,
        priority: 0,
        routing_rules: RoutingRules::default(),
    };

    agent_store.bind(&b1).await.unwrap();
    agent_store.bind(&b2).await.unwrap();

    // Make agent-1 default
    agent_store
        .set_default("uc-shared", "agent-1")
        .await
        .unwrap();
    let bindings = agent_store.list_by_user_channel("uc-shared").await.unwrap();
    let a1 = bindings.iter().find(|b| b.agent_id == "agent-1").unwrap();
    let a2 = bindings.iter().find(|b| b.agent_id == "agent-2").unwrap();
    assert!(a1.is_default);
    assert!(!a2.is_default);

    // Switch default to agent-2
    agent_store
        .set_default("uc-shared", "agent-2")
        .await
        .unwrap();
    let bindings = agent_store.list_by_user_channel("uc-shared").await.unwrap();
    let a1 = bindings.iter().find(|b| b.agent_id == "agent-1").unwrap();
    let a2 = bindings.iter().find(|b| b.agent_id == "agent-2").unwrap();
    assert!(!a1.is_default);
    assert!(a2.is_default);
}
