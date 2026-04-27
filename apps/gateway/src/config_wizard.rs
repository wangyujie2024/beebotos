//! Configuration Wizard for BeeBotOS Gateway
//!
//! Provides interactive configuration setup on first launch or when explicitly
//! requested. Supports both interactive TUI mode and non-interactive mode with
//! environment variables.
//!
//! Color Themes:
//! - Supports multiple color themes: default, dark, light, high_contrast,
//!   minimal, no_color
//! - Theme can be set via: --theme <theme> flag, BEE__WIZARD__COLOR_THEME env
//!   var, or config file
//! - Use --no-color flag to disable colors (for CI/logs)

use std::io::{self, Write};
use std::path::Path;

use secrecy::{ExposeSecret, SecretString};
use serde_json::json;

use crate::color_theme::{ColorTheme, ThemedText};
use crate::config::{BeeBotOSConfig, ChannelConfig};

/// Configuration categories for user selection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigCategory {
    Server,
    Database,
    Jwt,
    Models,
    Channels,
    Blockchain,
    Security,
    Logging,
    Metrics,
    Tls,
    All,
    Skip,
}

impl ConfigCategory {
    pub fn name(&self) -> &'static str {
        match self {
            ConfigCategory::Server => "Server Settings",
            ConfigCategory::Database => "Database Configuration",
            ConfigCategory::Jwt => "JWT Authentication",
            ConfigCategory::Models => "AI/LLM Models",
            ConfigCategory::Channels => "Communication Channels",
            ConfigCategory::Blockchain => "Blockchain/Web3",
            ConfigCategory::Security => "Security Settings",
            ConfigCategory::Logging => "Logging & Tracing",
            ConfigCategory::Metrics => "Metrics & Monitoring",
            ConfigCategory::Tls => "TLS/SSL Configuration",
            ConfigCategory::All => "Configure All",
            ConfigCategory::Skip => "Skip (use defaults)",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            ConfigCategory::Server => "HTTP server host, port, timeouts, CORS",
            ConfigCategory::Database => "SQLite database path, connection pool settings",
            ConfigCategory::Jwt => "JWT secret, token expiration, issuer settings",
            ConfigCategory::Models => "LLM providers (Kimi, OpenAI, etc.), API keys",
            ConfigCategory::Channels => "Messaging platforms (Lark, Discord, Telegram, etc.)",
            ConfigCategory::Blockchain => "Chain ID, RPC URL, wallet, contract addresses",
            ConfigCategory::Security => "Rate limiting, webhook validation, encryption",
            ConfigCategory::Logging => "Log level, format, rotation, tracing",
            ConfigCategory::Metrics => "Prometheus metrics endpoint and interval",
            ConfigCategory::Tls => "HTTPS certificates and mutual TLS",
            ConfigCategory::All => "Complete configuration of all settings",
            ConfigCategory::Skip => "Use default/minimal configuration",
        }
    }
}

/// Configuration change record for tracking modifications
#[derive(Debug, Clone)]
pub struct ConfigChange {
    pub category: String,
    pub field: String,
    pub old_value: String,
    pub new_value: String,
}

/// Configuration wizard state
pub struct ConfigWizard {
    config: BeeBotOSConfig,
    interactive: bool,
    changes: Vec<ConfigChange>,
    original_config: Option<BeeBotOSConfig>,
    theme: ThemedText,
}

impl ConfigWizard {
    /// Create new configuration wizard
    pub fn new(interactive: bool) -> Self {
        Self::with_theme(interactive, ColorTheme::Default)
    }

    /// Create new configuration wizard with specific theme
    pub fn with_theme(interactive: bool, theme: ColorTheme) -> Self {
        theme.apply();
        Self {
            config: BeeBotOSConfig::default(),
            interactive,
            changes: Vec::new(),
            original_config: None,
            theme: ThemedText::new(theme),
        }
    }

    /// Set color theme
    pub fn set_theme(&mut self, theme: ColorTheme) {
        theme.apply();
        self.theme.set_theme(theme);
    }

    /// Get current theme
    pub fn theme(&self) -> ColorTheme {
        self.theme.theme()
    }

    /// Check if configuration file exists
    pub fn config_exists() -> bool {
        Path::new("config/beebotos.toml").exists()
    }

    /// Backup existing configuration
    fn backup_existing_config() -> anyhow::Result<Option<String>> {
        let config_path = Path::new("config/beebotos.toml");
        if !config_path.exists() {
            return Ok(None);
        }

        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let backup_path = format!("config/beebotos.toml.backup.{}", timestamp);

        std::fs::copy(config_path, &backup_path)?;
        Ok(Some(backup_path))
    }

    /// Run the configuration wizard
    pub async fn run(&mut self) -> anyhow::Result<BeeBotOSConfig> {
        if !self.interactive {
            // Non-interactive mode: use environment variables or defaults
            self.config_from_env()?;
            return Ok(self.config.clone());
        }

        // Interactive mode with restart capability
        'main_loop: loop {
            self.print_banner();

            // Check if config already exists
            if Self::config_exists() {
                println!(
                    "\n{} Configuration file already exists at config/beebotos.toml",
                    self.theme.warning("⚠️ ")
                );

                if self.prompt_bool("Do you want to reconfigure?", false)? {
                    // Backup existing config
                    match Self::backup_existing_config()? {
                        Some(backup_path) => {
                            println!(
                                "{} Existing configuration backed up to: {}",
                                self.theme.info("📦"),
                                self.theme.success(&backup_path)
                            );
                            // Store original for change tracking
                            self.original_config = Some(BeeBotOSConfig::load()?);
                        }
                        None => {}
                    }
                } else {
                    println!("   Keeping existing configuration.");
                    return Ok(BeeBotOSConfig::load()?);
                }
            }

            // Show main menu
            'config_menu: loop {
                self.show_main_menu()?;

                print!(
                    "\n{} ",
                    self.theme.primary_bold("Enter your choice (0-12):")
                );
                io::stdout().flush()?;

                let mut input = String::new();
                io::stdin().read_line(&mut input)?;

                match input.trim().parse::<u8>() {
                    Ok(0) => {
                        println!(
                            "\n{} Configuration cancelled. Exiting...",
                            self.theme.warning("👋")
                        );
                        std::process::exit(0);
                    }
                    Ok(1) => self.configure_server().await?,
                    Ok(2) => self.configure_database().await?,
                    Ok(3) => self.configure_jwt().await?,
                    Ok(4) => self.configure_models().await?,
                    Ok(5) => self.configure_channels().await?,
                    Ok(6) => self.configure_blockchain().await?,
                    Ok(7) => self.configure_security().await?,
                    Ok(8) => self.configure_logging().await?,
                    Ok(9) => self.configure_metrics().await?,
                    Ok(10) => self.configure_tls().await?,
                    Ok(11) => {
                        self.configure_all().await?;
                        break 'config_menu;
                    }
                    Ok(12) => {
                        println!("   Using default configuration...");
                        break 'config_menu;
                    }
                    _ => println!(
                        "   {} Invalid choice, please try again.",
                        self.theme.error("❌")
                    ),
                }

                // Ask if user wants to configure more
                if !self.prompt_bool("\nConfigure another section?", true)? {
                    break 'config_menu;
                }
            }

