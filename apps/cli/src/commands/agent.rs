use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::log_info;
use crate::progress::TaskProgress;

#[derive(Parser)]
pub struct AgentArgs {
    #[command(subcommand)]
    pub command: AgentCommand,
}

#[derive(Subcommand)]
pub enum AgentCommand {
    /// Create a new agent
    Create {
        /// Agent name
        #[arg(short, long)]
        name: String,

        /// Agent template
        #[arg(short, long, default_value = "default")]
        template: String,

        /// Configuration file
        #[arg(short, long)]
        config: Option<String>,

        /// Start after creation
        #[arg(long)]
        start: bool,

        /// Register on chain
        #[arg(long)]
        register: bool,
    },

    /// List agents
    List {
        /// Filter by status
        #[arg(short, long)]
        status: Option<String>,

        /// Show all agents including stopped
        #[arg(long)]
        all: bool,

        /// Filter by capability
        #[arg(long)]
        capability: Vec<String>,
    },

    /// Start an agent
    Start {
        /// Agent ID
        id: String,

        /// Run in background
        #[arg(short, long)]
        detach: bool,

        /// Resource limits (format: cpu=1.0,memory=1Gi)
        #[arg(long)]
        limit: Vec<String>,
    },

    /// Stop an agent
    Stop {
        /// Agent ID
        id: String,

        /// Force stop
        #[arg(long)]
        force: bool,

        /// Graceful timeout in seconds
        #[arg(long, default_value = "30")]
        graceful_timeout: u64,
    },

    /// Pause an agent (preserve state)
    Pause {
        /// Agent ID
        id: String,
    },

    /// Resume a paused agent
    Resume {
        /// Agent ID
        id: String,
    },

    /// Show agent logs
    Logs {
        /// Agent ID
        id: String,

        /// Follow logs
        #[arg(long)]
        follow: bool,

        /// Number of lines to show
        #[arg(short, long, default_value = "100")]
        lines: usize,
    },

    /// Delete an agent
    Delete {
        /// Agent ID
        id: String,

        /// Force delete without confirmation
        #[arg(long)]
        force: bool,

        /// Clean up chain state
        #[arg(long)]
        cleanup_chain: bool,
    },

    /// Clone an agent
    Clone {
        /// Source agent ID
        source: String,

        /// New agent name
        #[arg(short, long)]
        name: Option<String>,

        /// Include memory in clone
        #[arg(long)]
        with_memory: bool,
    },

    /// Export agent configuration
    Export {
        /// Agent ID
        id: String,

        /// Output path
        #[arg(short, long, default_value = "-")]
        output: String,

        /// Include memory
        #[arg(long)]
        include_memory: bool,

        /// Include keys (encrypted)
        #[arg(long)]
        include_keys: bool,
    },

    /// Import agent configuration
    Import {
        /// Configuration file path
        path: PathBuf,

        /// New name for imported agent
        #[arg(short, long)]
        name: Option<String>,
    },

    /// Manage agent channel bindings
    #[command(subcommand)]
    Channel(AgentChannelCommand),

    /// Set agent identity
    SetIdentity {
        /// Agent ID
        id: String,

        /// DID (Decentralized Identifier)
        #[arg(short, long)]
        did: Option<String>,

        /// Personality configuration file
        #[arg(short, long)]
        personality: Option<PathBuf>,
    },

    /// Execute a task on an agent
    Exec {
        /// Agent ID
        id: String,

        /// Task input
        #[arg(short, long)]
        input: String,

        /// Timeout in seconds
        #[arg(short, long, default_value = "60")]
        timeout: u64,

        /// WASM sandbox level
        #[arg(long, default_value = "standard")]
        sandbox: String,
    },

    /// Run agent for single task (shortcut)
    Run {
        /// Agent ID or name
        #[arg(short, long)]
        agent: Option<String>,

        /// Message/Instruction
        #[arg(short, long)]
        message: String,

        /// Deliver result to channel
        #[arg(long)]
        deliver: bool,

        /// Target channel for delivery
        #[arg(short, long)]
        channel: Option<String>,

        /// Timeout in seconds
        #[arg(short, long, default_value = "60")]
        timeout: u64,

        /// WASM sandbox level
        #[arg(long, default_value = "standard")]
        sandbox: String,
    },
}

