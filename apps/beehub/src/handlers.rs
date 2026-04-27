use axum::extract::Path;
use axum::Json;

use crate::models::{PublishRequest, PublishResponse, Skill};

pub async fn index() -> &'static str {
    "ClawHub - BeeBotOS Skill Marketplace"
}

pub async fn list_skills() -> Json<Vec<Skill>> {
    Json(vec![Skill {
        id: "http-client".to_string(),
        name: "HTTP Client".to_string(),
        version: "1.0.0".to_string(),
        description: "Make HTTP requests".to_string(),
        author: "beebotos".to_string(),
        license: "MIT".to_string(),
        repository: None,
        hash: "abc123".to_string(),
        downloads: 15000,
        rating: 4.8,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }])
}

pub async fn get_skill(Path(id): Path<String>) -> Json<Skill> {
    Json(Skill {
        id,
        name: "HTTP Client".to_string(),
        version: "1.0.0".to_string(),
        description: "Make HTTP requests".to_string(),
        author: "beebotos".to_string(),
        license: "MIT".to_string(),
        repository: None,
        hash: "abc123".to_string(),
        downloads: 15000,
        rating: 4.8,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    })
}

pub async fn publish_skill(Json(req): Json<PublishRequest>) -> Json<PublishResponse> {
    Json(PublishResponse {
        id: uuid::Uuid::new_v4().to_string(),
        message: format!("Published {} v{}", req.name, req.version),
    })
}

pub async fn download_skill(Path(id): Path<String>) -> String {
    format!("Downloading skill {}", id)
}
