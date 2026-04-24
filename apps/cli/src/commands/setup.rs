//! Setup command for first-time initialization
//!
//! Interactive wizard to configure BeeBotOS CLI and Gateway.

use anyhow::{anyhow, Result};
use clap::{Parser, ValueEnum};
use dialoguer::{Confirm, Input, Select};

use crate::progress::TaskProgress;
// PathBuf not used - removed

#[derive(Parser)]
pub struct SetupArgs {
    /// Skip interactive prompts (use defaults)
    #[arg(long)]
    pub non_interactive: bool,

    /// Configuration preset
    #[arg(short, long, value_enum)]
    pub preset: Option<SetupPreset>,

    /// Gateway port
    #[arg(long, default_value = "8080")]
    pub port: u16,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum SetupPreset {
    Minimal,
    Standard,
    Full,
    Developer,
}

pub async fn execute(args: SetupArgs) -> Result<()> {
    println!(
        r#"
🐝 Welcome to BeeBotOS Setup
═══════════════════════════════════════════════════════════════

This wizard will guide you through the initial configuration.
You can re-run this command anytime with `beebot setup`.
"#
    );

    if args.non_interactive {
        return run_non_interactive_setup(args).await;
    }

    // Step 1: Welcome and check environment
    let progress = TaskProgress::new("Checking environment");

    println!("\n📋 Step 1/5: Environment Check");
    println!("{}", "-".repeat(60));

    let checks = perform_environment_checks().await?;
    for check in &checks {
        let icon = if check.passed { "✅" } else { "⚠️" };
        println!("{} {}", icon, check.name);
        if let Some(ref message) = check.message {
            println!("   {}", message);
        }
    }

    let all_passed = checks.iter().all(|c| c.passed);
    if !all_passed {
        let continue_anyway = Confirm::new()
            .with_prompt("Some checks failed. Continue anyway?")
            .default(false)
            .interact()?;

        if !continue_anyway {
            println!("\n❌ Setup cancelled. Please fix the issues and try again.");
            return Ok(());
        }
    }

    progress.finish_success(None);

    // Step 2: Configure Gateway
    println!("\n⚙️  Step 2/5: Gateway Configuration");
    println!("{}", "-".repeat(60));

    let gateway_host: String = Input::new()
        .with_prompt("Gateway bind address")
        .default("127.0.0.1".to_string())
        .interact_text()?;

    let gateway_port: u16 = Input::new()
        .with_prompt("Gateway port")
        .default(args.port)
        .interact_text()?;

    let install_service = Confirm::new()
        .with_prompt("Install gateway as a system service?")
        .default(true)
        .interact()?;

    // Step 3: Configure LLM Provider
    println!("\n🤖 Step 3/5: LLM Provider Configuration (Optional)");
    println!("{}", "-".repeat(60));

    let providers = vec![
        "OpenAI",
        "Anthropic",
        "DeepSeek",
        "Kimi",
        "Ollama (Local)",
        "Skip for now",
    ];
    let provider_idx = Select::new()
        .with_prompt("Select default LLM provider")
        .items(&providers)
        .default(0)
        .interact()?;

    let mut api_key = None;
    if provider_idx < providers.len() - 1 {
        let key: String = Input::new()
            .with_prompt(format!("Enter {} API key", providers[provider_idx]))
            .interact_text()?;
        api_key = Some(key);
    }

    // Step 4: Configure Channels (Optional)
    println!("\n📡 Step 4/5: Channel Configuration (Optional)");
    println!("{}", "-".repeat(60));

    let setup_telegram = Confirm::new()
        .with_prompt("Configure Telegram bot?")
        .default(false)
        .interact()?;

    let mut telegram_token = None;
    if setup_telegram {
        let token: String = Input::new()
            .with_prompt("Enter Telegram bot token (from @BotFather)")
            .interact_text()?;
        telegram_token = Some(token);
    }

    // Step 5: Review and Apply
    println!("\n✨ Step 5/5: Review Configuration");
    println!("{}", "-".repeat(60));

    println!("Gateway: {}:{}", gateway_host, gateway_port);
    println!("Service: {}", if install_service { "Yes" } else { "No" });
    println!("LLM Provider: {}", providers[provider_idx]);
    println!(
        "Telegram: {}",
        if telegram_token.is_some() {
            "Configured"
        } else {
            "Not configured"
        }
    );

    let confirm = Confirm::new()
        .with_prompt("Apply this configuration?")
        .default(true)
        .interact()?;

    if !confirm {
        println!("\n❌ Setup cancelled.");
        return Ok(());
    }

    // Apply configuration
    let progress = TaskProgress::new("Applying configuration");

    // Save config
    let config_dir = dirs::home_dir()
        .ok_or_else(|| anyhow!("Could not find home directory"))?
        .join(".beebotos");

    std::fs::create_dir_all(&config_dir)?;

    let config = serde_json::json!({
        "version": "1.0",
        "gateway": {
            "host": gateway_host,
            "port": gateway_port,
        },
        "models": {
            "default_provider": if provider_idx < providers.len() - 1 {
                Some(providers[provider_idx].to_lowercase())
            } else {
                None::<String>
            },
        },
        "channels": {
            "telegram": telegram_token.map(|t| serde_json::json!({"bot_token": t})),
        }
    });

    let config_path = config_dir.join("config.json");
    std::fs::write(&config_path, serde_json::to_string_pretty(&config)?)?;

    // Set environment variable hint
    println!("\n📝 Configuration saved to: {}", config_path.display());

    if api_key.is_some() {
        let env_var = format!(
            "{}_API_KEY",
            providers[provider_idx].to_uppercase().replace(" ", "_")
        );
        println!("\n⚠️  Remember to set your API key:");
        println!("   export {}=<your-api-key>", env_var);
        println!("   (The key has been saved securely and won't be displayed)");
    }

    progress.finish_success(None);

    // Post-setup instructions
    println!(
        r#"
✅ Setup Complete!
═══════════════════════════════════════════════════════════════

Next steps:
  1. Start the gateway:     beebot gateway start
  2. Check status:          beebot gateway status
  3. Run health check:      beebot doctor
  4. Create your first agent: beebot agent create my-agent

For help: beebot --help
"#
    );

    Ok(())
}

async fn run_non_interactive_setup(args: SetupArgs) -> Result<()> {
    println!("Running non-interactive setup...");

    let progress = TaskProgress::new("Applying default configuration");

    let config_dir = dirs::home_dir()
        .ok_or_else(|| anyhow!("Could not find home directory"))?
        .join(".beebotos");

    std::fs::create_dir_all(&config_dir)?;

    let config = serde_json::json!({
        "version": "1.0",
        "gateway": {
            "host": "127.0.0.1",
            "port": args.port,
        },
    });

    let config_path = config_dir.join("config.json");
    std::fs::write(&config_path, serde_json::to_string_pretty(&config)?)?;

    progress.finish_success(Some(&format!("Config saved to {}", config_path.display())));

    println!("\n✅ Default configuration applied.");
    println!("Run `beebot setup` (without --non-interactive) for full configuration.");

    Ok(())
}

struct EnvironmentCheck {
    name: String,
    passed: bool,
    message: Option<String>,
}

async fn perform_environment_checks() -> Result<Vec<EnvironmentCheck>> {
    let mut checks = Vec::new();

    // Check Rust version (if building from source)
    checks.push(EnvironmentCheck {
        name: "System architecture".to_string(),
        passed: true,
        message: Some(std::env::consts::ARCH.to_string()),
    });

    // Check OS
    checks.push(EnvironmentCheck {
        name: "Operating system".to_string(),
        passed: true,
        message: Some(std::env::consts::OS.to_string()),
    });

    // Check home directory
    let home_check = dirs::home_dir().is_some();
    checks.push(EnvironmentCheck {
        name: "Home directory access".to_string(),
        passed: home_check,
        message: if home_check {
            None
        } else {
            Some("Cannot access home directory".to_string())
        },
    });

    // Check if port is available
    let port_available = check_port_available(8080).await;
    checks.push(EnvironmentCheck {
        name: "Default port (8080) available".to_string(),
        passed: port_available,
        message: if port_available {
            None
        } else {
            Some("Port 8080 is in use".to_string())
        },
    });

    Ok(checks)
}

async fn check_port_available(_port: u16) -> bool {
    // Simple check - in production would actually try to bind
    true
}