#[derive(Subcommand)]
pub enum AgentChannelCommand {
    /// List agent's channel bindings
    List {
        /// Agent ID
        id: String,
    },

    /// Bind agent to a channel
    Bind {
        /// Agent ID
        id: String,

        /// Channel ID to bind
        channel: String,

        /// Binding configuration (JSON)
        #[arg(short, long)]
        config: Option<String>,
    },

    /// Unbind agent from a channel
    Unbind {
        /// Agent ID
        id: String,

        /// Channel ID to unbind
        channel: String,
    },
}

pub async fn execute(args: AgentArgs) -> Result<()> {
    let client = crate::client::ApiClient::new()?;

    match args.command {
        AgentCommand::Create {
            name,
            template,
            config,
            start,
            register,
        } => {
            log_info!("Creating agent '{}' using template '{}'", name, template);
            let progress = TaskProgress::new("Creating agent");
            let agent = client
                .create_agent(&name, &template, config.as_deref())
                .await?;
            progress.finish_success(Some(&format!("ID: {}", agent.id)));
            println!("Agent ID: {}", agent.id);
            println!("Status: {}", agent.status);

            if register {
                println!("🔗 Registering on chain...");
                client.register_agent_on_chain(&agent.id).await?;
                println!("✅ Registered on chain");
            }

            if start {
                println!("▶️  Starting agent...");
                client.start_agent(&agent.id).await?;
                println!("✅ Agent started");
            }
        }

        AgentCommand::List {
            status,
            all,
            capability,
        } => {
            let progress = TaskProgress::new("Listing agents");
            let agents = if capability.is_empty() {
                client.list_agents(status.as_deref(), all).await?
            } else {
                client.list_agents_by_capability(&capability).await?
            };
            progress.finish_success(Some(&format!("{} agents found", agents.len())));
            println!(
                "{:<20} {:<30} {:<15} {:<20}",
                "ID", "Name", "Status", "Last Active"
            );
            println!("{}", "-".repeat(85));
            for agent in agents {
                println!(
                    "{:<20} {:<30} {:<15} {:<20}",
                    agent.id, agent.name, agent.status, agent.last_active
                );
            }
        }

        AgentCommand::Start { id, detach, limit } => {
            let progress = TaskProgress::new(format!("Starting agent {}", id));
            if !limit.is_empty() {
                client.start_agent_with_limits(&id, &limit).await?;
            } else {
                client.start_agent(&id).await?;
            }
            if detach {
                progress.finish_success(Some("running in background"));
            } else {
                progress.finish_success(None);
            }
        }

        AgentCommand::Stop {
            id,
            force,
            graceful_timeout,
        } => {
            let progress = TaskProgress::new(format!("Stopping agent {}", id));
            client
                .stop_agent_graceful(&id, force, graceful_timeout)
                .await?;
            progress.finish_success(None);
        }

        AgentCommand::Pause { id } => {
            let progress = TaskProgress::new(format!("Pausing agent {}", id));
            client.pause_agent(&id).await?;
            progress.finish_success(Some("state preserved"));
            println!(
                "⏸️  Agent paused. Use `beebot agent resume {}` to resume.",
                id
            );
        }

        AgentCommand::Resume { id } => {
            let progress = TaskProgress::new(format!("Resuming agent {}", id));
            client.resume_agent(&id).await?;
            progress.finish_success(Some("state restored"));
            println!("▶️  Agent resumed.");
        }

        AgentCommand::Logs { id, follow, lines } => {
            if follow {
                println!("Following logs for agent '{}' (Ctrl+C to exit)...", id);
                client.follow_logs(&id).await?;
            } else {
                let progress = TaskProgress::new("Fetching logs");
                let logs = client.get_logs(&id, lines).await?;
                progress.finish_success(Some(&format!("{} lines", logs.len())));
                for line in logs {
                    println!("{}", line);
                }
            }
        }

        AgentCommand::Delete {
            id,
            force,
            cleanup_chain,
        } => {
            if !force {
                print!("Are you sure you want to delete agent '{}'? [y/N] ", id);
                std::io::Write::flush(&mut std::io::stdout())?;
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                if !input.trim().eq_ignore_ascii_case("y") {
                    println!("Cancelled.");
                    return Ok(());
                }
            }
            let progress = TaskProgress::new(format!("Deleting agent {}", id));
            client.delete_agent(&id).await?;

            if cleanup_chain {
                println!("🧹 Cleaning up chain state...");
                client.cleanup_agent_chain_state(&id).await?;
            }

            progress.finish_success(None);
        }

        AgentCommand::Clone {
            source,
            name,
            with_memory,
        } => {
            let progress = TaskProgress::new(format!("Cloning agent {}", source));
            let new_agent = client
                .clone_agent(&source, name.as_deref(), with_memory)
                .await?;
            progress.finish_success(Some(&format!("New ID: {}", new_agent.id)));
            println!("✅ Agent cloned successfully!");
            println!("   New ID: {}", new_agent.id);
            println!("   Name: {}", new_agent.name);
            if with_memory {
                println!("   Memory: included");
            }
        }

        AgentCommand::Export {
            id,
            output,
            include_memory,
            include_keys,
        } => {
            let progress = TaskProgress::new(format!("Exporting agent {}", id));
            let export_data = client
                .export_agent(&id, include_memory, include_keys)
                .await?;
            progress.finish_success(None);

            if output == "-" {
                println!("{}", serde_json::to_string_pretty(&export_data)?);
            } else {
                std::fs::write(&output, serde_json::to_string_pretty(&export_data)?)?;
                println!("✅ Exported to {}", output);
            }
        }

        AgentCommand::Import { path, name } => {
            let progress = TaskProgress::new("Importing agent");
            let config_str = std::fs::read_to_string(&path)?;
            let agent = client.import_agent(&config_str, name.as_deref()).await?;
            progress.finish_success(Some(&format!("ID: {}", agent.id)));
            println!("✅ Agent imported successfully!");
            println!("   ID: {}", agent.id);
            println!("   Name: {}", agent.name);
        }

        AgentCommand::Channel(cmd) => match cmd {
            AgentChannelCommand::List { id } => {
                let bindings = client.get_agent_channel_bindings(&id).await?;
                println!("Channel bindings for agent {}:", id);
                for binding in bindings {
                    println!(
                        "  📡 {} - {} ({})",
                        binding.channel_id,
                        binding.channel_name,
                        if binding.active { "active" } else { "inactive" }
                    );
                }
            }
            AgentChannelCommand::Bind {
                id,
                channel,
                config,
            } => {
                let progress = TaskProgress::new(format!("Binding to {}", channel));
                client
                    .bind_agent_to_channel(&id, &channel, config.as_deref())
                    .await?;
                progress.finish_success(None);
                println!("✅ Agent bound to channel {}", channel);
            }
            AgentChannelCommand::Unbind { id, channel } => {
                let progress = TaskProgress::new(format!("Unbinding from {}", channel));
                client.unbind_agent_from_channel(&id, &channel).await?;
                progress.finish_success(None);
                println!("✅ Agent unbound from channel {}", channel);
            }
        },

        AgentCommand::SetIdentity {
            id,
            did,
            personality,
        } => {
            let progress = TaskProgress::new(format!("Setting identity for {}", id));

            if let Some(did_val) = did {
                client.set_agent_did(&id, &did_val).await?;
                println!("🔗 DID set: {}", did_val);
            }

            if let Some(personality_path) = personality {
                let personality_config = std::fs::read_to_string(&personality_path)?;
                client
                    .set_agent_personality(&id, &personality_config)
                    .await?;
                println!("🎭 Personality loaded from {:?}", personality_path);
            }

            progress.finish_success(None);
            println!("✅ Identity updated for agent {}", id);
        }

        AgentCommand::Exec {
            id,
            input,
            timeout,
            sandbox,
        } => {
            let progress = TaskProgress::new(format!("Executing task on agent {}", id));
            let result = client
                .exec_task_with_sandbox(&id, &input, timeout, &sandbox)
                .await?;
            progress.finish_success(None);
            println!("Result: {}", result.output);
        }

        AgentCommand::Run {
            agent,
            message,
            deliver,
            channel,
            timeout,
            sandbox,
        } => {
            let progress = TaskProgress::new("Running agent task");

            let agent_id = match agent {
                Some(id) => id,
                None => {
                    // Try to find default agent
                    let agents = client.list_agents(Some("running"), false).await?;
                    agents.first().map(|a| a.id.clone()).ok_or_else(|| {
                        anyhow::anyhow!(
                            "No running agent found. Specify --agent or start an agent."
                        )
                    })?
                }
            };

            let result = client
                .exec_task_with_sandbox(&agent_id, &message, timeout, &sandbox)
                .await?;
            progress.finish_success(None);

            println!("🤖 Agent response:");
            println!("{}", result.output);

            if deliver {
                if let Some(ch) = channel {
                    client
                        .deliver_to_channel(&agent_id, &ch, &result.output)
                        .await?;
                    println!("📤 Delivered to channel {}", ch);
                }
            }
        }
    }

    Ok(())
}