            // Show configuration preview before saving
            if self.prompt_bool("\nPreview configuration before saving?", true)? {
                self.show_config_preview()?;
            }

            // Confirm save
            if !self.prompt_bool("\nSave this configuration?", true)? {
                println!("{} Configuration not saved.", self.theme.warning("⚠️ "));
                if self.prompt_bool("Start over?", false)? {
                    self.config = BeeBotOSConfig::default();
                    self.changes.clear();
                    // Restart from the beginning
                    continue 'main_loop;
                }
                println!("Exiting without saving...");
                std::process::exit(0);
            }

            // Save configuration
            self.save_config().await?;

            // Success - exit the main loop
            break 'main_loop;
        }

        Ok(self.config.clone())
    }

    /// Print welcome banner
    fn print_banner(&self) {
        let banner = r#"
╔═══════════════════════════════════════════════════════════════╗
║                                                               ║
║           🐝 BeeBotOS Gateway Configuration Wizard            ║
║                                                               ║
║           AI Agent Operating System - API Gateway             ║
║                                                               ║
╚═══════════════════════════════════════════════════════════════╝
"#;
        println!("{}", self.theme.primary_bold(banner));
        println!(
            "{} This wizard will help you configure BeeBotOS Gateway.\n",
            self.theme.success_bold("Welcome!")
        );
    }

    /// Show main configuration menu
    fn show_main_menu(&self) -> io::Result<()> {
        println!("\n{}", self.theme.primary_bold("📋 Configuration Menu:"));
        println!();
        println!(
            "  1.  {} - {}",
            self.theme.secondary(ConfigCategory::Server.name()),
            self.theme.muted(ConfigCategory::Server.description())
        );
        println!(
            "  2.  {} - {}",
            self.theme.secondary(ConfigCategory::Database.name()),
            self.theme.muted(ConfigCategory::Database.description())
        );
        println!(
            "  3.  {} - {}",
            self.theme.secondary(ConfigCategory::Jwt.name()),
            self.theme.muted(ConfigCategory::Jwt.description())
        );
        println!(
            "  4.  {} - {}",
            self.theme.secondary(ConfigCategory::Models.name()),
            self.theme.muted(ConfigCategory::Models.description())
        );
        println!(
            "  5.  {} - {}",
            self.theme.secondary(ConfigCategory::Channels.name()),
            self.theme.muted(ConfigCategory::Channels.description())
        );
        println!(
            "  6.  {} - {}",
            self.theme.secondary(ConfigCategory::Blockchain.name()),
            self.theme.muted(ConfigCategory::Blockchain.description())
        );
        println!(
            "  7.  {} - {}",
            self.theme.secondary(ConfigCategory::Security.name()),
            self.theme.muted(ConfigCategory::Security.description())
        );
        println!(
            "  8.  {} - {}",
            self.theme.secondary(ConfigCategory::Logging.name()),
            self.theme.muted(ConfigCategory::Logging.description())
        );
        println!(
            "  9.  {} - {}",
            self.theme.secondary(ConfigCategory::Metrics.name()),
            self.theme.muted(ConfigCategory::Metrics.description())
        );
        println!(
            "  10. {} - {}",
            self.theme.secondary(ConfigCategory::Tls.name()),
            self.theme.muted(ConfigCategory::Tls.description())
        );
        println!();
        println!(
            "  11. {} - {}",
            self.theme.success_bold(ConfigCategory::All.name()),
            self.theme.muted("Configure everything step by step")
        );
        println!(
            "  12. {} - {}",
            self.theme.muted(ConfigCategory::Skip.name()),
            self.theme.muted("Use minimal default settings")
        );
        println!();
        println!(
            "  0.  {} - {}",
            self.theme.error("Exit"),
            self.theme.muted("Exit without saving")
        );
        Ok(())
    }

    /// Show configuration preview
    fn show_config_preview(&self) -> anyhow::Result<()> {
        println!("\n{}", self.theme.primary_bold("📋 Configuration Preview:"));
        println!("{}", self.theme.muted(&"═".repeat(60)));

        // Server
        println!("\n{}", self.theme.secondary("🌐 Server:"));
        println!("  Host: {}", self.theme.success(&self.config.server.host));
        println!(
            "  Port: {}",
            self.theme.success(&self.config.server.port.to_string())
        );
        println!(
            "  Timeout: {}s",
            self.theme
                .success(&self.config.server.timeout_seconds.to_string())
        );
        println!(
            "  Max Body: {}MB",
            self.theme
                .success(&self.config.server.max_body_size_mb.to_string())
        );

        // Database
        println!("\n{}", self.theme.secondary("🗄️  Database:"));
        println!("  URL: {}", self.theme.success(&self.config.database.url));
        println!(
            "  Max Connections: {}",
            self.theme
                .success(&self.config.database.max_connections.to_string())
        );

        // JWT
        println!("\n{}", self.theme.secondary("🔐 JWT:"));
        let jwt_secret = self.config.jwt.secret.expose_secret();
        let masked = if jwt_secret.len() > 8 {
            format!(
                "{}...{} ({} chars)",
                &jwt_secret[..4],
                &jwt_secret[jwt_secret.len() - 4..],
                jwt_secret.len()
            )
        } else {
            self.theme.error("[NOT SET]").to_string()
        };
        println!("  Secret: {}", self.theme.success(&masked));
        println!(
            "  Expiry: {}h",
            self.theme
                .success(&self.config.jwt.expiry_hours.to_string())
        );

        // Models
        println!("\n{}", self.theme.secondary("🤖 Models:"));
        println!(
            "  Default Provider: {}",
            self.theme.success(&self.config.models.default_provider)
        );
        println!(
            "  Configured Providers: {}",
            self.theme.success(
                &self
                    .config
                    .models
                    .providers
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        );

        // Channels
        println!("\n{}", self.theme.secondary("📱 Channels:"));
        let enabled_channels = self.config.get_enabled_channels();
        if enabled_channels.is_empty() {
            println!("  {}", self.theme.muted("No channels enabled"));
        } else {
            for (name, _) in enabled_channels {
                println!("  ✅ {}", self.theme.success(name));
            }
        }

        // Blockchain
        println!("\n{}", self.theme.secondary("⛓️  Blockchain:"));
        if self.config.blockchain.enabled {
            println!("  Status: {}", self.theme.success("Enabled"));
            println!(
                "  Chain ID: {}",
                self.theme
                    .success(&self.config.blockchain.chain_id.to_string())
            );
        } else {
            println!("  Status: {}", self.theme.muted("Disabled"));
        }

        // Security
        println!("\n{}", self.theme.secondary("🔒 Security:"));
        println!(
            "  Rate Limiting: {}",
            if self.config.rate_limit.enabled {
                self.theme.success("Enabled")
            } else {
                self.theme.muted("Disabled")
            }
        );
        println!(
            "  Webhook Verification: {}",
            if self.config.security.verify_webhook_signatures {
                self.theme.success("Enabled")
            } else {
                self.theme.muted("Disabled")
            }
        );

        // Changes summary
        if !self.changes.is_empty() {
            println!("\n{}", self.theme.primary("📝 Changes Made:"));
            for change in &self.changes {
                println!(
                    "  {}.{}: {} → {}",
                    self.theme.muted(&change.category),
                    change.field,
                    self.theme.error(&change.old_value),
                    self.theme.success(&change.new_value)
                );
            }
        }

        println!("\n{}", self.theme.muted(&"═".repeat(60)));
        Ok(())
    }

    /// Configure server settings
    async fn configure_server(&mut self) -> anyhow::Result<()> {
        println!("\n{}", self.theme.primary_bold("🌐 Server Configuration"));
        println!();

        // Validate and set host
        loop {
            let default_host = self.config.server.host.clone();
            let host = self.prompt_string("Server host", &default_host)?;
            if Self::validate_host(&host) {
                self.record_change("server", "host", &default_host, &host);
                self.config.server.host = host;
                break;
            }
            println!(
                "   {} Invalid host format. Use IP address or hostname.",
                self.theme.error("❌")
            );
        }

        // Validate and set port
        loop {
            let port = self.prompt_u16("Server port", self.config.server.port, 1, 65535)?;
            if Self::validate_port_available(port) {
                self.record_change(
                    "server",
                    "port",
                    &self.config.server.port.to_string(),
                    &port.to_string(),
                );
                self.config.server.port = port;
                break;
            }
            println!(
                "   {} Port {} may be in use. Choose another.",
                self.theme.warning("⚠️ "),
                port
            );
        }

        self.config.server.timeout_seconds = self.prompt_u64(
            "Request timeout (seconds)",
            self.config.server.timeout_seconds,
            1,
            3600,
        )?;

        self.config.server.max_body_size_mb = self.prompt_usize(
            "Max body size (MB)",
            self.config.server.max_body_size_mb,
            1,
            1000,
        )?;

        // CORS configuration
        println!("\n{}", self.theme.warning_bold("📡 CORS Configuration:"));
        let origins =
            self.prompt_string("Allowed origins (comma-separated, use * for all)", "*")?;
        self.config.server.cors.allowed_origins =
            origins.split(',').map(|s| s.trim().to_string()).collect();

        println!("\n{} Server configuration saved.", self.theme.success("✅"));
        Ok(())
    }

    /// Configure database settings
    async fn configure_database(&mut self) -> anyhow::Result<()> {
        println!(
            "\n{}",
            self.theme.primary_bold("🗄️  Database Configuration")
        );
        println!();

        // Validate database URL
        loop {
            let default_url = self.config.database.url.clone();
            let url = self.prompt_string(
                "Database URL (sqlite://path or postgres://host/db)",
                &default_url,
            )?;

            if Self::validate_database_url(&url) {
                self.record_change("database", "url", &default_url, &url);
                self.config.database.url = url;
                break;
            }
            println!("   {} Invalid database URL format.", self.theme.error("❌"));
        }

        self.config.database.max_connections = self.prompt_u32(
            "Max connections",
            self.config.database.max_connections,
            1,
            1000,
        )?;

        self.config.database.min_connections = self.prompt_u32(
            "Min connections",
            self.config.database.min_connections,
            1,
            self.config.database.max_connections,
        )?;

        self.config.database.run_migrations = self.prompt_bool(
            "Run database migrations on startup?",
            self.config.database.run_migrations,
        )?;

        println!(
            "\n{} Database configuration saved.",
            self.theme.success("✅")
        );
        Ok(())
    }

    /// Configure JWT settings
    async fn configure_jwt(&mut self) -> anyhow::Result<()> {
        println!(
            "\n{}",
            self.theme
                .primary_bold("🔐 JWT Authentication Configuration")
        );
        println!();

        println!(
            "{} JWT secret should be at least 32 characters long!",
            self.theme.warning_bold("⚠️ ")
        );

        // Secret input with confirmation
        let secret = loop {
            let s1 = self.prompt_password_hidden("JWT secret (leave empty for auto-generated)")?;

            if s1.is_empty() {
                let generated = Self::generate_jwt_secret();
                println!(
                    "   {} Generated secret: {}",
                    self.theme.info("🔑"),
                    self.theme.success(&generated[..20.min(generated.len())])
                );
                break generated;
            }

            if s1.len() < 32 {
                println!(
                    "   {} Secret must be at least 32 characters!",
                    self.theme.error("❌")
                );
                continue;
            }

            // Confirm password
            let s2 = self.prompt_password_hidden("Confirm JWT secret")?;
            if s1 != s2 {
                println!("   {} Secrets do not match!", self.theme.error("❌"));
                continue;
            }

            break s1;
        };

        self.config.jwt.secret = SecretString::new(secret);

        self.config.jwt.expiry_hours = self.prompt_i64(
            "Token expiry (hours)",
            self.config.jwt.expiry_hours,
            1,
            8760,
        )?;

        self.config.jwt.refresh_expiry_hours = self.prompt_i64(
            "Refresh token expiry (hours)",
            self.config.jwt.refresh_expiry_hours,
            1,
            8760,
        )?;

        self.config.jwt.issuer = self.prompt_string("Token issuer", &self.config.jwt.issuer)?;

        println!("\n{} JWT configuration saved.", self.theme.success("✅"));
        Ok(())
    }

    /// Configure LLM model settings
    async fn configure_models(&mut self) -> anyhow::Result<()> {
        println!(
            "\n{}",
            self.theme.primary_bold("🤖 AI/LLM Model Configuration")
        );
        println!();

        let providers = vec!["kimi", "openai", "anthropic", "zhipu", "ollama"];

        println!("Available providers:");
        for (i, provider) in providers.iter().enumerate() {
            println!(
                "  {}. {}",
                self.theme.info(&(i + 1).to_string()),
                self.theme.success(provider)
            );
        }

        let default_idx = self.prompt_usize("Select default provider", 1, 1, providers.len())?;
        self.config.models.default_provider = providers[default_idx - 1].to_string();

        // Configure each provider
        for provider in &providers {
            if self.prompt_bool(
                &format!("\nConfigure {}?", self.theme.success(provider)),
                false,
            )? {
                let mut provider_config = crate::config::ModelProviderConfig::default();

                // API Key with hidden input
                let api_key = self.prompt_password_hidden(&format!("{} API key", provider))?;
                if !api_key.is_empty() {
                    provider_config.api_key = Some(api_key);
                }

                // Model name with defaults
                let default_model = match *provider {
                    "kimi" => "moonshot-v1-8k",
                    "openai" => "gpt-4",
                    "anthropic" => "claude-3-sonnet",
                    "zhipu" => "glm-4",
                    "ollama" => "llama2",
                    _ => "default",
                };

                provider_config.model =
                    Some(self.prompt_string(&format!("{} model name", provider), default_model)?);

                // Base URL for specific providers
                if provider == &"ollama" || provider == &"kimi" {
                    let default_url = match *provider {
                        "ollama" => "http://localhost:11434",
                        "kimi" => "https://api.moonshot.cn",
                        _ => "",
                    };
                    provider_config.base_url =
                        Some(self.prompt_string(&format!("{} base URL", provider), default_url)?);
                }

                // Temperature
                let temp_str = self.prompt_string("Temperature (0.0-2.0)", "0.7")?;
                if let Ok(temp) = temp_str.parse::<f32>() {
                    provider_config.temperature = temp.clamp(0.0, 2.0);
                }

                self.config
                    .models
                    .providers
                    .insert(provider.to_string(), provider_config);
            }
        }

        println!("\n{} Model configuration saved.", self.theme.success("✅"));
        Ok(())
    }

    /// Configure communication channels
    async fn configure_channels(&mut self) -> anyhow::Result<()> {
        println!(
            "\n{}",
            self.theme
                .primary_bold("📱 Communication Channel Configuration")
        );
        println!();

        let channels = vec![
            ("lark", "Lark/Feishu"),
            ("dingtalk", "DingTalk"),
            ("telegram", "Telegram"),
            ("discord", "Discord"),
            ("slack", "Slack"),
            ("wechat", "WeChat Work"),
            ("personal_wechat", "Personal WeChat (iPad protocol)"),
        ];

        for (id, name) in channels {
            if self.prompt_bool(&format!("Configure {}?", self.theme.success(name)), false)? {
                let mut channel_config = ChannelConfig::default();
                channel_config.enabled = true;

                println!("   Enter {} settings:", self.theme.warning(name));

                match id {
                    "lark" => {
                        let app_id = self.prompt_string("  App ID", "")?;
                        let app_secret = self.prompt_password_hidden("  App Secret")?;
                        let encrypt_key =
                            self.prompt_password_hidden("  Encrypt Key (optional)")?;

                        channel_config
                            .settings
                            .insert("app_id".to_string(), json!(app_id));
                        channel_config
                            .settings
                            .insert("app_secret".to_string(), json!(app_secret));
                        if !encrypt_key.is_empty() {
                            channel_config
                                .settings
                                .insert("encrypt_key".to_string(), json!(encrypt_key));
                        }
                    }
                    "dingtalk" => {
                        let app_key = self.prompt_string("  App Key", "")?;
                        let app_secret = self.prompt_password_hidden("  App Secret")?;

                        channel_config
                            .settings
                            .insert("app_key".to_string(), json!(app_key));
                        channel_config
                            .settings
                            .insert("app_secret".to_string(), json!(app_secret));
                    }
                    "telegram" => {
                        let bot_token = self.prompt_password_hidden("  Bot Token")?;

                        channel_config
                            .settings
                            .insert("bot_token".to_string(), json!(bot_token));
                    }
                    "discord" => {
                        let bot_token = self.prompt_password_hidden("  Bot Token")?;

                        channel_config
                            .settings
                            .insert("bot_token".to_string(), json!(bot_token));
                    }
                    "slack" => {
                        let bot_token = self.prompt_password_hidden("  Bot Token")?;
                        let app_token = self.prompt_password_hidden("  App Token (optional)")?;

                        channel_config
                            .settings
                            .insert("bot_token".to_string(), json!(bot_token));
                        if !app_token.is_empty() {
                            channel_config
                                .settings
                                .insert("app_token".to_string(), json!(app_token));
                        }
                    }
                    "wechat" => {
                        let corp_id = self.prompt_string("  Corp ID", "")?;
                        let agent_id = self.prompt_string("  Agent ID", "")?;
                        let secret = self.prompt_password_hidden("  Secret")?;
                        let token = self.prompt_string("  Token", "")?;
                        let aes_key = self.prompt_password_hidden("  Encoding AES Key")?;

                        channel_config
                            .settings
                            .insert("corp_id".to_string(), json!(corp_id));
                        channel_config
                            .settings
                            .insert("agent_id".to_string(), json!(agent_id));
                        channel_config
                            .settings
                            .insert("secret".to_string(), json!(secret));
                        channel_config
                            .settings
                            .insert("token".to_string(), json!(token));
                        channel_config
                            .settings
                            .insert("encoding_aes_key".to_string(), json!(aes_key));
                    }
                    "personal_wechat" => {
                        println!(
                            "   {} Personal WeChat uses QR code login, no additional settings \
                             needed.",
                            self.theme.info("ℹ️ ")
                        );
                    }
                    _ => {}
                }

                // Set the channel config
                match id {
                    "lark" => self.config.channels.lark = Some(channel_config),
                    "dingtalk" => self.config.channels.dingtalk = Some(channel_config),
                    "telegram" => self.config.channels.telegram = Some(channel_config),
                    "discord" => self.config.channels.discord = Some(channel_config),
                    "slack" => self.config.channels.slack = Some(channel_config),
                    "wechat" => self.config.channels.wechat = Some(channel_config),
                    "personal_wechat" => {
                        self.config.channels.personal_wechat = Some(channel_config)
                    }
                    _ => {}
                }

                println!("   {} {} configured.", self.theme.success("✅"), name);
            }
        }

        // Advanced channel settings
        if self.prompt_bool("\nConfigure advanced channel settings?", false)? {
            self.config.channels.auto_download_media = self.prompt_bool(
                "Auto-download media?",
                self.config.channels.auto_download_media,
            )?;

            self.config.channels.max_file_size_mb = self.prompt_u32(
                "Max file size (MB)",
                self.config.channels.max_file_size_mb,
                1,
                500,
            )?;

            self.config.channels.context_window_size = self.prompt_usize(
                "Context window size (messages)",
                self.config.channels.context_window_size,
                1,
                100,
            )?;
        }

        println!(
            "\n{} Channel configuration saved.",
            self.theme.success("✅")
        );
        Ok(())
    }

    /// Configure blockchain settings
    async fn configure_blockchain(&mut self) -> anyhow::Result<()> {
        println!(
            "\n{}",
            self.theme.primary_bold("⛓️  Blockchain/Web3 Configuration")
        );
        println!();

        self.config.blockchain.enabled = self.prompt_bool(
            "Enable blockchain integration?",
            self.config.blockchain.enabled,
        )?;

        if self.config.blockchain.enabled {
            self.config.blockchain.chain_id =
                self.prompt_u64("Chain ID", self.config.blockchain.chain_id, 1, u64::MAX)?;

            // Validate RPC URL
            loop {
                let rpc_url = self.prompt_string(
                    "RPC URL (leave empty to skip)",
                    self.config.blockchain.rpc_url.as_deref().unwrap_or(""),
                )?;

                if rpc_url.is_empty() {
                    break;
                }

                if Self::validate_url(&rpc_url) {
                    self.config.blockchain.rpc_url = Some(rpc_url);
                    break;
                }
                println!("   {} Invalid URL format.", self.theme.error("❌"));
            }

            // Mnemonic with confirmation
            let mnemonic = loop {
                let m1 =
                    self.prompt_password_hidden("Wallet mnemonic (leave empty for new wallet)")?;

                if m1.is_empty() {
                    break None;
                }

                let m2 = self.prompt_password_hidden("Confirm mnemonic")?;
                if m1 != m2 {
                    println!("   {} Mnemonics do not match!", self.theme.error("❌"));
                    continue;
                }

                break Some(m1);
            };

            if let Some(m) = mnemonic {
                self.config.blockchain.agent_wallet_mnemonic = Some(m);
            }

            println!("\n   Smart Contract Addresses (optional):");

            // Validate contract addresses
            let identity_addr = self.prompt_string("  Identity Contract (0x...)", "")?;
            if Self::validate_ethereum_address(&identity_addr) {
                self.config.blockchain.identity_contract_address = Some(identity_addr);
            } else if !identity_addr.is_empty() {
                println!("   {} Invalid address format.", self.theme.warning("⚠️ "));
            }

            let registry_addr = self.prompt_string("  Registry Contract (0x...)", "")?;
            if Self::validate_ethereum_address(&registry_addr) {
                self.config.blockchain.registry_contract_address = Some(registry_addr);
            } else if !registry_addr.is_empty() {
                println!("   {} Invalid address format.", self.theme.warning("⚠️ "));
            }

            let dao_addr = self.prompt_string("  DAO Contract (0x...)", "")?;
            if Self::validate_ethereum_address(&dao_addr) {
                self.config.blockchain.dao_contract_address = Some(dao_addr);
            } else if !dao_addr.is_empty() {
                println!("   {} Invalid address format.", self.theme.warning("⚠️ "));
            }

            let skill_nft_addr = self.prompt_string("  SkillNFT Contract (0x...)", "")?;
            if Self::validate_ethereum_address(&skill_nft_addr) {
                self.config.blockchain.skill_nft_contract_address = Some(skill_nft_addr);
            } else if !skill_nft_addr.is_empty() {
                println!("   {} Invalid address format.", self.theme.warning("⚠️ "));
            }
        }

        println!(
            "\n{} Blockchain configuration saved.",
            self.theme.success("✅")
        );
        Ok(())
    }

    /// Configure security settings
    async fn configure_security(&mut self) -> anyhow::Result<()> {
        println!("\n{}", self.theme.primary_bold("🔒 Security Configuration"));
        println!();

        self.config.rate_limit.enabled =
            self.prompt_bool("Enable rate limiting?", self.config.rate_limit.enabled)?;

        if self.config.rate_limit.enabled {
            self.config.rate_limit.requests_per_second = self.prompt_u32(
                "Requests per second",
                self.config.rate_limit.requests_per_second,
                1,
                10000,
            )?;

            self.config.rate_limit.burst_size =
                self.prompt_u32("Burst size", self.config.rate_limit.burst_size, 1, 100000)?;
        }

        self.config.security.verify_webhook_signatures = self.prompt_bool(
            "Verify webhook signatures?",
            self.config.security.verify_webhook_signatures,
        )?;

        self.config.security.encryption_enabled = self.prompt_bool(
            "Enable encryption for sensitive data?",
            self.config.security.encryption_enabled,
        )?;

        // Advanced: Allowed webhook IPs
        if self.prompt_bool("Configure allowed webhook IPs? (Advanced)", false)? {
            let ips =
                self.prompt_string("Allowed IPs (comma-separated CIDR notation)", "0.0.0.0/0")?;
            self.config.security.allowed_webhook_ips =
                ips.split(',').map(|s| s.trim().to_string()).collect();
        }

        println!(
            "\n{} Security configuration saved.",
            self.theme.success("✅")
        );
        Ok(())
    }

    /// Configure logging settings
    async fn configure_logging(&mut self) -> anyhow::Result<()> {
        println!("\n{}", self.theme.primary_bold("📝 Logging Configuration"));
        println!();

        self.config.logging.level = self.prompt_choice(
            "Log level",
            &["trace", "debug", "info", "warn", "error"],
            &self.config.logging.level,
        )?;

        self.config.logging.format = self.prompt_choice(
            "Log format",
            &["json", "pretty", "compact"],
            &self.config.logging.format,
        )?;

        self.config.logging.file =
            self.prompt_string("Log file path", &self.config.logging.file)?;

        self.config.logging.rotation.enabled =
            self.prompt_bool("Enable log rotation?", self.config.logging.rotation.enabled)?;

        if self.config.logging.rotation.enabled {
            self.config.logging.rotation.max_size_mb = self.prompt_u32(
                "Max log file size (MB)",
                self.config.logging.rotation.max_size_mb,
                1,
                10000,
            )?;

            self.config.logging.rotation.max_files = self.prompt_u32(
                "Max rotated files to keep",
                self.config.logging.rotation.max_files,
                1,
                100,
            )?;
        }

        self.config.tracing.enabled = self.prompt_bool(
            "Enable distributed tracing (OpenTelemetry)?",
            self.config.tracing.enabled,
        )?;

        if self.config.tracing.enabled {
            let endpoint = self.prompt_string(
                "OpenTelemetry endpoint (leave empty for default)",
                self.config.tracing.otel_endpoint.as_deref().unwrap_or(""),
            )?;
            if !endpoint.is_empty() {
                if Self::validate_url(&endpoint) {
                    self.config.tracing.otel_endpoint = Some(endpoint);
                } else {
                    println!(
                        "   {} Invalid URL format, skipping.",
                        self.theme.warning("⚠️ ")
                    );
                }
            }
        }

        println!(
            "\n{} Logging configuration saved.",
            self.theme.success("✅")
        );
        Ok(())
    }

    /// Configure metrics settings
    async fn configure_metrics(&mut self) -> anyhow::Result<()> {
        println!(
            "\n{}",
            self.theme
                .primary_bold("📊 Metrics & Monitoring Configuration")
        );
        println!();

        self.config.metrics.enabled =
            self.prompt_bool("Enable Prometheus metrics?", self.config.metrics.enabled)?;

        if self.config.metrics.enabled {
            let endpoint = self.prompt_string(
                "Metrics endpoint (host:port)",
                &self.config.metrics.endpoint,
            )?;

            // Validate endpoint format
            if endpoint.contains(':') {
                self.config.metrics.endpoint = endpoint;
            } else {
                println!(
                    "   {} Invalid endpoint format, using default.",
                    self.theme.warning("⚠️ ")
                );
            }

            self.config.metrics.interval_seconds = self.prompt_u64(
                "Metrics collection interval (seconds)",
                self.config.metrics.interval_seconds,
                1,
                3600,
            )?;
        }

        println!(
            "\n{} Metrics configuration saved.",
            self.theme.success("✅")
        );
        Ok(())
    }

    /// Configure TLS settings
    async fn configure_tls(&mut self) -> anyhow::Result<()> {
        println!("\n{}", self.theme.primary_bold("🔏 TLS/SSL Configuration"));
        println!();

        let enabled = self.prompt_bool(
            "Enable TLS/HTTPS?",
            self.config.tls.as_ref().map(|t| t.enabled).unwrap_or(false),
        )?;

        if enabled {
            let mut tls = self.config.tls.clone().unwrap_or_default();
            tls.enabled = true;

            // Certificate path with validation
            loop {
                let cert_path = self.prompt_string(
                    "Certificate file path",
                    tls.cert_path.as_deref().unwrap_or(""),
                )?;

                if cert_path.is_empty() {
                    println!(
                        "   {} Certificate path is required for TLS!",
                        self.theme.error("❌")
                    );
                    continue;
                }

                if !std::path::Path::new(&cert_path).exists() {
                    println!(
                        "   {} Certificate file not found: {}",
                        self.theme.warning("⚠️ "),
                        cert_path
                    );
                    if !self.prompt_bool("   Use this path anyway?", false)? {
                        continue;
                    }
                }

                tls.cert_path = Some(cert_path);
                break;
            }

            // Key path with validation
            loop {
                let key_path = self.prompt_string(
                    "Private key file path",
                    tls.key_path.as_deref().unwrap_or(""),
                )?;

                if key_path.is_empty() {
                    println!(
                        "   {} Key path is required for TLS!",
                        self.theme.error("❌")
                    );
                    continue;
                }

                if !std::path::Path::new(&key_path).exists() {
                    println!(
                        "   {} Key file not found: {}",
                        self.theme.warning("⚠️ "),
                        key_path
                    );
                    if !self.prompt_bool("   Use this path anyway?", false)? {
                        continue;
                    }
                }

                tls.key_path = Some(key_path);
                break;
            }

            // CA path (optional)
            let ca_path = self.prompt_string(
                "CA certificate path (optional, for mTLS)",
                tls.ca_path.as_deref().unwrap_or(""),
            )?;
            if !ca_path.is_empty() {
                if std::path::Path::new(&ca_path).exists() {
                    tls.ca_path = Some(ca_path);
                    tls.mutual_tls = self
                        .prompt_bool("Enable mutual TLS (client certificates)?", tls.mutual_tls)?;
                } else {
                    println!(
                        "   {} CA file not found, skipping.",
                        self.theme.warning("⚠️ ")
                    );
                }
            }

            self.config.tls = Some(tls);
        } else {
            self.config.tls = None;
        }

        println!("\n{} TLS configuration saved.", self.theme.success("✅"));
        Ok(())
    }

    /// Configure all settings step by step
    async fn configure_all(&mut self) -> anyhow::Result<()> {
        println!(
            "\n{}",
            self.theme.primary_bold("🚀 Complete Configuration Mode")
        );
        println!("This will guide you through all configuration sections.\n");

        self.configure_server().await?;
        self.configure_database().await?;
        self.configure_jwt().await?;
        self.configure_models().await?;
        self.configure_channels().await?;
        self.configure_blockchain().await?;
        self.configure_security().await?;
        self.configure_logging().await?;
        self.configure_metrics().await?;
        self.configure_tls().await?;

        Ok(())
    }

    /// Save configuration to file
    async fn save_config(&self) -> anyhow::Result<()> {
        // Ensure config directory exists
        std::fs::create_dir_all("config")?;

        let toml_content = toml::to_string_pretty(&self.config)?;
        std::fs::write("config/beebotos.toml", toml_content)?;

        println!(
            "\n{} Configuration saved to {}",
            self.theme.success("✅"),
            self.theme.primary_bold("config/beebotos.toml")
        );
        println!(
            "   You can edit this file manually or run with {} to change settings.",
            self.theme.warning("--reconfigure")
        );

        Ok(())
    }

    /// Configure from environment variables (non-interactive mode)
    fn config_from_env(&mut self) -> anyhow::Result<()> {
        // This uses the existing BeeBotOSConfig::load() logic
        self.config = BeeBotOSConfig::load()?;
        Ok(())
    }

    // Validation helper methods

    fn validate_host(host: &str) -> bool {
        // Allow localhost, IP addresses, and hostnames
        if host == "localhost" {
            return true;
        }

        // Check if it's a valid IP address
        if host.parse::<std::net::IpAddr>().is_ok() {
            return true;
        }

        // Check if it's a valid hostname (basic check)
        let hostname_regex = regex::Regex::new(r"^[a-zA-Z0-9]([a-zA-Z0-9\-]{0,61}[a-zA-Z0-9])?(\.[a-zA-Z0-9]([a-zA-Z0-9\-]{0,61}[a-zA-Z0-9])?)*$").unwrap();
        hostname_regex.is_match(host)
    }

    fn validate_port_available(_port: u16) -> bool {
        // In a real implementation, you could check if the port is in use
        // For now, just accept any valid port number
        true
    }

    fn validate_database_url(url: &str) -> bool {
        url.starts_with("sqlite://")
            || url.starts_with("postgres://")
            || url.starts_with("mysql://")
    }

    fn validate_url(url: &str) -> bool {
        url.starts_with("http://")
            || url.starts_with("https://")
            || url.starts_with("ws://")
            || url.starts_with("wss://")
    }

    fn validate_ethereum_address(addr: &str) -> bool {
        if addr.is_empty() {
            return true; // Empty is allowed (optional)
        }
        // Basic Ethereum address validation (0x followed by 40 hex characters)
        let addr_regex = regex::Regex::new(r"^0x[a-fA-F0-9]{40}$").unwrap();
        addr_regex.is_match(addr)
    }

    fn record_change(&mut self, category: &str, field: &str, old: &str, new: &str) {
        self.changes.push(ConfigChange {
            category: category.to_string(),
            field: field.to_string(),
            old_value: old.to_string(),
            new_value: new.to_string(),
        });
    }

    // Helper methods for user input

    fn prompt_string(&self, prompt: &str, default: &str) -> io::Result<String> {
        if default.is_empty() {
            print!("{}: ", self.theme.primary_bold(prompt));
        } else {
            print!(
                "{} [{}]: ",
                self.theme.primary_bold(prompt),
                self.theme.muted(default)
            );
        }
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        let trimmed = input.trim();
        if trimmed.is_empty() && !default.is_empty() {
            Ok(default.to_string())
        } else {
            Ok(trimmed.to_string())
        }
    }

    /// Prompt for password with hidden input
    fn prompt_password_hidden(&self, prompt: &str) -> io::Result<String> {
        print!("{}: ", self.theme.primary_bold(prompt));
        io::stdout().flush()?;

        match rpassword::read_password() {
            Ok(password) => Ok(password.trim().to_string()),
            Err(_) => {
                // Fallback to visible input if hidden input fails
                let mut input = String::new();
                io::stdin().read_line(&mut input)?;
                Ok(input.trim().to_string())
            }
        }
    }

    fn prompt_bool(&self, prompt: &str, default: bool) -> io::Result<bool> {
        let default_str = if default { "Y/n" } else { "y/N" };
        print!(
            "{} [{}]: ",
            self.theme.primary_bold(prompt),
            self.theme.muted(default_str)
        );
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        let trimmed = input.trim().to_lowercase();
        if trimmed.is_empty() {
            Ok(default)
        } else {
            Ok(trimmed == "y" || trimmed == "yes")
        }
    }

    fn prompt_choice(&self, prompt: &str, choices: &[&str], default: &str) -> io::Result<String> {
        println!("{}:", self.theme.primary_bold(prompt));
        for (i, choice) in choices.iter().enumerate() {
            let marker = if *choice == default { " *" } else { "" };
            println!(
                "  {}. {}{}",
                self.theme.info(&(i + 1).to_string()),
                self.theme.success(choice),
                self.theme.muted(marker)
            );
        }
        print!("Select (1-{}): ", choices.len());
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        let trimmed = input.trim();
        if let Ok(idx) = trimmed.parse::<usize>() {
            if idx > 0 && idx <= choices.len() {
                return Ok(choices[idx - 1].to_string());
            }
        }

        if trimmed.is_empty() {
            Ok(default.to_string())
        } else {
            Ok(trimmed.to_string())
        }
    }

    fn prompt_u16(&self, prompt: &str, default: u16, min: u16, max: u16) -> io::Result<u16> {
        loop {
            print!(
                "{} [{}]: ",
                self.theme.primary_bold(prompt),
                self.theme.muted(&default.to_string())
            );
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            let trimmed = input.trim();
            if trimmed.is_empty() {
                return Ok(default);
            }

            if let Ok(val) = trimmed.parse::<u16>() {
                if val >= min && val <= max {
                    return Ok(val);
                }
            }
            println!(
                "   {} Please enter a number between {} and {}",
                self.theme.error("❌"),
                min,
                max
            );
        }
    }

    fn prompt_u32(&self, prompt: &str, default: u32, min: u32, max: u32) -> io::Result<u32> {
        loop {
            print!(
                "{} [{}]: ",
                self.theme.primary_bold(prompt),
                self.theme.muted(&default.to_string())
            );
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            let trimmed = input.trim();
            if trimmed.is_empty() {
                return Ok(default);
            }

            if let Ok(val) = trimmed.parse::<u32>() {
                if val >= min && val <= max {
                    return Ok(val);
                }
            }
            println!(
                "   {} Please enter a number between {} and {}",
                self.theme.error("❌"),
                min,
                max
            );
        }
    }

    fn prompt_u64(&self, prompt: &str, default: u64, min: u64, max: u64) -> io::Result<u64> {
        loop {
            print!(
                "{} [{}]: ",
                self.theme.primary_bold(prompt),
                self.theme.muted(&default.to_string())
            );
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            let trimmed = input.trim();
            if trimmed.is_empty() {
                return Ok(default);
            }

            if let Ok(val) = trimmed.parse::<u64>() {
                if val >= min && val <= max {
                    return Ok(val);
                }
            }
            println!(
                "   {} Please enter a number between {} and {}",
                self.theme.error("❌"),
                min,
                max
            );
        }
    }

    fn prompt_i64(&self, prompt: &str, default: i64, min: i64, max: i64) -> io::Result<i64> {
        loop {
            print!(
                "{} [{}]: ",
                self.theme.primary_bold(prompt),
                self.theme.muted(&default.to_string())
            );
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            let trimmed = input.trim();
            if trimmed.is_empty() {
                return Ok(default);
            }

            if let Ok(val) = trimmed.parse::<i64>() {
                if val >= min && val <= max {
                    return Ok(val);
                }
            }
            println!(
                "   {} Please enter a number between {} and {}",
                self.theme.error("❌"),
                min,
                max
            );
        }
    }

    fn prompt_usize(
        &self,
        prompt: &str,
        default: usize,
        min: usize,
        max: usize,
    ) -> io::Result<usize> {
        loop {
            print!(
                "{} [{}]: ",
                self.theme.primary_bold(prompt),
                self.theme.muted(&default.to_string())
            );
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            let trimmed = input.trim();
            if trimmed.is_empty() {
                return Ok(default);
            }

            if let Ok(val) = trimmed.parse::<usize>() {
                if val >= min && val <= max {
                    return Ok(val);
                }
            }
            println!(
                "   {} Please enter a number between {} and {}",
                self.theme.error("❌"),
                min,
                max
            );
        }
    }

    fn generate_jwt_secret() -> String {
        use rand::Rng;
        const CHARSET: &[u8] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!@#$%^&*";
        let mut rng = rand::thread_rng();

        (0..64)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect()
    }
}

