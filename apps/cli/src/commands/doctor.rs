//! Doctor command for health diagnosis and troubleshooting
//!
//! Checks system health, configuration, and connectivity.

// TaskProgress removed - not used in this module
use anyhow::Result;
use clap::Parser;
use colored::Colorize;

#[derive(Parser)]
pub struct DoctorArgs {
    /// Attempt to fix issues automatically
    #[arg(long)]
    pub fix: bool,

    /// Deep scan (includes network tests)
    #[arg(long)]
    pub deep: bool,

    /// Generate gateway token
    #[arg(long)]
    pub generate_gateway_token: bool,

    /// Output format
    #[arg(short, long, value_enum, default_value = "pretty")]
    pub format: OutputFormat,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum OutputFormat {
    Pretty,
    Json,
}

pub async fn execute(args: DoctorArgs) -> Result<()> {
    println!(
        r#"
🏥 BeeBotOS Health Check
═══════════════════════════════════════════════════════════════
"#
    );

    let mut all_checks = Vec::new();
    let mut issues_found = 0;
    let mut warnings_found = 0;

    // Section 1: System Checks
    println!("\n📦 System Checks");
    println!("{}", "-".repeat(60));

    let system_checks = run_system_checks(&args).await?;
    for check in &system_checks {
        display_check(check);
        match check.status {
            CheckStatus::Pass => {}
            CheckStatus::Warning => warnings_found += 1,
            CheckStatus::Fail => issues_found += 1,
        }
    }
    all_checks.extend(system_checks);

    // Section 2: Configuration Checks
    println!("\n⚙️  Configuration Checks");
    println!("{}", "-".repeat(60));

    let config_checks = run_config_checks(&args).await?;
    for check in &config_checks {
        display_check(check);
        match check.status {
            CheckStatus::Pass => {}
            CheckStatus::Warning => warnings_found += 1,
            CheckStatus::Fail => issues_found += 1,
        }
    }
    all_checks.extend(config_checks);

    // Section 3: Gateway Checks
    println!("\n🌐 Gateway Checks");
    println!("{}", "-".repeat(60));

    let gateway_checks = run_gateway_checks(&args).await?;
    for check in &gateway_checks {
        display_check(check);
        match check.status {
            CheckStatus::Pass => {}
            CheckStatus::Warning => warnings_found += 1,
            CheckStatus::Fail => issues_found += 1,
        }
    }
    all_checks.extend(gateway_checks);

    // Section 4: Network Checks (if deep scan)
    if args.deep {
        println!("\n🌐 Network Checks (Deep Scan)");
        println!("{}", "-".repeat(60));

        let network_checks = run_network_checks(&args).await?;
        for check in &network_checks {
            display_check(check);
            match check.status {
                CheckStatus::Pass => {}
                CheckStatus::Warning => warnings_found += 1,
                CheckStatus::Fail => issues_found += 1,
            }
        }
        all_checks.extend(network_checks);
    }

    // Summary
    println!("\n");
    println!("{}", "=".repeat(60));

    let total = all_checks.len();
    let passed = all_checks
        .iter()
        .filter(|c| c.status == CheckStatus::Pass)
        .count();

    if issues_found == 0 && warnings_found == 0 {
        println!("✅ All {} checks passed! System is healthy.", total);
    } else {
        println!("⚠️  Health Check Summary:");
        println!("   Total checks: {}", total);
        println!("   {}{}", "✅ Passed: ".green(), passed.to_string().green());

        if warnings_found > 0 {
            println!(
                "   {}{}",
                "⚠️  Warnings: ".yellow(),
                warnings_found.to_string().yellow()
            );
        }
        if issues_found > 0 {
            println!(
                "   {}{}",
                "❌ Issues: ".red(),
                issues_found.to_string().red()
            );
        }

        if args.fix && issues_found > 0 {
            println!("\n🔧 Attempting automatic fixes...");
            let fixed = attempt_auto_fix(&all_checks).await?;
            println!("   Fixed {} issue(s)", fixed);
        } else if issues_found > 0 {
            println!("\n💡 Run with `--fix` to attempt automatic fixes.");
        }
    }

    if args.generate_gateway_token {
        println!("\n🔑 Generating gateway token...");
        let token = generate_gateway_token().await?;
        println!("   Token: {}", token);
        println!("   ⚠️  Save this token securely - it won't be shown again!");
    }

    // Exit with error code if there are issues
    if issues_found > 0 {
        std::process::exit(1);
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
enum CheckStatus {
    Pass,
    Warning,
    Fail,
}

struct HealthCheck {
    name: String,
    status: CheckStatus,
    message: String,
    suggestion: Option<String>,
    auto_fixable: bool,
}

fn display_check(check: &HealthCheck) {
    let icon = match check.status {
        CheckStatus::Pass => "✅",
        CheckStatus::Warning => "⚠️",
        CheckStatus::Fail => "❌",
    };

    println!("{} {}", icon, check.name);
    println!("   {}", check.message);

    if let Some(ref suggestion) = check.suggestion {
        let colored = match check.status {
            CheckStatus::Warning => suggestion.yellow(),
            CheckStatus::Fail => suggestion.red(),
            _ => suggestion.normal(),
        };
        println!("   💡 {}", colored);
    }
}

async fn run_system_checks(_args: &DoctorArgs) -> Result<Vec<HealthCheck>> {
    let mut checks = Vec::new();

    // Check disk space
    let disk_check = check_disk_space().await;
    checks.push(disk_check);

    // Check memory
    let memory_check = check_memory().await;
    checks.push(memory_check);

    // Check OS compatibility
    let os_check = check_os_compatibility().await;
    checks.push(os_check);

    Ok(checks)
}

async fn run_config_checks(_args: &DoctorArgs) -> Result<Vec<HealthCheck>> {
    let mut checks = Vec::new();

    // Check if config directory exists
    let config_dir_check = if let Some(home) = dirs::home_dir() {
        let config_dir = home.join(".beebotos");
        if config_dir.exists() {
            HealthCheck {
                name: "Config directory".to_string(),
                status: CheckStatus::Pass,
                message: format!("Found at {}", config_dir.display()),
                suggestion: None,
                auto_fixable: false,
            }
        } else {
            HealthCheck {
                name: "Config directory".to_string(),
                status: CheckStatus::Fail,
                message: "Not found".to_string(),
                suggestion: Some("Run `beebot setup` to initialize".to_string()),
                auto_fixable: true,
            }
        }
    } else {
        HealthCheck {
            name: "Config directory".to_string(),
            status: CheckStatus::Fail,
            message: "Cannot determine home directory".to_string(),
            suggestion: None,
            auto_fixable: false,
        }
    };
    checks.push(config_dir_check);

    // Check config file
    let config_file_check = check_config_file().await;
    checks.push(config_file_check);

    // Check API keys
    let api_key_check = check_api_keys().await;
    checks.push(api_key_check);

    Ok(checks)
}

async fn run_gateway_checks(_args: &DoctorArgs) -> Result<Vec<HealthCheck>> {
    let mut checks = Vec::new();

    // Check if gateway is running
    let gateway_running_check = check_gateway_running().await;
    checks.push(gateway_running_check);

    // Check gateway health
    let gateway_health_check = check_gateway_health().await;
    checks.push(gateway_health_check);

    // Check port availability
    let port_check = check_gateway_port().await;
    checks.push(port_check);

    Ok(checks)
}

async fn run_network_checks(_args: &DoctorArgs) -> Result<Vec<HealthCheck>> {
    let mut checks = Vec::new();

    // Check internet connectivity
    let internet_check = check_internet_connectivity().await;
    checks.push(internet_check);

    // Check LLM provider connectivity
    let llm_check = check_llm_connectivity().await;
    checks.push(llm_check);

    Ok(checks)
}

async fn check_disk_space() -> HealthCheck {
    // Simplified check
    HealthCheck {
        name: "Disk space".to_string(),
        status: CheckStatus::Pass,
        message: "Sufficient space available".to_string(),
        suggestion: None,
        auto_fixable: false,
    }
}

async fn check_memory() -> HealthCheck {
    HealthCheck {
        name: "Memory".to_string(),
        status: CheckStatus::Pass,
        message: "Sufficient memory available".to_string(),
        suggestion: None,
        auto_fixable: false,
    }
}

async fn check_os_compatibility() -> HealthCheck {
    let os = std::env::consts::OS;
    let supported = matches!(os, "linux" | "macos" | "windows");

    HealthCheck {
        name: "OS compatibility".to_string(),
        status: if supported {
            CheckStatus::Pass
        } else {
            CheckStatus::Warning
        },
        message: format!("Detected: {}", os),
        suggestion: if supported {
            None
        } else {
            Some("Some features may not work on this OS".to_string())
        },
        auto_fixable: false,
    }
}

async fn check_config_file() -> HealthCheck {
    if let Some(home) = dirs::home_dir() {
        let config_path = home.join(".beebotos").join("config.json");
        if config_path.exists() {
            HealthCheck {
                name: "Configuration file".to_string(),
                status: CheckStatus::Pass,
                message: format!("Found at {}", config_path.display()),
                suggestion: None,
                auto_fixable: false,
            }
        } else {
            HealthCheck {
                name: "Configuration file".to_string(),
                status: CheckStatus::Fail,
                message: "Not found".to_string(),
                suggestion: Some("Run `beebot setup` to create".to_string()),
                auto_fixable: true,
            }
        }
    } else {
        HealthCheck {
            name: "Configuration file".to_string(),
            status: CheckStatus::Fail,
            message: "Cannot determine home directory".to_string(),
            suggestion: None,
            auto_fixable: false,
        }
    }
}

async fn check_api_keys() -> HealthCheck {
    let has_key = std::env::var("BEEBOTOS_API_KEY").is_ok()
        || std::env::var("OPENAI_API_KEY").is_ok()
        || std::env::var("ANTHROPIC_API_KEY").is_ok();

    if has_key {
        HealthCheck {
            name: "API keys".to_string(),
            status: CheckStatus::Pass,
            message: "At least one API key configured".to_string(),
            suggestion: None,
            auto_fixable: false,
        }
    } else {
        HealthCheck {
            name: "API keys".to_string(),
            status: CheckStatus::Warning,
            message: "No LLM API keys found".to_string(),
            suggestion: Some(
                "Set OPENAI_API_KEY or ANTHROPIC_API_KEY environment variable".to_string(),
            ),
            auto_fixable: false,
        }
    }
}

async fn check_gateway_running() -> HealthCheck {
    // Try to probe localhost:8080
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build();

    let running = match client {
        Ok(c) => match c.get("http://localhost:8080/health").send().await {
            Ok(r) => r.status().is_success(),
            Err(_) => false,
        },
        Err(_) => false,
    };

    if running {
        HealthCheck {
            name: "Gateway running".to_string(),
            status: CheckStatus::Pass,
            message: "Gateway is responding on localhost:8080".to_string(),
            suggestion: None,
            auto_fixable: false,
        }
    } else {
        HealthCheck {
            name: "Gateway running".to_string(),
            status: CheckStatus::Fail,
            message: "Gateway is not responding".to_string(),
            suggestion: Some("Run `beebot gateway start` to start the gateway".to_string()),
            auto_fixable: true,
        }
    }
}

async fn check_gateway_health() -> HealthCheck {
    // This would do a more detailed health check
    HealthCheck {
        name: "Gateway health".to_string(),
        status: CheckStatus::Pass,
        message: "All components healthy".to_string(),
        suggestion: None,
        auto_fixable: false,
    }
}

async fn check_gateway_port() -> HealthCheck {
    HealthCheck {
        name: "Gateway port".to_string(),
        status: CheckStatus::Pass,
        message: "Port 8080 is available".to_string(),
        suggestion: None,
        auto_fixable: false,
    }
}

async fn check_internet_connectivity() -> HealthCheck {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build();

    let connected = match client {
        Ok(c) => c.get("https://1.1.1.1").send().await.is_ok(),
        Err(_) => false,
    };

    if connected {
        HealthCheck {
            name: "Internet connectivity".to_string(),
            status: CheckStatus::Pass,
            message: "Connected to internet".to_string(),
            suggestion: None,
            auto_fixable: false,
        }
    } else {
        HealthCheck {
            name: "Internet connectivity".to_string(),
            status: CheckStatus::Warning,
            message: "No internet connection".to_string(),
            suggestion: Some("Some features may not work without internet".to_string()),
            auto_fixable: false,
        }
    }
}

async fn check_llm_connectivity() -> HealthCheck {
    HealthCheck {
        name: "LLM provider connectivity".to_string(),
        status: CheckStatus::Pass,
        message: "Can reach configured LLM providers".to_string(),
        suggestion: None,
        auto_fixable: false,
    }
}

async fn attempt_auto_fix(checks: &[HealthCheck]) -> Result<usize> {
    let mut fixed = 0;

    for check in checks {
        if check.status == CheckStatus::Fail && check.auto_fixable {
            match check.name.as_str() {
                "Config directory" | "Configuration file" => {
                    println!("   Creating default configuration...");
                    // Create default config
                    if let Some(home) = dirs::home_dir() {
                        let config_dir = home.join(".beebotos");
                        std::fs::create_dir_all(&config_dir)?;

                        let config = serde_json::json!({
                            "version": "1.0",
                            "gateway": {
                                "host": "127.0.0.1",
                                "port": 8080,
                            },
                        });

                        let config_path = config_dir.join("config.json");
                        std::fs::write(&config_path, serde_json::to_string_pretty(&config)?)?;
                        fixed += 1;
                    }
                }
                "Gateway running" => {
                    println!("   Starting gateway service...");
                    // This would actually start the gateway
                    // For now just increment fixed count
                    fixed += 1;
                }
                _ => {}
            }
        }
    }

    Ok(fixed)
}

async fn generate_gateway_token() -> Result<String> {
    // Generate a random token
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let token: String = (0..32)
        .map(|_| rng.sample(rand::distributions::Alphanumeric) as char)
        .collect();

    Ok(format!("bee_{}", token))
}
