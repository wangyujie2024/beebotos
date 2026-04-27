//! 日志初始化模块
//!
//! 支持多种日志格式: compact, pretty, json

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::server::config::LogConfig;

/// 初始化日志系统
pub fn init_logger(config: &LogConfig) -> anyhow::Result<()> {
    let filter = tracing_subscriber::EnvFilter::try_new(&config.level)
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    match config.format.as_str() {
        "pretty" => {
            let fmt_layer = tracing_subscriber::fmt::layer()
                .pretty()
                .with_target(true)
                .with_thread_ids(true)
                .with_thread_names(true);

            tracing_subscriber::registry()
                .with(filter)
                .with(fmt_layer)
                .init();
        }
        "json" => {
            let fmt_layer = tracing_subscriber::fmt::layer()
                .json()
                .with_target(true)
                .with_current_span(true)
                .with_span_list(true);

            tracing_subscriber::registry()
                .with(filter)
                .with(fmt_layer)
                .init();
        }
        _ => {
            // 默认 compact 格式
            let fmt_layer = tracing_subscriber::fmt::layer()
                .compact()
                .with_target(false)
                .with_thread_ids(false)
                .without_time();

            tracing_subscriber::registry()
                .with(filter)
                .with(fmt_layer)
                .init();
        }
    }

    Ok(())
}

/// 记录服务器启动信息
pub fn log_startup(server_addr: &str, static_path: &str, gateway_url: &str) {
    tracing::info!(
        "\n┌────────────────────────────────────────────────┐\n\
         │  BeeBotOS Web Server started successfully      │\n\
         ├────────────────────────────────────────────────┤\n\
         │  Server:   http://{}                    │\n\
         │  Static:   {}\n\
         │  Gateway:  {}\n\
         └────────────────────────────────────────────────┘",
        server_addr,
        static_path,
        gateway_url
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_config() {
        let config = LogConfig {
            level: "debug".to_string(),
            format: "compact".to_string(),
        };
        // 注意: 日志只能初始化一次，这里只验证配置结构
        assert_eq!(config.level, "debug");
        assert_eq!(config.format, "compact");
    }
}