/// Check if running in interactive mode
pub fn is_interactive_mode() -> bool {
    // Check for --wizard or --interactive flags
    let args: Vec<String> = std::env::args().collect();
    args.contains(&"--wizard".to_string())
        || args.contains(&"--interactive".to_string())
        || args.contains(&"--configure".to_string())
        || args.contains(&"--reconfigure".to_string())
}

/// Run configuration wizard if needed
///
/// Theme priority (highest to lowest):
/// 1. Command line arguments (--theme, --no-color)
/// 2. Environment variables (BEE__WIZARD__COLOR_THEME, NO_COLOR)
/// 3. Config file wizard.color_theme setting
/// 4. Auto-detection (CI/non-TTY = no_color)
/// 5. Default theme
pub async fn run_wizard_if_needed() -> anyhow::Result<BeeBotOSConfig> {
    let is_interactive = is_interactive_mode();
    let config_exists = ConfigWizard::config_exists();

    if is_interactive || !config_exists {
        // Determine theme: check if already set from command line, otherwise use config
        // or default
        let args: Vec<String> = std::env::args().collect();
        let theme = if let Some(cmd_theme) = ColorTheme::from_args(&args) {
            cmd_theme
        } else if config_exists {
            // Try to load theme from existing config
            if let Ok(config) = BeeBotOSConfig::load() {
                config.wizard.color_theme
            } else {
                ColorTheme::detect_from_env()
            }
        } else {
            ColorTheme::detect_from_env()
        };

        let mut wizard = ConfigWizard::with_theme(true, theme);

        // Check if we should show completion message before running wizard
        let show_completion = is_interactive && !config_exists;

        let mut config = wizard.run().await?;

        // Save the theme to config for future runs
        config.wizard.color_theme = theme;

        if show_completion {
            println!(
                "\n{} Configuration complete! Starting BeeBotOS Gateway...\n",
                wizard.theme.success_bold("🎉")
            );
        }

        Ok(config)
    } else {
        // Normal mode: just load existing config
        Ok(BeeBotOSConfig::load()?)
    }
}

