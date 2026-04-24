//! Gateway management commands
//!
//! Manage BeeBotOS gateway service lifecycle, health, and configuration.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand, ValueEnum};

use crate::progress::TaskProgress;

#[derive(Parser)]
pub struct GatewayArgs {
    #[command(subcommand)]
    pub command: GatewayCommand,
}

#[derive(Subcommand)]
pub enum GatewayCommand {
    /// Run gateway in foreground
    Run {
        /// Bind address
        #[arg(short, long, default_value = "127.0.0.1:8080")]
        bind: String,
        /// Run in kernel mode
        #[arg(long)]
        kernel_mode: bool,
        /// Debug mode
        #[arg(long)]
        debug: bool,
        /// Configuration file
        #[arg(short, long)]
        config: Option<PathBuf>,
    },

    /// Install gateway as system service
    Install {
        /// Service type
        #[arg(value_enum, default_value = "systemd")]
        service_type: ServiceType,
        /// Auto-start on boot
        #[arg(long)]
        auto_start: bool,
    },

    /// Uninstall gateway service
    Uninstall {
        /// Keep configuration data
        #[arg(long)]
        keep_data: bool,
    },

    /// Start gateway service
    Start {
        /// Wait for service to be ready
        #[arg(long)]
        wait: bool,
        /// Timeout in seconds
        #[arg(short, long, default_value = "30")]
        timeout: u64,
    },

    /// Stop gateway service
    Stop {
        /// Force stop
        #[arg(long)]
        force: bool,
    },

    /// Restart gateway service
    Restart {
        /// Graceful restart
        #[arg(long)]
        graceful: bool,
    },

    /// Show gateway status
    Status {
        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: OutputFormat,
        /// Watch mode - refresh every N seconds
        #[arg(short, long)]
        watch: Option<u64>,
    },

    /// Health check
    Health {
        /// Verbose output
        #[arg(short, long)]
        verbose: bool,
        /// Check blockchain connection
        #[arg(long)]
        check_chain: bool,
    },

    /// Probe gateway endpoint
    Probe {
        /// Gateway endpoint
        #[arg(default_value = "http://localhost:8080")]
        endpoint: String,
        /// Timeout in seconds
        #[arg(short, long, default_value = "5")]
        timeout: u64,
    },

    /// Discover gateways
    Discover {
        /// Discovery method
        #[arg(value_enum, default_value = "local")]
        method: DiscoverMethod,
        /// Timeout in seconds
        #[arg(short, long, default_value = "10")]
        timeout: u64,
    },

    /// RPC call to gateway
    Call {
        /// Method name
        method: String,
        /// Parameters (JSON format)
        #[arg(short, long)]
        params: Option<String>,
        /// Target gateway
        #[arg(short, long)]
        gateway: Option<String>,
    },

    /// Show usage and cost summary
    UsageCost {
        /// Time range
        #[arg(short, long, value_enum, default_value = "24h")]
        range: TimeRange,
        /// Group by agent
        #[arg(long)]
        by_agent: bool,
    },

    /// View gateway logs
    Logs {
        /// Follow log output
        #[arg(long)]
        follow: bool,
        /// Number of lines
        #[arg(short, long, default_value = "100")]
        lines: usize,
        /// Filter by level
        #[arg(long)]
        level: Option<String>,
    },

