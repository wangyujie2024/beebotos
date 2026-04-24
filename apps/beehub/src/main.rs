//! BeeHub - Skill Marketplace for BeeBotOS

use std::net::SocketAddr;

use axum::routing::get;
use axum::Router;

mod handlers;
mod models;
mod storage;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let app = Router::new()
        .route("/", get(handlers::index))
        .route(
            "/api/skills",
            get(handlers::list_skills).post(handlers::publish_skill),
        )
        .route("/api/skills/:id", get(handlers::get_skill))
        .route("/api/skills/:id/download", get(handlers::download_skill));

    let addr: SocketAddr = "0.0.0.0:3000".parse()?;
    tracing::info!("BeeHub listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
