//! 服务器配置管理
//!
//! 支持 TOML 配置文件和环境变量覆盖

use std::path::Path;

use serde::{Deserialize, Serialize};

/// 应用配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// 服务器配置
    #[serde(default)]
    pub server: ServerConfig,
    /// 静态文件配置
    #[serde(default)]
    pub static_file: StaticConfig,
    /// 代理配置
    #[serde(default)]
    pub proxy: ProxyConfig,
    /// 日志配置
    #[serde(default)]
    pub log: LogConfig,
}

impl AppConfig {
    /// 从文件加载配置
    pub fn load<P: AsRef<Path>>(path: Option<P>) -> anyhow::Result<Self> {
        let mut builder = config::Config::builder();

        // 添加默认配置
        builder = builder.set_default("server.host", "0.0.0.0")?;
        builder = builder.set_default("server.port", 8090i64)?;
        builder = builder.set_default("static_file.path", "pkg")?;
        builder = builder.set_default("static_file.index", "index.html")?;
        builder = builder.set_default("static_file.cache_max_age", 3600i64)?;
        builder = builder.set_default("proxy.gateway_url", "http://localhost:3000")?;
        builder = builder.set_default("proxy.timeout_secs", 30i64)?;
        builder = builder.set_default("proxy.forward_host", false)?;
        builder = builder.set_default("log.level", "info")?;
        builder = builder.set_default("log.format", "compact")?;

        // 如果提供了配置文件路径，添加它
        if let Some(path) = path {
            builder = builder.add_source(config::File::from(path.as_ref()));
        } else if Path::new("config/web-server.toml").exists() {
            // 默认配置文件路径
            builder = builder.add_source(config::File::from(Path::new("config/web-server.toml")));
        }

        // 添加环境变量覆盖
        // 格式: WEB_SERVER__HOST, WEB_STATIC__PATH, WEB_PROXY__GATEWAY_URL, etc.
        builder = builder.add_source(
            config::Environment::with_prefix("WEB")
                .separator("__")
                .try_parsing(true),
        );

        let config = builder.build()?;
        let app_config: AppConfig = config.try_deserialize()?;

        Ok(app_config)
    }

    /// 从环境变量创建最小配置（用于快速启动）
    pub fn from_env() -> Self {
        let host = std::env::var("WEB_SERVER__HOST")
            .ok()
            .unwrap_or_else(|| "0.0.0.0".to_string());
        let port = std::env::var("WEB_SERVER__PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(8090);
        let static_path = std::env::var("WEB_STATIC__PATH")
            .ok()
            .unwrap_or_else(|| "pkg".to_string());
        let gateway_url = std::env::var("WEB_PROXY__GATEWAY_URL")
            .ok()
            .unwrap_or_else(|| "http://localhost:3000".to_string());
        let log_level = std::env::var("WEB_LOG__LEVEL")
            .ok()
            .unwrap_or_else(|| "info".to_string());

        Self {
            server: ServerConfig { host, port },
            static_file: StaticConfig {
                path: static_path,
                index: "index.html".to_string(),
                cache_max_age: 3600,
            },
            proxy: ProxyConfig {
                gateway_url,
                timeout_secs: 30,
                forward_host: false,
            },
            log: LogConfig {
                level: log_level,
                format: "compact".to_string(),
            },
        }
    }
}

/// 服务器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// 绑定主机
    pub host: String,
    /// 绑定端口
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8090,
        }
    }
}

/// 静态文件配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticConfig {
    /// 静态文件根目录
    pub path: String,
    /// 默认索引文件
    pub index: String,
    /// 缓存控制（秒）
    #[serde(rename = "cache_max_age")]
    pub cache_max_age: u64,
}

impl Default for StaticConfig {
    fn default() -> Self {
        Self {
            path: "pkg".to_string(),
            index: "index.html".to_string(),
            cache_max_age: 3600,
        }
    }
}

/// 代理配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    /// 后端 Gateway 地址
    #[serde(rename = "gateway_url")]
    pub gateway_url: String,
    /// 代理超时（秒）
    #[serde(rename = "timeout_secs")]
    pub timeout_secs: u64,
    /// 是否转发原始 Host 头
    #[serde(rename = "forward_host")]
    pub forward_host: bool,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            gateway_url: "http://localhost:3000".to_string(),
            timeout_secs: 30,
            forward_host: false,
        }
    }
}

/// 日志配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogConfig {
    /// 日志级别
    pub level: String,
    /// 格式: compact, pretty, json
    pub format: String,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: "compact".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AppConfig::load::<&str>(None).unwrap();
        assert_eq!(config.server.port, 8090);
        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.static_file.path, "pkg");
    }

    #[test]
    fn test_config_from_env() {
        std::env::set_var("WEB_SERVER__PORT", "8888");
        let config = AppConfig::from_env();
        assert_eq!(config.server.port, 8888);
        std::env::remove_var("WEB_SERVER__PORT");
    }
}
