//! BeeBotOS Web 服务器模块
//!
//! 纯 Rust 实现的 HTTP 服务器，用于服务静态文件和代理 API 请求

pub mod config;
pub mod logger;
pub mod proxy;

use axum::{
    extract::{Request, State},
    http::{header, HeaderValue, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;

/// 创建应用路由
pub fn create_app(
    static_path: &str,
    proxy_state: proxy::ProxyState,
) -> Router {
    // 静态文件服务 - 使用 fallback 返回 index.html 支持 SPA 路由
    let serve_dir = ServeDir::new(static_path)
        .append_index_html_on_directories(true)
        .fallback(tower_http::services::ServeFile::new(format!("{}/index.html", static_path)));

    // 创建 API 路由，使用嵌套路由匹配 /api/v1/... 等多级路径
    let api_routes = Router::new()
        .route("/*path", get(proxy_handler).post(proxy_handler))
        .route("/*path", axum::routing::put(proxy_handler))
        .route("/*path", axum::routing::delete(proxy_handler))
        .route("/*path", axum::routing::patch(proxy_handler))
        .route("/*path", axum::routing::head(proxy_handler))
        .route("/*path", axum::routing::options(proxy_handler));

    Router::new()
        // 健康检查
        .route("/health", get(proxy::health_check))
        // API 代理 - 嵌套在 /api 下
        .nest("/api", api_routes)
        // 静态文件服务
        .fallback_service(serve_dir)
        .with_state(proxy_state)
}

/// 代理处理器包装
async fn proxy_handler(
    State(state): axum::extract::State<proxy::ProxyState>,
    req: Request,
) -> Result<impl IntoResponse, StatusCode> {
    proxy::proxy_handler(State(state), req).await
}

/// 启动服务器
pub async fn run(config: &config::AppConfig) -> anyhow::Result<()> {
    use tower_http::cors::{Any, CorsLayer};
    use tower_http::trace::TraceLayer;

    // 创建代理状态
    let proxy_state = proxy::ProxyState::new(
        config.proxy.gateway_url.clone(),
        config.proxy.timeout_secs,
        config.proxy.forward_host,
    )?;

    // 创建路由
    let app = create_app(&config.static_file.path, proxy_state)
        // 添加 CORS 支持
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        // 开发环境禁用缓存，确保浏览器加载最新 WASM
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-cache, no-store, must-revalidate"),
        ))
        // 添加日志追踪
        .layer(TraceLayer::new_for_http());

    // 绑定地址
    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    // 记录启动信息
    logger::log_startup(
        &addr,
        &config.static_file.path,
        &config.proxy.gateway_url,
    );

    // 启动服务器
    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_app() {
        let proxy_state = proxy::ProxyState::new(
            "http://localhost:3000".to_string(),
            30,
            false,
        )
        .unwrap();

        let _app = create_app("pkg", proxy_state);
    }
}