/// Export configuration to different formats
pub fn export_config(format: &str) -> anyhow::Result<String> {
    let config = BeeBotOSConfig::load()?;

    match format {
        "env" => {
            // Export as environment variables
            let mut output = String::new();
            output.push_str(&format!("BEE__SERVER__HOST={}\n", config.server.host));
            output.push_str(&format!("BEE__SERVER__PORT={}\n", config.server.port));
            output.push_str(&format!("BEE__DATABASE__URL={}\n", config.database.url));
            output.push_str(&format!(
                "BEE__JWT__SECRET={}\n",
                config.jwt.secret.expose_secret()
            ));
            output.push_str(&format!(
                "BEE__MODELS__DEFAULT_PROVIDER={}\n",
                config.models.default_provider
            ));
            Ok(output)
        }
        "docker" => {
            // Export as Docker Compose environment
            let mut output = String::new();
            output.push_str("environment:\n");
            output.push_str(&format!("  - BEE__SERVER__HOST={}\n", config.server.host));
            output.push_str(&format!("  - BEE__SERVER__PORT={}\n", config.server.port));
            output.push_str(&format!("  - BEE__DATABASE__URL={}\n", config.database.url));
            Ok(output)
        }
        "k8s" => {
            // Export as Kubernetes ConfigMap
            let mut output = String::new();
            output.push_str(
                "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: beebotos-config\ndata:\n",
            );
            output.push_str(&format!(
                "  BEE__SERVER__HOST: \"{}\"\n",
                config.server.host
            ));
            output.push_str(&format!(
                "  BEE__SERVER__PORT: \"{}\"\n",
                config.server.port
            ));
            output.push_str(&format!(
                "  BEE__DATABASE__URL: \"{}\"\n",
                config.database.url
            ));
            Ok(output)
        }
        _ => Err(anyhow::anyhow!("Unsupported export format: {}", format)),
    }
}

