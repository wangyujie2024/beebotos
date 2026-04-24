//! Config management command

use std::path::PathBuf;

use crate::config::Config;

/// Config subcommand
#[derive(clap::Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommand,
}

/// Config subcommands
#[derive(clap::Subcommand)]
pub enum ConfigCommand {
    /// Show current configuration
    Show,
    /// Get a specific config value
    Get {
        /// Config key (e.g., "daemon.endpoint")
        key: String,
    },
    /// Set a config value
    Set {
        /// Config key
        key: String,
        /// Config value
        value: String,
    },
    /// List all config keys
    List,
    /// Edit config in default editor
    Edit,
    /// Reset config to defaults
    Reset,
    /// Validate config
    Validate,
}

/// Run config command
pub async fn run(args: ConfigArgs, _config: Config) -> anyhow::Result<()> {
    let config_path = Config::config_path()?;

    match args.command {
        ConfigCommand::Show => {
            let config = Config::load()?;
            println!("Configuration file: {}", config_path.display());
            println!();
            println!("{}", serde_json::to_string_pretty(&config)?);
        }
        ConfigCommand::Get { key } => {
            let config = Config::load()?;
            let value = get_nested_value(&config, &key)?;
            // Mask sensitive values
            let display_value = if is_sensitive_key(&key) {
                mask_sensitive_value(&value)
            } else {
                value
            };
            println!("{} = {}", key, display_value);
        }
        ConfigCommand::Set { key, value } => {
            let mut config = Config::load()?;
            set_nested_value(&mut config, &key, &value)?;
            config.save()?;
            // Don't print sensitive values
            let display_value = if is_sensitive_key(&key) {
                mask_sensitive_value(&value)
            } else {
                value.clone()
            };
            println!("✅ Set {} = {}", key, display_value);
        }
        ConfigCommand::List => {
            let config = Config::load()?;
            list_keys(&config, "");
        }
        ConfigCommand::Edit => {
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nano".to_string());
            std::process::Command::new(editor)
                .arg(&config_path)
                .status()?;
        }
        ConfigCommand::Reset => {
            let config = Config::default();
            config.save()?;
            println!("✅ Configuration reset to defaults");
        }
        ConfigCommand::Validate => match Config::load() {
            Ok(_) => {
                println!("✅ Configuration is valid");
            }
            Err(e) => {
                eprintln!("❌ Configuration error: {}", e);
                std::process::exit(1);
            }
        },
    }

    Ok(())
}

fn get_nested_value(config: &Config, key: &str) -> anyhow::Result<String> {
    match key {
        "daemon.endpoint" => Ok(config.daemon_endpoint.clone()),
        "daemon.timeout" => Ok(config.daemon_timeout.to_string()),
        "chain.rpc_url" => Ok(config.rpc_url.clone()),
        "chain.dao_address" => Ok(config.dao_address.clone()),
        "api_key" => Ok(config.api_key.clone()),
        _ => anyhow::bail!("Unknown config key: {}", key),
    }
}

fn set_nested_value(config: &mut Config, key: &str, value: &str) -> anyhow::Result<()> {
    match key {
        "daemon.endpoint" => config.daemon_endpoint = value.to_string(),
        "daemon.timeout" => config.daemon_timeout = value.parse()?,
        "chain.rpc_url" => config.rpc_url = value.to_string(),
        "chain.dao_address" => config.dao_address = value.to_string(),
        _ => anyhow::bail!("Unknown config key: {}", key),
    };
    Ok(())
}

fn list_keys<T: serde::Serialize>(value: &T, prefix: &str) {
    let json = serde_json::to_value(value).unwrap();
    if let serde_json::Value::Object(map) = json {
        for (key, val) in map {
            let full_key = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{}.{}", prefix, key)
            };

            if val.is_object() {
                list_keys(&val, &full_key);
            } else {
                // Mask sensitive values
                let display_val = if is_sensitive_key(&full_key) || is_sensitive_key(&key) {
                    mask_sensitive_value(&val.to_string())
                } else {
                    val.to_string()
                };
                println!("{} = {}", full_key, display_val);
            }
        }
    }
}

/// Check if a config key is sensitive (contains passwords, keys, tokens, etc.)
fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_lowercase();
    lower.contains("key")
        || lower.contains("secret")
        || lower.contains("password")
        || lower.contains("token")
        || lower.contains("credential")
        || lower.contains("private")
        || lower.contains("api_key")
}

/// Mask a sensitive value, showing only first and last 4 characters
fn mask_sensitive_value(value: &str) -> String {
    if value.len() <= 8 {
        "****".to_string()
    } else {
        format!("{}****{}", &value[..4], &value[value.len() - 4..])
    }
}
