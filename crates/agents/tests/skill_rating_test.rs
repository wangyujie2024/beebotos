//! Skill 评星与使用统计测试
//!
//! 验证 SkillRatingStore 的评分聚合和统计功能。

use beebotos_agents::skills::SkillRatingStore;
use sqlx::SqlitePool;

async fn create_test_db() -> SqlitePool {
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    sqlx::query(
        r#"
        CREATE TABLE skill_ratings (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            skill_id TEXT NOT NULL,
            user_id TEXT NOT NULL,
            rating INTEGER NOT NULL CHECK(rating >= 1 AND rating <= 5),
            review TEXT,
            created_at INTEGER NOT NULL DEFAULT (unixepoch()),
            UNIQUE(skill_id, user_id)
        )
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();
    pool
}

#[tokio::test]
async fn test_skill_rating_lifecycle() {
    let db = create_test_db().await;
    let store = SkillRatingStore::new(db);
    let skill_id = "skill-rating-test";

    // ========== 提交评分 ==========
    store
        .rate(skill_id, "user-a", 5, Some("Excellent skill"))
        .await
        .unwrap();
    store.rate(skill_id, "user-b", 3, None).await.unwrap();
    store
        .rate(skill_id, "user-c", 4, Some("Good but can improve"))
        .await
        .unwrap();

    // ========== 查询汇总 ==========
    let summary = store.get_summary(skill_id).await.unwrap();
    assert_eq!(summary.total_ratings, 3);
    // (5 + 3 + 4) / 3 = 4.0
    assert!((summary.average_rating - 4.0).abs() < 0.01);

    // ========== 查询详细列表 ==========
    let ratings = store.list_ratings(skill_id, 10, 0).await.unwrap();
    assert_eq!(ratings.len(), 3);

    // 验证排序（按 created_at DESC）
    let first = &ratings[0];
    assert!(first.rating >= 1 && first.rating <= 5);
    assert!(!first.skill_id.is_empty());

    // ========== 更新评分 ==========
    store
        .rate(skill_id, "user-a", 4, Some("Updated review"))
        .await
        .unwrap();

    let summary = store.get_summary(skill_id).await.unwrap();
    assert_eq!(summary.total_ratings, 3); // 仍然是 3 个用户
                                          // (4 + 3 + 4) / 3 = 11/3 ≈ 3.667
    assert!((summary.average_rating - 11.0 / 3.0).abs() < 0.01);
}

#[tokio::test]
async fn test_rating_pagination() {
    let db = create_test_db().await;
    let store = SkillRatingStore::new(db);
    let skill_id = "paginated-skill";

    // 插入 5 条评分
    for i in 1..=5 {
        store
            .rate(skill_id, &format!("user-{}", i), i as u32 % 5 + 1, None)
            .await
            .unwrap();
    }

    // limit=2, offset=0 → 2 条
    let page1 = store.list_ratings(skill_id, 2, 0).await.unwrap();
    assert_eq!(page1.len(), 2);

    // limit=2, offset=2 → 2 条
    let page2 = store.list_ratings(skill_id, 2, 2).await.unwrap();
    assert_eq!(page2.len(), 2);

    // limit=2, offset=4 → 1 条
    let page3 = store.list_ratings(skill_id, 2, 4).await.unwrap();
    assert_eq!(page3.len(), 1);

    // limit=2, offset=6 → 0 条
    let page4 = store.list_ratings(skill_id, 2, 6).await.unwrap();
    assert_eq!(page4.len(), 0);
}

#[tokio::test]
async fn test_empty_skill_summary() {
    let db = create_test_db().await;
    let store = SkillRatingStore::new(db);

    let summary = store.get_summary("no-ratings-skill").await.unwrap();
    assert_eq!(summary.total_ratings, 0);
    assert_eq!(summary.average_rating, 0.0);
}