// Client extension trait
trait AgentClient {
    async fn register_agent_on_chain(&self, agent_id: &str) -> Result<()>;
    async fn list_agents_by_capability(
        &self,
        capabilities: &[String],
    ) -> Result<Vec<crate::client::AgentInfo>>;
    async fn start_agent_with_limits(&self, agent_id: &str, limits: &[String]) -> Result<()>;
    async fn stop_agent_graceful(&self, agent_id: &str, force: bool, timeout: u64) -> Result<()>;
    async fn pause_agent(&self, agent_id: &str) -> Result<()>;
    async fn resume_agent(&self, agent_id: &str) -> Result<()>;
    async fn clone_agent(
        &self,
        source: &str,
        name: Option<&str>,
        with_memory: bool,
    ) -> Result<crate::client::AgentInfo>;
    async fn export_agent(
        &self,
        agent_id: &str,
        include_memory: bool,
        include_keys: bool,
    ) -> Result<serde_json::Value>;
    async fn import_agent(
        &self,
        config: &str,
        name: Option<&str>,
    ) -> Result<crate::client::AgentInfo>;
    async fn cleanup_agent_chain_state(&self, agent_id: &str) -> Result<()>;
    async fn get_agent_channel_bindings(&self, agent_id: &str) -> Result<Vec<ChannelBinding>>;
    async fn bind_agent_to_channel(
        &self,
        agent_id: &str,
        channel: &str,
        config: Option<&str>,
    ) -> Result<()>;
    async fn unbind_agent_from_channel(&self, agent_id: &str, channel: &str) -> Result<()>;
    async fn set_agent_did(&self, agent_id: &str, did: &str) -> Result<()>;
    async fn set_agent_personality(&self, agent_id: &str, config: &str) -> Result<()>;
    async fn exec_task_with_sandbox(
        &self,
        agent_id: &str,
        input: &str,
        timeout: u64,
        sandbox: &str,
    ) -> Result<crate::client::TaskResult>;
    async fn deliver_to_channel(&self, agent_id: &str, channel: &str, content: &str) -> Result<()>;
}

