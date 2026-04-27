//! InstanceManager 状态机与统计测试

use std::collections::HashMap;

use beebotos_agents::skills::{InstanceFilter, InstanceManager, InstanceStatus};

#[tokio::test]
async fn test_instance_state_machine() {
    let manager = InstanceManager::new();

    // 创建实例 → 默认 Pending
    let id = manager
        .create("skill-a", "agent-1", HashMap::new())
        .await
        .unwrap();
    let inst = manager.get(&id).await.unwrap();
    assert_eq!(inst.status, InstanceStatus::Pending);

    // Pending → Running ✅
    manager
        .update_status(&id, InstanceStatus::Running)
        .await
        .unwrap();
    assert_eq!(
        manager.get(&id).await.unwrap().status,
        InstanceStatus::Running
    );

    // Running → Paused ✅
    manager
        .update_status(&id, InstanceStatus::Paused)
        .await
        .unwrap();
    assert_eq!(
        manager.get(&id).await.unwrap().status,
        InstanceStatus::Paused
    );

    // Paused → Running ✅
    manager
        .update_status(&id, InstanceStatus::Running)
        .await
        .unwrap();
    assert_eq!(
        manager.get(&id).await.unwrap().status,
        InstanceStatus::Running
    );

    // Running → Stopped ✅
    manager
        .update_status(&id, InstanceStatus::Stopped)
        .await
        .unwrap();
    assert_eq!(
        manager.get(&id).await.unwrap().status,
        InstanceStatus::Stopped
    );

    // 清理
    manager.delete(&id).await.unwrap();
}

#[tokio::test]
async fn test_invalid_state_transitions() {
    let manager = InstanceManager::new();

    // Pending → Paused ❌（不允许）
    let id = manager
        .create("skill-a", "agent-1", HashMap::new())
        .await
        .unwrap();
    let result = manager.update_status(&id, InstanceStatus::Paused).await;
    assert!(result.is_err());

    // Pending → Stopped ❌（不允许）
    let result = manager.update_status(&id, InstanceStatus::Stopped).await;
    assert!(result.is_err());

    // 必须先 Running
    manager
        .update_status(&id, InstanceStatus::Running)
        .await
        .unwrap();
    manager
        .update_status(&id, InstanceStatus::Stopped)
        .await
        .unwrap();

    // Stopped → Running ❌（已终止）
    let result = manager.update_status(&id, InstanceStatus::Running).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_usage_stats_and_filtering() {
    let manager = InstanceManager::new();

    let id1 = manager
        .create("skill-a", "agent-1", HashMap::new())
        .await
        .unwrap();
    let id2 = manager
        .create("skill-b", "agent-1", HashMap::new())
        .await
        .unwrap();
    let _id3 = manager
        .create("skill-a", "agent-2", HashMap::new())
        .await
        .unwrap();

    manager
        .update_status(&id1, InstanceStatus::Running)
        .await
        .unwrap();
    manager
        .update_status(&id2, InstanceStatus::Running)
        .await
        .unwrap();
    manager
        .update_status(&id2, InstanceStatus::Stopped)
        .await
        .unwrap();

    // 记录执行统计
    manager.record_execution(&id1, true, 100.0).await.unwrap();
    manager.record_execution(&id1, true, 200.0).await.unwrap();
    manager.record_execution(&id1, false, 300.0).await.unwrap();

    let inst1 = manager.get(&id1).await.unwrap();
    assert_eq!(inst1.usage.total_calls, 3);
    assert_eq!(inst1.usage.successful_calls, 2);
    assert_eq!(inst1.usage.failed_calls, 1);
    // avg_latency = (100 + 200 + 300) / 3 = 200.0
    assert_eq!(inst1.usage.avg_latency_ms, 200.0);

    // 按 agent_id 过滤
    let filter = InstanceFilter {
        agent_id: Some("agent-1".into()),
        ..Default::default()
    };
    assert_eq!(manager.list(&filter).await.len(), 2);

    // 按 skill_id 过滤
    let filter = InstanceFilter {
        skill_id: Some("skill-a".into()),
        ..Default::default()
    };
    assert_eq!(manager.list(&filter).await.len(), 2);

    // 按状态过滤
    let filter = InstanceFilter {
        status: Some(InstanceStatus::Running),
        ..Default::default()
    };
    assert_eq!(manager.list(&filter).await.len(), 1);
}
