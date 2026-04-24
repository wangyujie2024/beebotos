//! BeeBotOS Web 服务器
//!
//! 纯 Rust 实现的 HTTP 服务器，用于服务静态文件和代理 API 请求
//!
//! 使用方法:
//!   cargo run --bin web-server
//!   cargo run --bin web-server -- --config data/web-server.toml
//!   cargo run --bin web-server -- --host 127.0.0.1 --port 8080

use std::path::PathBuf;

use clap::Parser;

/// 命令行参数
#[derive(Parser, Debug)]
#[command(name = "beebotos-web-server")]
#[command(about = "BeeBotOS Web 管理后台服务器")]
#[command(version)]
struct Args {
    /// 配置文件路径
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// 绑定主机
    #[arg(short = 'H', long)]
    host: Option<String>,

    /// 绑定端口
    #[arg(short, long)]
    port: Option<u16>,

    /// 静态文件目录
    #[arg(short = 's', long)]
    static_path: Option<String>,

    /// Gateway 地址
    #[arg(short = 'g', long)]
    gateway_url: Option<String>,

    /// 日志级别
    #[arg(short = 'l', long)]
    log_level: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 解析命令行参数
    let args = Args::parse();

    // 加载配置
    let mut config = beebotos_web::server::config::AppConfig::load(args.config.as_deref())?;

    // 命令行参数覆盖配置文件
    if let Some(host) = args.host {
        config.server.host = host;
    }
    if let Some(port) = args.port {
        config.server.port = port;
    }
    if let Some(static_path) = args.static_path {
        config.static_file.path = static_path;
    }
    if let Some(gateway_url) = args.gateway_url {
        config.proxy.gateway_url = gateway_url;
    }
    if let Some(log_level) = args.log_level {
        config.log.level = log_level;
    }

    // 初始化日志
    beebotos_web::server::logger::init_logger(&config.log)?;

    tracing::info!("BeeBotOS Web Server starting...");
    tracing::debug!("Configuration: {:?}", config);

    // 检查静态文件目录是否存在
    let static_path = &config.static_file.path;
    if !std::path::Path::new(static_path).exists() {
        tracing::warn!("Static file directory '{}' does not exist", static_path);
        std::fs::create_dir_all(static_path)?;
        tracing::info!("Created static file directory: {}", static_path);
    }

    // 检查 index.html 是否存在
    let index_path = format!("{}/index.html", static_path);
    if !std::path::Path::new(&index_path).exists() {
        tracing::warn!("index.html not found at {}", index_path);
        tracing::warn!("Please run 'wasm-pack build --target web --out-dir pkg' first");
    }

    // 启动服务器
    beebotos_web::server::run(&config).await?;

    Ok(())
}