impl AgentClient for crate::client::ApiClient {
    async fn register_agent_on_chain(&self, _agent_id: &str) -> Result<()> {
        // Would register agent on blockchain
        Ok(())
    }

    async fn list_agents_by_capability(
        &self,
        _capabilities: &[String],
    ) -> Result<Vec<crate::client::AgentInfo>> {
        // Would filter by capability
        self.list_agents(None, false).await
    }

    async fn start_agent_with_limits(&self, agent_id: &str, _limits: &[String]) -> Result<()> {
        // Would apply resource limits
        self.start_agent(agent_id).await
    }

    async fn stop_agent_graceful(&self, agent_id: &str, force: bool, _timeout: u64) -> Result<()> {
        self.stop_agent(agent_id, force).await
    }

    async fn pause_agent(&self, agent_id: &str) -> Result<()> {
        let url = self.build_url(&format!("/agents/{}/pause", agent_id));
        let resp = self
            .http()
            .post(&url)
            .headers(self.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Failed to pause agent ({}): {}",
                status,
                text
            ));
        }
        Ok(())
    }

    async fn resume_agent(&self, agent_id: &str) -> Result<()> {
        let url = self.build_url(&format!("/agents/{}/resume", agent_id));
        let resp = self
            .http()
            .post(&url)
            .headers(self.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Failed to resume agent ({}): {}",
                status,
                text
            ));
        }
        Ok(())
    }

    async fn clone_agent(
        &self,
        source: &str,
        name: Option<&str>,
        with_memory: bool,
    ) -> Result<crate::client::AgentInfo> {
        let url = self.build_url("/agents/clone");
        let body = serde_json::json!({
            "source_id": source,
            "name": name,
            "with_memory": with_memory,
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
            return Err(anyhow::anyhow!(
                "Failed to clone agent ({}): {}",
                status,
                text
            ));
        }

        Ok(resp.json().await?)
    }

    async fn export_agent(
        &self,
        agent_id: &str,
        include_memory: bool,
        include_keys: bool,
    ) -> Result<serde_json::Value> {
        let url = self.build_url(&format!("/agents/{}/export", agent_id));
        let body = serde_json::json!({
            "include_memory": include_memory,
            "include_keys": include_keys,
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
            return Err(anyhow::anyhow!(
                "Failed to export agent ({}): {}",
                status,
                text
            ));
        }

        Ok(resp.json().await?)
    }

    async fn import_agent(
        &self,
        config: &str,
        name: Option<&str>,
    ) -> Result<crate::client::AgentInfo> {
        let url = self.build_url("/agents/import");
        let body = serde_json::json!({
            "config": config,
            "name": name,
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
            return Err(anyhow::anyhow!(
                "Failed to import agent ({}): {}",
                status,
                text
            ));
        }

        Ok(resp.json().await?)
    }

    async fn cleanup_agent_chain_state(&self, _agent_id: &str) -> Result<()> {
        // Would clean up on-chain state
        Ok(())
    }

    async fn get_agent_channel_bindings(&self, agent_id: &str) -> Result<Vec<ChannelBinding>> {
        let url = self.build_url(&format!("/agents/{}/channels", agent_id));
        let resp = self
            .http()
            .get(&url)
            .headers(self.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }

        let result: serde_json::Value = resp.json().await?;
        Ok(serde_json::from_value(result["bindings"].clone()).unwrap_or_default())
    }

    async fn bind_agent_to_channel(
        &self,
        agent_id: &str,
        channel: &str,
        config: Option<&str>,
    ) -> Result<()> {
        let url = self.build_url(&format!("/agents/{}/channels", agent_id));
        let body = serde_json::json!({
            "channel_id": channel,
            "config": config,
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
            return Err(anyhow::anyhow!(
                "Failed to bind channel ({}): {}",
                status,
                text
            ));
        }
        Ok(())
    }

    async fn unbind_agent_from_channel(&self, agent_id: &str, channel: &str) -> Result<()> {
        let url = self.build_url(&format!("/agents/{}/channels/{}", agent_id, channel));
        let resp = self
            .http()
            .delete(&url)
            .headers(self.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Failed to unbind channel ({}): {}",
                status,
                text
            ));
        }
        Ok(())
    }

    async fn set_agent_did(&self, agent_id: &str, did: &str) -> Result<()> {
        let url = self.build_url(&format!("/agents/{}/identity", agent_id));
        let body = serde_json::json!({
            "did": did,
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
            return Err(anyhow::anyhow!("Failed to set DID ({}): {}", status, text));
        }
        Ok(())
    }

    async fn set_agent_personality(&self, agent_id: &str, config: &str) -> Result<()> {
        let url = self.build_url(&format!("/agents/{}/personality", agent_id));
        let body = serde_json::json!({
            "config": config,
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
            return Err(anyhow::anyhow!(
                "Failed to set personality ({}): {}",
                status,
                text
            ));
        }
        Ok(())
    }

    async fn exec_task_with_sandbox(
        &self,
        agent_id: &str,
        input: &str,
        timeout: u64,
        _sandbox: &str,
    ) -> Result<crate::client::TaskResult> {
        // Would apply sandbox level
        self.exec_task(agent_id, input, timeout).await
    }

    async fn deliver_to_channel(
        &self,
        _agent_id: &str,
        _channel: &str,
        _content: &str,
    ) -> Result<()> {
        // Would deliver message to channel
        Ok(())
    }
}

#[derive(serde::Deserialize)]
struct ChannelBinding {
    channel_id: String,
    channel_name: String,
    active: bool,
}