/// Import configuration from .env file
pub fn import_from_env_file(path: &str) -> anyhow::Result<BeeBotOSConfig> {
    use std::io::BufRead;

    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);

    for line in reader.lines() {
        let line = line?;
        let line = line.trim();

        // Skip comments and empty lines
        if line.starts_with('#') || line.is_empty() {
            continue;
        }

        // Parse KEY=VALUE format
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim().trim_matches('"').trim_matches('\'');

            // Set environment variable
            std::env::set_var(key, value);
        }
    }

    // Load config from environment
    BeeBotOSConfig::load().map_err(|e| anyhow::anyhow!("Failed to load config: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_category_names() {
        assert_eq!(ConfigCategory::Server.name(), "Server Settings");
        assert_eq!(ConfigCategory::Database.name(), "Database Configuration");
        assert_eq!(ConfigCategory::Jwt.name(), "JWT Authentication");
        assert_eq!(ConfigCategory::Models.name(), "AI/LLM Models");
        assert_eq!(ConfigCategory::Channels.name(), "Communication Channels");
        assert_eq!(ConfigCategory::Blockchain.name(), "Blockchain/Web3");
        assert_eq!(ConfigCategory::Security.name(), "Security Settings");
        assert_eq!(ConfigCategory::Logging.name(), "Logging & Tracing");
        assert_eq!(ConfigCategory::Metrics.name(), "Metrics & Monitoring");
        assert_eq!(ConfigCategory::Tls.name(), "TLS/SSL Configuration");
        assert_eq!(ConfigCategory::All.name(), "Configure All");
        assert_eq!(ConfigCategory::Skip.name(), "Skip (use defaults)");
    }

    #[test]
    fn test_config_category_descriptions() {
        assert!(ConfigCategory::Server.description().contains("HTTP server"));
        assert!(ConfigCategory::Database.description().contains("SQLite"));
        assert!(ConfigCategory::Jwt.description().contains("JWT"));
        assert!(ConfigCategory::Models.description().contains("LLM"));
        assert!(ConfigCategory::Channels.description().contains("Messaging"));
    }

    #[test]
    fn test_config_wizard_creation() {
        let wizard = ConfigWizard::new(false);
        assert!(!wizard.interactive);
    }

    #[test]
    fn test_jwt_secret_generation() {
        let secret1 = ConfigWizard::generate_jwt_secret();
        let secret2 = ConfigWizard::generate_jwt_secret();

        // Secrets should be different (random)
        assert_ne!(secret1, secret2);

        // Secret should be at least 32 characters
        assert!(secret1.len() >= 32);
        assert!(secret2.len() >= 32);
    }

    #[test]
    fn test_validate_host() {
        assert!(ConfigWizard::validate_host("localhost"));
        assert!(ConfigWizard::validate_host("127.0.0.1"));
        assert!(ConfigWizard::validate_host("0.0.0.0"));
        assert!(ConfigWizard::validate_host("192.168.1.1"));
        assert!(ConfigWizard::validate_host("example.com"));
        assert!(ConfigWizard::validate_host("api.example.com"));
        assert!(!ConfigWizard::validate_host(""));
        assert!(!ConfigWizard::validate_host("invalid host!"));
    }

    #[test]
    fn test_validate_database_url() {
        assert!(ConfigWizard::validate_database_url("sqlite://./data.db"));
        assert!(ConfigWizard::validate_database_url(
            "postgres://localhost/db"
        ));
        assert!(ConfigWizard::validate_database_url("mysql://localhost/db"));
        assert!(!ConfigWizard::validate_database_url("invalid://url"));
        assert!(!ConfigWizard::validate_database_url("not_a_url"));
    }

    #[test]
    fn test_validate_url() {
        assert!(ConfigWizard::validate_url("http://localhost:8080"));
        assert!(ConfigWizard::validate_url("https://api.example.com"));
        assert!(ConfigWizard::validate_url("ws://localhost:8080"));
        assert!(ConfigWizard::validate_url("wss://secure.example.com"));
        assert!(!ConfigWizard::validate_url("not_a_url"));
        assert!(!ConfigWizard::validate_url("ftp://files.example.com"));
    }

    #[test]
    fn test_validate_ethereum_address() {
        assert!(ConfigWizard::validate_ethereum_address("")); // Empty is allowed
        assert!(ConfigWizard::validate_ethereum_address(
            "0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb0"
        ));
        assert!(ConfigWizard::validate_ethereum_address(
            "0x742d35Cc6634C0532925a3b844Bc9e7595f0bEBD"
        ));
        assert!(!ConfigWizard::validate_ethereum_address("0xinvalid"));
        assert!(!ConfigWizard::validate_ethereum_address("not_an_address"));
        assert!(!ConfigWizard::validate_ethereum_address(
            "742d35Cc6634C0532925a3b844Bc9e7595f0bEBD"
        )); // Missing 0x
    }

    #[test]
    fn test_is_interactive_mode() {
        // Save original args
        let original_args: Vec<String> = std::env::args().collect();

        // Test without flags
        assert!(!is_interactive_mode());

        // Note: We can't easily test with flags set because
        // std::env::args() reads process arguments at startup
    }

    #[test]
    fn test_default_config_creation() {
        let config = BeeBotOSConfig::default();

        // Check default values
        assert!(!config.system_name.is_empty());
        assert!(!config.version.is_empty());
        assert!(!config.server.host.is_empty());
        assert!(config.server.port > 0);
        assert!(!config.database.url.is_empty());
    }

    #[tokio::test]
    async fn test_config_save_and_load() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("beebotos.toml");

        // Create a test config
        let config = BeeBotOSConfig {
            system_name: "TestBeeBotOS".to_string(),
            version: "1.0.0".to_string(),
            server: crate::config::ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 9999,
                ..Default::default()
            },
            ..Default::default()
        };

        // Save config
        let toml_content = toml::to_string_pretty(&config).unwrap();
        std::fs::write(&config_path, toml_content).unwrap();

        // Verify file exists
        assert!(config_path.exists());

        // Load and verify
        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("TestBeeBotOS"));
        assert!(content.contains("127.0.0.1"));
        assert!(content.contains("9999"));
    }

    #[test]
    fn test_export_config() {
        // This test requires a valid config file, so we skip it in CI
        // In real tests, you'd set up a temporary config
    }

    #[test]
    fn test_config_change_tracking() {
        let mut wizard = ConfigWizard::new(false);

        wizard.record_change("server", "port", "8080", "9090");
        wizard.record_change("database", "url", "old.db", "new.db");

        assert_eq!(wizard.changes.len(), 2);
        assert_eq!(wizard.changes[0].category, "server");
        assert_eq!(wizard.changes[0].field, "port");
        assert_eq!(wizard.changes[0].old_value, "8080");
        assert_eq!(wizard.changes[0].new_value, "9090");
    }
}