    /// Upgrade gateway
    Upgrade {
        /// Target version
        #[arg(short = 'V', long)]
        version: Option<String>,
        /// Hot upgrade
        #[arg(long)]
        hot: bool,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum ServiceType {
    Systemd,
    Launchd,
    Windows,
    Docker,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
    Yaml,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum DiscoverMethod {
    Local,
    Network,
    Dht,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum TimeRange {
    #[value(name = "1h")]
    OneHour,
    #[value(name = "24h")]
    TwentyFourHours,
    #[value(name = "7d")]
    SevenDays,
    #[value(name = "30d")]
    ThirtyDays,
}

pub async fn execute(args: GatewayArgs) -> Result<()> {
    let client = crate::client::ApiClient::new()?;

    match args.command {
        GatewayCommand::Run {
            bind,
            kernel_mode,
            debug,
            config,
        } => {
            let progress = TaskProgress::new("Starting gateway in foreground");

            println!("🚀 Starting BeeBotOS Gateway...");
            println!("   Bind address: {}", bind);
            if kernel_mode {
                println!("   Mode: Kernel (TEE enabled)");
            } else {
                println!("   Mode: Standard");
            }
            if debug {
                println!("   Debug: enabled");
            }
            if let Some(ref cfg) = config {
                println!("   Config: {}", cfg.display());
            }

            // Try to start gateway process
            match client
                .run_gateway(&bind, kernel_mode, debug, config.as_ref())
                .await
            {
                Ok(_) => {
                    progress.finish_success(Some("Gateway stopped"));
                    Ok(())
                }
                Err(e) => {
                    progress.finish_error(&format!("Failed to start gateway: {}", e));
                    Err(e)
                }
            }
        }

        GatewayCommand::Install {
            service_type,
            auto_start,
        } => {
            let progress = TaskProgress::new("Installing gateway service");

            let service_name = match service_type {
                ServiceType::Systemd => "systemd",
                ServiceType::Launchd => "LaunchAgent",
                ServiceType::Windows => "Windows Service",
                ServiceType::Docker => "Docker container",
            };

            println!("📦 Installing gateway as {}...", service_name);

            client.install_gateway(service_name, auto_start).await?;
            progress.finish_success(Some(&format!("Gateway installed as {}", service_name)));

            if auto_start {
                println!("✅ Auto-start enabled");
            }

            Ok(())
        }

        GatewayCommand::Uninstall { keep_data } => {
            let progress = TaskProgress::new("Uninstalling gateway service");

            println!("🗑️  Uninstalling gateway service...");

            client.uninstall_gateway(keep_data).await?;

            if keep_data {
                println!("📁 Configuration data preserved");
            }

            progress.finish_success(Some("Gateway uninstalled"));
            Ok(())
        }

        GatewayCommand::Start { wait, timeout } => {
            let progress = TaskProgress::new("Starting gateway service");

            println!("▶️  Starting gateway service...");

            client.start_gateway_service().await?;

            if wait {
                println!(
                    "⏳ Waiting for gateway to be ready (timeout: {}s)...",
                    timeout
                );
                let start = std::time::Instant::now();
                let timeout_duration = Duration::from_secs(timeout);

                loop {
                    match client.probe_gateway("http://localhost:8080", 2).await {
                        Ok(true) => {
                            progress.finish_success(Some("Gateway is ready"));
                            println!("✅ Gateway is running and accepting connections");
                            break;
                        }
                        Ok(false) => {
                            if start.elapsed() > timeout_duration {
                                progress.finish_error("Timeout waiting for gateway");
                                return Err(anyhow!(
                                    "Gateway failed to start within {} seconds",
                                    timeout
                                ));
                            }
                            tokio::time::sleep(Duration::from_millis(500)).await;
                        }
                        Err(_) => {
                            if start.elapsed() > timeout_duration {
                                progress.finish_error("Timeout waiting for gateway");
                                return Err(anyhow!(
                                    "Gateway failed to start within {} seconds",
                                    timeout
                                ));
                            }
                            tokio::time::sleep(Duration::from_millis(500)).await;
                        }
                    }
                }
            } else {
                progress.finish_success(Some("Gateway service started"));
            }

            Ok(())
        }

        GatewayCommand::Stop { force } => {
            let progress = TaskProgress::new("Stopping gateway service");

            if force {
                println!("⏹️  Force stopping gateway service...");
            } else {
                println!("⏹️  Stopping gateway service gracefully...");
            }

            client.stop_gateway_service(force).await?;
            progress.finish_success(Some("Gateway service stopped"));
            Ok(())
        }

        GatewayCommand::Restart { graceful } => {
            let progress = TaskProgress::new("Restarting gateway service");

            if graceful {
                println!("🔄 Gracefully restarting gateway...");
            } else {
                println!("🔄 Restarting gateway...");
            }

            client.restart_gateway_service(graceful).await?;
            progress.finish_success(Some("Gateway service restarted"));
            Ok(())
        }

        GatewayCommand::Status { format, watch } => {
            if let Some(interval) = watch {
                println!("👁️  Watching gateway status (press Ctrl+C to stop)...\n");
                loop {
                    print!("\x1B[2J\x1B[1;1H"); // Clear screen
                    let status = client.get_gateway_status().await?;
                    display_status(&status, format);
                    tokio::time::sleep(Duration::from_secs(interval)).await;
                }
            } else {
                let status = client.get_gateway_status().await?;
                display_status(&status, format);
            }
            Ok(())
        }

        GatewayCommand::Health {
            verbose,
            check_chain,
        } => {
            let progress = TaskProgress::new("Checking gateway health");

            let health = client.check_gateway_health(check_chain).await?;

            progress.finish_success(None);

            println!("🏥 Gateway Health Report");
            println!("{}", "=".repeat(50));
            println!(
                "Status: {}",
                if health.healthy {
                    "✅ Healthy"
                } else {
                    "❌ Unhealthy"
                }
            );
            println!("Uptime: {}", health.uptime);
            println!("Version: {}", health.version);

            if verbose {
                println!("\n📊 Components:");
                for (name, status) in &health.components {
                    let icon = if *status { "✅" } else { "❌" };
                    println!("  {} {}", icon, name);
                }

                println!("\n📈 Metrics:");
                println!("  Active Agents: {}", health.metrics.active_agents);
                println!("  Active Sessions: {}", health.metrics.active_sessions);
                println!("  Memory Usage: {} MB", health.metrics.memory_mb);
                println!("  CPU Usage: {}%", health.metrics.cpu_percent);

                if check_chain {
                    println!("\n⛓️  Blockchain:");
                    println!(
                        "  Connected: {}",
                        if health.chain.connected { "✅" } else { "❌" }
                    );
                    println!("  Chain ID: {}", health.chain.chain_id);
                    println!("  Block Height: {}", health.chain.block_height);
                }
            }

            if !health.healthy {
                std::process::exit(1);
            }

            Ok(())
        }

        GatewayCommand::Probe { endpoint, timeout } => {
            let progress = TaskProgress::new(format!("Probing {}", endpoint));

            match client.probe_gateway(&endpoint, timeout).await {
                Ok(true) => {
                    progress.finish_success(Some("Gateway is reachable"));
                    println!("✅ Gateway at {} is responding", endpoint);

                    // Try to get version info
                    if let Ok(version) = client.get_gateway_version(&endpoint).await {
                        println!("   Version: {}", version.version);
                        println!("   API Version: {}", version.api_version);
                    }

                    Ok(())
                }
                Ok(false) => {
                    progress.finish_error("Gateway is not responding");
                    println!("❌ Gateway at {} is not responding", endpoint);
                    std::process::exit(1);
                }
                Err(e) => {
                    progress.finish_error(&format!("Probe failed: {}", e));
                    Err(e)
                }
            }
        }

        GatewayCommand::Discover { method, timeout } => {
            let progress = TaskProgress::new("Discovering gateways");

            let method_str = match method {
                DiscoverMethod::Local => "local network",
                DiscoverMethod::Network => "multicast",
                DiscoverMethod::Dht => "DHT",
            };

            println!("🔍 Discovering gateways via {}...", method_str);

            let gateways = client.discover_gateways(method_str, timeout).await?;

            progress.finish_success(Some(&format!("{} gateways found", gateways.len())));

            if gateways.is_empty() {
                println!("⚠️  No gateways found");
            } else {
                println!("\n📋 Discovered Gateways:");
                println!("{:<20} {:<25} {:<15} Status", "Name", "Endpoint", "Version");
                println!("{}", "-".repeat(90));
                for gw in gateways {
                    let status = if gw.available { "🟢" } else { "🔴" };
                    println!(
                        "{:<20} {:<25} {:<15} {}",
                        gw.name, gw.endpoint, gw.version, status
                    );
                }
            }

            Ok(())
        }

        GatewayCommand::Call {
            method,
            params,
            gateway,
        } => {
            let endpoint = gateway.as_deref().unwrap_or("http://localhost:8080");

            println!("📞 Calling {} on {}...", method, endpoint);

            let result = client
                .call_gateway_rpc(endpoint, &method, params.as_deref())
                .await?;

            println!("✅ RPC call successful");
            println!("\nResult:");
            println!("{}", serde_json::to_string_pretty(&result)?);

            Ok(())
        }

        GatewayCommand::UsageCost { range, by_agent } => {
            let progress = TaskProgress::new("Fetching usage statistics");

            let range_str = match range {
                TimeRange::OneHour => "1h",
                TimeRange::TwentyFourHours => "24h",
                TimeRange::SevenDays => "7d",
                TimeRange::ThirtyDays => "30d",
            };

            let stats = client.get_gateway_usage(range_str, by_agent).await?;

            progress.finish_success(None);

            println!("📊 Gateway Usage & Cost (Last {})", range_str);
            println!("{}", "=".repeat(60));

            if by_agent {
                println!("\nBreakdown by Agent:");
                for agent in stats.agents {
                    println!("\n  🤖 {}", agent.name);
                    println!("     Requests: {}", agent.requests);
                    println!(
                        "     Tokens: {} input / {} output",
                        agent.input_tokens, agent.output_tokens
                    );
                    println!("     Cost: ${:.4}", agent.cost);
                }
            }

            println!("\n📈 Total Usage:");
            println!("   Total Requests: {}", stats.total_requests);
            println!(
                "   Total Tokens: {} input / {} output",
                stats.total_input_tokens, stats.total_output_tokens
            );
            println!("   Estimated Cost: ${:.4}", stats.total_cost);

            if stats.chain_gas_used > 0 {
                println!("\n⛓️  Blockchain:");
                println!("   Gas Used: {}", stats.chain_gas_used);
                println!("   Gas Cost: ${:.4}", stats.chain_gas_cost);
            }

            Ok(())
        }

        GatewayCommand::Logs {
            follow,
            lines,
            level,
        } => {
            if follow {
                println!("👁️  Following gateway logs (press Ctrl+C to stop)...\n");
                client.follow_gateway_logs(level.as_deref()).await?;
            } else {
                let logs = client.get_gateway_logs(lines, level.as_deref()).await?;

                println!("📜 Gateway Logs (last {} lines):", lines);
                println!("{}", "=".repeat(60));

                for entry in logs {
                    let level_icon = match entry.level.as_str() {
                        "ERROR" => "🔴",
                        "WARN" => "🟡",
                        "INFO" => "🔵",
                        "DEBUG" => "⚪",
                        _ => "⚫",
                    };
                    println!(
                        "{} [{}] {} - {}",
                        level_icon, entry.timestamp, entry.level, entry.message
                    );
                }
            }

            Ok(())
        }

        GatewayCommand::Upgrade { version, hot } => {
            let progress = TaskProgress::new("Upgrading gateway");

            if hot {
                println!("🔥 Performing hot upgrade...");
            } else {
                println!("⬆️  Upgrading gateway...");
            }

            if let Some(ref v) = version {
                println!("   Target version: {}", v);
            } else {
                println!("   Target version: latest");
            }

            let result = client.upgrade_gateway(version.as_deref(), hot).await?;

            progress.finish_success(Some(&format!("Upgraded to {}", result.new_version)));

            if hot {
                println!("✅ Hot upgrade completed - no downtime!");
            } else {
                println!("⚠️  Please restart the gateway service to apply the update");
            }

            Ok(())
        }
    }
}

fn display_status(status: &GatewayStatus, format: OutputFormat) {
    match format {
        OutputFormat::Table => {
            println!("🌐 Gateway Status");
            println!("{}", "=".repeat(60));
            println!(
                "Status:          {}",
                if status.running {
                    "🟢 Running"
                } else {
                    "🔴 Stopped"
                }
            );
            println!("Version:         {}", status.version);
            println!("Uptime:          {}", status.uptime);
            println!("Endpoint:        {}", status.endpoint);
            println!("WebSocket:       {}", status.websocket_endpoint);

            if status.running {
                println!("\n📊 Current Load:");
                println!("  Active Agents:    {}", status.load.active_agents);
                println!("  Active Sessions:  {}", status.load.active_sessions);
                println!("  Pending Tasks:    {}", status.load.pending_tasks);
                println!("  Connected Chains: {}", status.load.connected_chains);

                println!("\n💻 System Resources:");
                println!(
                    "  Memory: {} MB / {} MB",
                    status.resources.memory_used_mb, status.resources.memory_total_mb
                );
                println!("  CPU:    {:.1}%", status.resources.cpu_percent);
                println!("  Disk:   {} MB free", status.resources.disk_free_mb);
            }
        }
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(status).unwrap());
        }
        OutputFormat::Yaml => {
            println!("{}", serde_yaml::to_string(status).unwrap());
        }
    }
}

// Response types
#[derive(serde::Deserialize, serde::Serialize)]
struct GatewayStatus {
    running: bool,
    version: String,
    uptime: String,
    endpoint: String,
    websocket_endpoint: String,
    load: GatewayLoad,
    resources: GatewayResources,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct GatewayLoad {
    active_agents: usize,
    active_sessions: usize,
    pending_tasks: usize,
    connected_chains: usize,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct GatewayResources {
    memory_used_mb: u64,
    memory_total_mb: u64,
    cpu_percent: f64,
    disk_free_mb: u64,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct GatewayHealth {
    healthy: bool,
    uptime: String,
    version: String,
    components: std::collections::HashMap<String, bool>,
    metrics: HealthMetrics,
    chain: ChainHealth,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct HealthMetrics {
    active_agents: usize,
    active_sessions: usize,
    memory_mb: u64,
    cpu_percent: f64,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct ChainHealth {
    connected: bool,
    chain_id: u64,
    block_height: u64,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct GatewayVersion {
    version: String,
    api_version: String,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct DiscoveredGateway {
    name: String,
    endpoint: String,
    version: String,
    available: bool,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct UsageStats {
    total_requests: u64,
    total_input_tokens: u64,
    total_output_tokens: u64,
    total_cost: f64,
    chain_gas_used: u64,
    chain_gas_cost: f64,
    agents: Vec<AgentUsage>,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct AgentUsage {
    name: String,
    requests: u64,
    input_tokens: u64,
    output_tokens: u64,
    cost: f64,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct LogEntry {
    timestamp: String,
    level: String,
    message: String,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct UpgradeResult {
    new_version: String,
}

// Client extension trait
trait GatewayClient {
    async fn run_gateway(
        &self,
        bind: &str,
        kernel_mode: bool,
        debug: bool,
        config: Option<&PathBuf>,
    ) -> Result<()>;
    async fn install_gateway(&self, service_type: &str, auto_start: bool) -> Result<()>;
    async fn uninstall_gateway(&self, keep_data: bool) -> Result<()>;
    async fn start_gateway_service(&self) -> Result<()>;
    async fn stop_gateway_service(&self, force: bool) -> Result<()>;
    async fn restart_gateway_service(&self, graceful: bool) -> Result<()>;
    async fn get_gateway_status(&self) -> Result<GatewayStatus>;
    async fn check_gateway_health(&self, check_chain: bool) -> Result<GatewayHealth>;
    async fn probe_gateway(&self, endpoint: &str, timeout: u64) -> Result<bool>;
    async fn get_gateway_version(&self, endpoint: &str) -> Result<GatewayVersion>;
    async fn discover_gateways(&self, method: &str, timeout: u64)
        -> Result<Vec<DiscoveredGateway>>;
    async fn call_gateway_rpc(
        &self,
        endpoint: &str,
        method: &str,
        params: Option<&str>,
    ) -> Result<serde_json::Value>;
    async fn get_gateway_usage(&self, range: &str, by_agent: bool) -> Result<UsageStats>;
    async fn get_gateway_logs(&self, lines: usize, level: Option<&str>) -> Result<Vec<LogEntry>>;
    async fn follow_gateway_logs(&self, level: Option<&str>) -> Result<()>;
    async fn upgrade_gateway(&self, version: Option<&str>, hot: bool) -> Result<UpgradeResult>;
}

impl GatewayClient for crate::client::ApiClient {
    async fn run_gateway(
        &self,
        _bind: &str,
        _kernel_mode: bool,
        _debug: bool,
        _config: Option<&PathBuf>,
    ) -> Result<()> {
        // This would actually spawn the gateway process
        // For now, return not implemented
        anyhow::bail!(
            "Running gateway in foreground is not yet implemented. Use 'gateway start' instead."
        )
    }

    async fn install_gateway(&self, service_type: &str, auto_start: bool) -> Result<()> {
        let url = format!(
            "{}/gateway/install",
            self.build_url("").trim_end_matches('/')
        );
        let body = serde_json::json!({
            "service_type": service_type,
            "auto_start": auto_start,
        });

        let resp = self
            .http()
            .post(&url)
            .headers(self.headers()?)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to install gateway ({}): {}", status, text));
        }

        Ok(())
    }

    async fn uninstall_gateway(&self, keep_data: bool) -> Result<()> {
        let url = format!(
            "{}/gateway/uninstall?keep_data={}",
            self.build_url("").trim_end_matches('/'),
            keep_data
        );
        let resp = self
            .http()
            .post(&url)
            .headers(self.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!(
                "Failed to uninstall gateway ({}): {}",
                status,
                text
            ));
        }

        Ok(())
    }

    async fn start_gateway_service(&self) -> Result<()> {
        let url = format!("{}/gateway/start", self.build_url("").trim_end_matches('/'));
        let resp = self
            .http()
            .post(&url)
            .headers(self.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to start gateway ({}): {}", status, text));
        }

        Ok(())
    }

    async fn stop_gateway_service(&self, force: bool) -> Result<()> {
        let url = format!(
            "{}/gateway/stop?force={}",
            self.build_url("").trim_end_matches('/'),
            force
        );
        let resp = self
            .http()
            .post(&url)
            .headers(self.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to stop gateway ({}): {}", status, text));
        }

        Ok(())
    }

    async fn restart_gateway_service(&self, graceful: bool) -> Result<()> {
        let url = format!(
            "{}/gateway/restart?graceful={}",
            self.build_url("").trim_end_matches('/'),
            graceful
        );
        let resp = self
            .http()
            .post(&url)
            .headers(self.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to restart gateway ({}): {}", status, text));
        }

        Ok(())
    }

    async fn get_gateway_status(&self) -> Result<GatewayStatus> {
        let url = format!(
            "{}/gateway/status",
            self.build_url("").trim_end_matches('/')
        );
        let resp = self
            .http()
            .get(&url)
            .headers(self.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!(
                "Failed to get gateway status ({}): {}",
                status,
                text
            ));
        }

        Ok(resp.json().await?)
    }

    async fn check_gateway_health(&self, check_chain: bool) -> Result<GatewayHealth> {
        let url = format!(
            "{}/gateway/health?check_chain={}",
            self.build_url("").trim_end_matches('/'),
            check_chain
        );
        let resp = self
            .http()
            .get(&url)
            .headers(self.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Health check failed ({}): {}", status, text));
        }

        Ok(resp.json().await?)
    }

    async fn probe_gateway(&self, endpoint: &str, timeout: u64) -> Result<bool> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout))
            .build()?;

        let url = format!("{}/health", endpoint.trim_end_matches('/'));

        match client.get(&url).send().await {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    async fn get_gateway_version(&self, endpoint: &str) -> Result<GatewayVersion> {
        let url = format!("{}/version", endpoint.trim_end_matches('/'));
        let resp = self.http().get(&url).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to get version ({}): {}", status, text));
        }

        Ok(resp.json().await?)
    }

    async fn discover_gateways(
        &self,
        method: &str,
        timeout: u64,
    ) -> Result<Vec<DiscoveredGateway>> {
        let url = format!(
            "{}/gateway/discover?method={}&timeout={}",
            self.build_url("").trim_end_matches('/'),
            method,
            timeout
        );
        let resp = self
            .http()
            .get(&url)
            .headers(self.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Discovery failed ({}): {}", status, text));
        }

        let result: serde_json::Value = resp.json().await?;
        Ok(serde_json::from_value(result["gateways"].clone())?)
    }

    async fn call_gateway_rpc(
        &self,
        endpoint: &str,
        method: &str,
        params: Option<&str>,
    ) -> Result<serde_json::Value> {
        let url = format!("{}/rpc", endpoint.trim_end_matches('/'));

        let params_json: Option<serde_json::Value> = match params {
            Some(p) => Some(serde_json::from_str(p)?),
            None => None,
        };

        let body = serde_json::json!({
            "method": method,
            "params": params_json,
        });

        let resp = self
            .http()
            .post(&url)
            .headers(self.headers()?)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("RPC call failed ({}): {}", status, text));
        }

        Ok(resp.json().await?)
    }

    async fn get_gateway_usage(&self, range: &str, by_agent: bool) -> Result<UsageStats> {
        let url = format!(
            "{}/gateway/usage?range={}&by_agent={}",
            self.build_url("").trim_end_matches('/'),
            range,
            by_agent
        );
        let resp = self
            .http()
            .get(&url)
            .headers(self.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to get usage stats ({}): {}", status, text));
        }

        Ok(resp.json().await?)
    }

    async fn get_gateway_logs(&self, lines: usize, level: Option<&str>) -> Result<Vec<LogEntry>> {
        let mut url = format!(
            "{}/gateway/logs?lines={}",
            self.build_url("").trim_end_matches('/'),
            lines
        );
        if let Some(l) = level {
            url.push_str(&format!("&level={}", l));
        }

        let resp = self
            .http()
            .get(&url)
            .headers(self.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to get logs ({}): {}", status, text));
        }

        let result: serde_json::Value = resp.json().await?;
        Ok(serde_json::from_value(result["logs"].clone())?)
    }

    async fn follow_gateway_logs(&self, level: Option<&str>) -> Result<()> {
        let mut url = format!(
            "{}/gateway/logs/stream",
            self.build_url("").trim_end_matches('/')
        );
        if let Some(l) = level {
            url.push_str(&format!("?level={}", l));
        }

        let mut resp = self
            .http()
            .get(&url)
            .headers(self.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to follow logs ({}): {}", status, text));
        }

        // Stream logs to stdout
        while let Some(chunk) = resp.chunk().await? {
            let text = String::from_utf8_lossy(&chunk);
            print!("{}", text);
        }

        Ok(())
    }

    async fn upgrade_gateway(&self, version: Option<&str>, hot: bool) -> Result<UpgradeResult> {
        let url = format!(
            "{}/gateway/upgrade",
            self.build_url("").trim_end_matches('/')
        );
        let body = serde_json::json!({
            "version": version,
            "hot": hot,
        });

        let resp = self
            .http()
            .post(&url)
            .headers(self.headers()?)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Upgrade failed ({}): {}", status, text));
        }

        Ok(resp.json().await?)
    }
}
