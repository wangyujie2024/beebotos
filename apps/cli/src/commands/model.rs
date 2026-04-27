//! Model management commands
//!
//! LLM provider configuration and failover management

use std::io::{self, Write};

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use futures::StreamExt;

use crate::progress::TaskProgress;

#[derive(Parser)]
pub struct ModelArgs {
    #[command(subcommand)]
    pub command: ModelCommand,
}

#[derive(Subcommand)]
pub enum ModelCommand {
    /// List configured models
    List {
        /// Filter by provider
        #[arg(short, long)]
        provider: Option<String>,
        /// Show detailed information
        #[arg(short, long)]
        verbose: bool,
        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: OutputFormat,
    },

    /// Check model status
    Status {
        /// Model ID (optional, checks all if not provided)
        id: Option<String>,
        /// Show provider details
        #[arg(long)]
        provider_details: bool,
    },

    /// Set default model for a task type
    Set {
        /// Model ID
        id: String,
        /// Task type
        #[arg(short = 't', long, value_enum, default_value = "default")]
        for_task: TaskType,
    },

    /// Set default image model
    SetImage {
        /// Model ID
        id: String,
    },

    /// Scan for available models
    Scan {
        /// Scan source
        #[arg(value_enum, default_value = "all")]
        source: ScanSource,
        /// Ollama endpoint
        #[arg(long, default_value = "http://localhost:11434")]
        ollama_url: String,
        /// Filter by capability
        #[arg(long)]
        capability: Vec<String>,
    },

    /// Show model details
    Info {
        /// Model ID
        id: String,
        /// Show pricing
        #[arg(long)]
        pricing: bool,
    },

    /// Compare models
    Compare {
        /// Model IDs to compare
        #[arg(required = true)]
        models: Vec<String>,
        /// Compare dimensions
        #[arg(short, long, value_enum)]
        dimension: Vec<CompareDimension>,
    },

    /// Test model with a prompt
    Test {
        /// Model ID
        #[arg(short, long)]
        model: Option<String>,
        /// Test prompt
        #[arg(short, long, default_value = "Hello, how are you?")]
        prompt: String,
        /// Max tokens
        #[arg(long, default_value = "100")]
        max_tokens: u32,
        /// Run multiple times for latency stats
        #[arg(short, long, default_value = "1")]
        runs: u32,
    },

    /// Authentication management
    #[command(subcommand)]
    Auth(ModelAuthCommand),

    /// Provider priority ordering
    #[command(subcommand)]
    Order(ModelOrderCommand),

    /// Model aliases
    #[command(subcommand)]
    Aliases(ModelAliasCommand),

    /// Fallback chain management
    #[command(subcommand)]
    Fallbacks(ModelFallbackCommand),

    /// Interactive chat with a model
    Chat {
        /// Model ID
        #[arg(short, long)]
        model: Option<String>,
        /// System prompt
        #[arg(short, long)]
        system: Option<String>,
        /// Load files into context
        #[arg(short = 'F', long)]
        file: Vec<std::path::PathBuf>,
        /// Enable streaming
        #[arg(long)]
        stream: bool,
    },

    /// Single completion
    Complete {
        /// Prompt
        prompt: String,
        /// Model ID
        #[arg(short, long)]
        model: Option<String>,
        /// Stream output
        #[arg(long)]
        stream: bool,
        /// Temperature
        #[arg(short, long)]
        temperature: Option<f32>,
        /// Max tokens
        #[arg(long)]
        max_tokens: Option<u32>,
        /// Output file
        #[arg(short, long)]
        output: Option<std::path::PathBuf>,
    },

    /// Generate embeddings
    Embed {
        /// Input text
        text: String,
        /// Model ID
        #[arg(short, long)]
        model: Option<String>,
        /// Output format
        #[arg(short, long, value_enum, default_value = "json")]
        format: EmbedFormat,
    },

    /// Update model list from remote
    Update,
}

#[derive(Subcommand)]
pub enum ModelAuthCommand {
    /// Add API key for a provider
    Add {
        /// Provider name
        provider: String,
        /// API key
        key: String,
    },
    /// Interactive login (OAuth flow)
    Login {
        /// Provider name
        provider: String,
        /// Open browser automatically
        #[arg(short, long)]
        browser: bool,
    },
    /// Setup using token file
    SetupToken {
        /// Provider name
        provider: String,
        /// Path to token file
        #[arg(short = 'p', long)]
        file: std::path::PathBuf,
    },
    /// List configured authentications
    List,
    /// Remove authentication
    Remove {
        /// Provider name
        provider: String,
    },
    /// Test authentication
    Test {
        /// Provider name
        provider: String,
    },
}

#[derive(Subcommand)]
pub enum ModelOrderCommand {
    /// Get current provider order
    Get,
    /// Set provider priority order
    Set {
        /// Provider names in priority order
        providers: Vec<String>,
    },
    /// Add provider to order at specific position
    Add {
        /// Provider name
        provider: String,
        /// Position (0 = highest priority)
        #[arg(short, long, default_value = "0")]
        position: usize,
    },
    /// Remove provider from order
    Remove {
        /// Provider name
        provider: String,
    },
    /// Clear custom order (use defaults)
    Clear,
}

#[derive(Subcommand)]
pub enum ModelAliasCommand {
    /// List all aliases
    List,
    /// Add an alias
    Add {
        /// Alias name
        name: String,
        /// Target model ID
        target: String,
    },
    /// Remove an alias
    Remove {
        /// Alias name
        name: String,
    },
    /// Show alias details
    Show {
        /// Alias name
        name: String,
    },
}

#[derive(Subcommand)]
pub enum ModelFallbackCommand {
    /// List fallback chain
    List {
        /// Task type
        #[arg(short = 't', long, value_enum, default_value = "default")]
        for_task: TaskType,
    },
    /// Add model to fallback chain
    Add {
        /// Model ID
        model: String,
        /// Position in chain
        #[arg(short, long)]
        position: Option<usize>,
        /// Task type
        #[arg(short = 't', long, value_enum, default_value = "default")]
        for_task: TaskType,
    },
    /// Remove model from fallback chain
    Remove {
        /// Model ID
        model: String,
        /// Task type
        #[arg(short = 't', long, value_enum, default_value = "default")]
        for_task: TaskType,
    },
    /// Clear fallback chain
    Clear {
        /// Task type
        #[arg(short = 't', long, value_enum, default_value = "default")]
        for_task: TaskType,
    },
    /// Test fallback chain
    Test {
        /// Task type
        #[arg(short = 't', long, value_enum, default_value = "default")]
        for_task: TaskType,
    },
}

#[derive(ValueEnum, Clone, Debug)]
pub enum TaskType {
    Default,
    Chat,
    Completion,
    Embedding,
    Image,
    Audio,
    Code,
    Analysis,
    Creative,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum ScanSource {
    All,
    Configured,
    Ollama,
    Openai,
    Anthropic,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum OutputFormat {
    Table,
    Json,
    Yaml,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum EmbedFormat {
    Json,
    Csv,
    Binary,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum CompareDimension {
    Price,
    Speed,
    Quality,
    Context,
    Capabilities,
}

pub async fn execute(args: ModelArgs) -> Result<()> {
    let client = crate::client::ApiClient::new()?;

    match args.command {
        ModelCommand::List {
            provider,
            verbose,
            format,
        } => {
            let progress = TaskProgress::new("Fetching models");
            let models = client.list_models(provider.as_deref()).await?;
            progress.finish_success(Some(&format!("{} models", models.len())));

            match format {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&models)?);
                }
                OutputFormat::Yaml => {
                    println!("{}", serde_yaml::to_string(&models)?);
                }
                OutputFormat::Table => {
                    if verbose {
                        println!(
                            "{:<20} {:<15} {:<25} {:<12} {:<10}",
                            "ID", "Provider", "Name", "Context", "Default"
                        );
                        println!("{}", "-".repeat(100));
                        for m in models {
                            let default_icon = if m.is_default { "✓" } else { "" };
                            println!(
                                "{:<20} {:<15} {:<25} {:<12} {:<10}",
                                m.id, m.provider, m.name, m.context_length, default_icon
                            );
                        }
                    } else {
                        println!(
                            "{:<20} {:<15} {:<25} {:<10}",
                            "ID", "Provider", "Name", "Default"
                        );
                        println!("{}", "-".repeat(80));
                        for m in models {
                            let default_icon = if m.is_default { "✓" } else { "" };
                            println!(
                                "{:<20} {:<15} {:<25} {:<10}",
                                m.id, m.provider, m.name, default_icon
                            );
                        }
                    }
                }
            }
        }

        ModelCommand::Status {
            id,
            provider_details,
        } => {
            let progress = TaskProgress::new("Checking model status");

            if let Some(model_id) = id {
                let status = client.get_model_status(&model_id).await?;
                progress.finish_success(None);

                println!("📊 Model: {} ({})", status.name, status.id);
                println!("Status: {}", status.status);
                println!("Availability: {}%", status.availability_percentage);
                println!("Average latency: {}ms", status.avg_latency_ms);
                println!("Rate limit: {}/min", status.rate_limit_per_minute);

                if provider_details {
                    println!("\nProvider Details:");
                    println!("  Provider: {}", status.provider);
                    println!("  Region: {}", status.region);
                    println!("  API version: {}", status.api_version);
                }
            } else {
                let statuses = client.get_all_model_status().await?;
                progress.finish_success(Some(&format!("{} models checked", statuses.len())));

                println!(
                    "{:<20} {:<12} {:<10} {:<15}",
                    "Model", "Status", "Latency", "Availability"
                );
                println!("{}", "-".repeat(70));
                for s in statuses {
                    let status_icon = match s.status.as_str() {
                        "available" => "🟢",
                        "degraded" => "🟡",
                        "unavailable" => "🔴",
                        _ => "⚪",
                    };
                    println!(
                        "{:<20} {} {:<10} {:<10}ms {:<15}",
                        s.id,
                        status_icon,
                        s.status,
                        s.avg_latency_ms,
                        format!("{}%", s.availability_percentage)
                    );
                }
            }
        }

        ModelCommand::Set { id, for_task } => {
            let progress = TaskProgress::new("Setting default model");
            client
                .set_default_model(&id, &format!("{:?}", for_task).to_lowercase())
                .await?;
            progress.finish_success(None);
            println!("✅ Set '{}' as default for {:?} tasks", id, for_task);
        }

        ModelCommand::SetImage { id } => {
            let progress = TaskProgress::new("Setting default image model");
            client.set_default_image_model(&id).await?;
            progress.finish_success(None);
            println!("✅ Set '{}' as default image model", id);
        }

        ModelCommand::Scan {
            source,
            ollama_url,
            capability,
        } => {
            let progress = TaskProgress::new("Scanning for models");
            let options = ScanOptions {
                source: format!("{:?}", source).to_lowercase(),
                ollama_url,
                capabilities: capability,
            };
            let models = client.scan_models(&options).await?;
            progress.finish_success(Some(&format!("{} models found", models.len())));

            println!("\n🤖 Available Models:");
            for m in models {
                println!("  • {} ({}) - {}", m.id, m.provider, m.name);
                if !m.capabilities.is_empty() {
                    println!("    Capabilities: {}", m.capabilities.join(", "));
                }
            }
        }

        ModelCommand::Info { id, pricing } => {
            let progress = TaskProgress::new("Fetching model info");
            let info = client.get_model_info(&id).await?;
            progress.finish_success(None);

            println!("🤖 Model: {}", info.name);
            println!("===================");
            println!("ID: {}", info.id);
            println!("Provider: {}", info.provider);
            println!("Context length: {} tokens", info.context_length);
            println!("Training cutoff: {}", info.training_cutoff);

            if !info.capabilities.is_empty() {
                println!("\nCapabilities:");
                for cap in info.capabilities {
                    println!("  ✓ {}", cap);
                }
            }

            if pricing {
                println!("\nPricing:");
                println!("  Input: ${:.2} / 1M tokens", info.pricing.input_per_1m);
                println!("  Output: ${:.2} / 1M tokens", info.pricing.output_per_1m);
            }
        }

        ModelCommand::Compare { models, dimension } => {
            let progress = TaskProgress::new("Comparing models");
            let comparison = client.compare_models(&models, &dimension).await?;
            progress.finish_success(None);

            println!("📊 Model Comparison");
            println!("{}", "=".repeat(80));

            // Print comparison table
            for (i, result) in comparison.iter().enumerate() {
                println!("\n{}. {}", i + 1, result.model_id);
                for (dim, score) in &result.scores {
                    let bar = "█".repeat((*score * 20.0) as usize);
                    println!("  {:15} [{:<20}] {:.1}/10", dim, bar, score * 10.0);
                }
            }
        }

        ModelCommand::Test {
            model,
            prompt,
            max_tokens,
            runs,
        } => {
            let progress = TaskProgress::new("Testing model");

            let mut total_latency = 0u64;
            let mut total_tokens = 0u32;

            for i in 0..runs {
                if runs > 1 {
                    print!("Run {}/{}... ", i + 1, runs);
                    io::stdout().flush()?;
                }

                let result = client
                    .test_model(model.as_deref(), &prompt, max_tokens)
                    .await?;
                total_latency += result.latency_ms;
                total_tokens += result.tokens_generated;

                if runs == 1 {
                    println!("\n📝 Prompt: {}", prompt);
                    println!("\n🤖 Response:");
                    println!("{}", result.response);
                    println!("\n📊 Stats:");
                    println!("  Latency: {}ms", result.latency_ms);
                    println!("  Tokens: {} generated", result.tokens_generated);
                }
            }

            progress.finish_success(None);

            if runs > 1 {
                let avg_latency = total_latency / runs as u64;
                println!("\n📊 Average Stats ({} runs):", runs);
                println!("  Latency: {}ms", avg_latency);
                println!("  Tokens: {}", total_tokens / runs);
            }
        }

        ModelCommand::Auth(cmd) => match cmd {
            ModelAuthCommand::Add { provider, key } => {
                let progress = TaskProgress::new(format!("Adding credentials for {}", provider));
                client.add_provider_credentials(&provider, &key).await?;
                progress.finish_success(None);
                println!("✅ Credentials added for {}", provider);
            }
            ModelAuthCommand::Login { provider, browser } => {
                let progress = TaskProgress::new(format!("Authenticating with {}", provider));
                let auth_url = client.get_provider_auth_url(&provider).await?;

                if browser {
                    open::that(&auth_url)?;
                    println!("Opening browser for authentication...");
                } else {
                    println!("Please open this URL to authenticate:");
                    println!("  {}", auth_url);
                }

                println!("\nWaiting for authentication...");
                client.wait_for_provider_auth(&provider).await?;
                progress.finish_success(None);
                println!("✅ Successfully authenticated with {}", provider);
            }
            ModelAuthCommand::SetupToken { provider, file } => {
                let token = std::fs::read_to_string(&file)?;
                let progress = TaskProgress::new(format!("Setting up token for {}", provider));
                client.set_provider_token(&provider, &token).await?;
                progress.finish_success(None);
                println!("✅ Token configured for {}", provider);
            }
            ModelAuthCommand::List => {
                let progress = TaskProgress::new("Listing authentications");
                let auths = client.list_provider_auths().await?;
                progress.finish_success(Some(&format!("{} providers", auths.len())));

                println!(
                    "{:<20} {:<15} {:<20}",
                    "Provider", "Status", "Last Verified"
                );
                println!("{}", "-".repeat(60));
                for auth in auths {
                    let status_icon = if auth.valid { "✓" } else { "✗" };
                    println!(
                        "{:<20} {} {:<14} {:<20}",
                        auth.provider, status_icon, auth.status, auth.last_verified
                    );
                }
            }
            ModelAuthCommand::Remove { provider } => {
                let progress = TaskProgress::new(format!("Removing credentials for {}", provider));
                client.remove_provider_credentials(&provider).await?;
                progress.finish_success(None);
                println!("✅ Credentials removed for {}", provider);
            }
            ModelAuthCommand::Test { provider } => {
                let progress = TaskProgress::new(format!("Testing {}", provider));
                let result = client.test_provider_auth(&provider).await?;
                progress.finish_success(None);

                if result.valid {
                    println!("✅ {} authentication is valid", provider);
                    println!("   Rate limit: {}/min", result.rate_limit);
                    println!("   Available models: {}", result.available_models);
                } else {
                    println!("✗ {} authentication failed", provider);
                    if let Some(error) = result.error {
                        println!("   Error: {}", error);
                    }
                }
            }
        },

        ModelCommand::Order(cmd) => match cmd {
            ModelOrderCommand::Get => {
                let order = client.get_provider_order().await?;
                println!("Provider Priority Order:");
                for (i, provider) in order.iter().enumerate() {
                    println!("  {}. {}", i + 1, provider);
                }
            }
            ModelOrderCommand::Set { providers } => {
                client.set_provider_order(&providers).await?;
                println!("✅ Provider order updated");
            }
            ModelOrderCommand::Add { provider, position } => {
                client.add_provider_to_order(&provider, position).await?;
                println!("✅ '{}' added at position {}", provider, position);
            }
            ModelOrderCommand::Remove { provider } => {
                client.remove_provider_from_order(&provider).await?;
                println!("✅ '{}' removed from order", provider);
            }
            ModelOrderCommand::Clear => {
                client.clear_provider_order().await?;
                println!("✅ Provider order reset to defaults");
            }
        },

        ModelCommand::Aliases(cmd) => match cmd {
            ModelAliasCommand::List => {
                let aliases = client.list_model_aliases().await?;
                println!("Model Aliases:");
                println!("{:<20} {:<30}", "Alias", "Target");
                println!("{}", "-".repeat(55));
                for alias in aliases {
                    println!("{:<20} {:<30}", alias.name, alias.target);
                }
            }
            ModelAliasCommand::Add { name, target } => {
                client.add_model_alias(&name, &target).await?;
                println!("✅ Alias '{}' -> '{}' created", name, target);
            }
            ModelAliasCommand::Remove { name } => {
                client.remove_model_alias(&name).await?;
                println!("✅ Alias '{}' removed", name);
            }
            ModelAliasCommand::Show { name } => {
                let alias = client.get_model_alias(&name).await?;
                println!("Alias: {}", alias.name);
                println!("Target: {}", alias.target);
                println!("Created: {}", alias.created_at);
            }
        },

        ModelCommand::Fallbacks(cmd) => match cmd {
            ModelFallbackCommand::List { for_task } => {
                let chain = client
                    .get_fallback_chain(&format!("{:?}", for_task).to_lowercase())
                    .await?;
                println!("Fallback chain for {:?}:", for_task);
                for (i, model) in chain.iter().enumerate() {
                    println!("  {}. {}", i + 1, model);
                }
            }
            ModelFallbackCommand::Add {
                model,
                position,
                for_task,
            } => {
                client
                    .add_to_fallback_chain(
                        &model,
                        position,
                        &format!("{:?}", for_task).to_lowercase(),
                    )
                    .await?;
                println!("✅ '{}' added to fallback chain", model);
            }
            ModelFallbackCommand::Remove { model, for_task } => {
                client
                    .remove_from_fallback_chain(&model, &format!("{:?}", for_task).to_lowercase())
                    .await?;
                println!("✅ '{}' removed from fallback chain", model);
            }
            ModelFallbackCommand::Clear { for_task } => {
                client
                    .clear_fallback_chain(&format!("{:?}", for_task).to_lowercase())
                    .await?;
                println!("✅ Fallback chain cleared");
            }
            ModelFallbackCommand::Test { for_task } => {
                let progress = TaskProgress::new("Testing fallback chain");
                let result = client
                    .test_fallback_chain(&format!("{:?}", for_task).to_lowercase())
                    .await?;
                progress.finish_success(None);

                println!("Fallback chain test results:");
                for r in result {
                    let icon = if r.available { "✓" } else { "✗" };
                    println!("  {} {}: {}", icon, r.model_id, r.message);
                }
            }
        },

        ModelCommand::Chat {
            model,
            system,
            file,
            stream,
        } => {
            println!("🤖 Interactive Chat (type 'exit' or 'quit' to end)\n");

            let mut messages = vec![];

            if let Some(sys) = system {
                messages.push(ChatMessage {
                    role: "system".to_string(),
                    content: sys,
                });
            }

            // Load files into context
            for f in file {
                match std::fs::read_to_string(&f) {
                    Ok(content) => {
                        messages.push(ChatMessage {
                            role: "user".to_string(),
                            content: format!("File '{}':\n{}", f.display(), content),
                        });
                    }
                    Err(e) => {
                        eprintln!("Warning: Could not read file '{}': {}", f.display(), e);
                    }
                }
            }

            let mut rl = rustyline::DefaultEditor::new()?;

            loop {
                let input = rl.readline("> ")?;
                let input = input.trim();

                if input.eq_ignore_ascii_case("exit") || input.eq_ignore_ascii_case("quit") {
                    break;
                }

                if input.is_empty() {
                    continue;
                }

                rl.add_history_entry(input)?;
                messages.push(ChatMessage {
                    role: "user".to_string(),
                    content: input.to_string(),
                });

                if stream {
                    print!("🤖 ");
                    io::stdout().flush()?;

                    let mut response_text = String::new();
                    let mut stream = client.chat_stream(model.as_deref(), &messages).await?;

                    while let Some(chunk) = stream.next().await {
                        match chunk {
                            Ok(text) => {
                                print!("{}", text);
                                io::stdout().flush()?;
                                response_text.push_str(&text);
                            }
                            Err(e) => {
                                eprintln!("\nError: {}", e);
                                break;
                            }
                        }
                    }
                    println!();
                    messages.push(ChatMessage {
                        role: "assistant".to_string(),
                        content: response_text,
                    });
                } else {
                    let response = client.chat(model.as_deref(), &messages).await?;
                    println!("🤖 {}\n", response.content);
                    messages.push(response);
                }
            }
        }

        ModelCommand::Complete {
            prompt,
            model,
            stream,
            temperature,
            max_tokens,
            output,
        } => {
            let req = CompletionRequest {
                prompt,
                model_id: model,
                temperature,
                max_tokens,
            };

            if stream {
                let mut stream = client.complete_stream(&req).await?;
                let mut full_response = String::new();

                while let Some(chunk) = stream.next().await {
                    match chunk {
                        Ok(text) => {
                            print!("{}", text);
                            io::stdout().flush()?;
                            full_response.push_str(&text);
                        }
                        Err(e) => {
                            eprintln!("\nError: {}", e);
                            break;
                        }
                    }
                }
                println!();

                if let Some(path) = output {
                    std::fs::write(&path, full_response)?;
                    println!("\n✅ Output saved to {}", path.display());
                }
            } else {
                let progress = TaskProgress::new("Generating completion");
                let response = client.complete(&req).await?;
                progress.finish_success(Some(&format!("{} tokens", response.tokens_generated)));

                if let Some(path) = output {
                    std::fs::write(&path, &response.text)?;
                    println!("✅ Output saved to {}", path.display());
                } else {
                    println!("{}", response.text);
                }

                if response.tokens_generated > 0 {
                    println!(
                        "\n📊 {} tokens generated in {}ms",
                        response.tokens_generated, response.latency_ms
                    );
                }
            }
        }

        ModelCommand::Embed {
            text,
            model,
            format,
        } => {
            let progress = TaskProgress::new("Generating embeddings");
            let embedding = client.generate_embeddings(model.as_deref(), &text).await?;
            progress.finish_success(Some(&format!("{} dimensions", embedding.dimensions)));

            match format {
                EmbedFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&embedding.vector)?);
                }
                EmbedFormat::Csv => {
                    let csv = embedding
                        .vector
                        .iter()
                        .map(|v| v.to_string())
                        .collect::<Vec<_>>()
                        .join(",");
                    println!("{}", csv);
                }
                EmbedFormat::Binary => {
                    // Output raw binary
                    use std::io::Write;
                    let bytes: Vec<u8> = embedding
                        .vector
                        .iter()
                        .flat_map(|&f| f.to_le_bytes().to_vec())
                        .collect();
                    io::stdout().write_all(&bytes)?;
                }
            }
        }

        ModelCommand::Update => {
            let progress = TaskProgress::new("Updating model list");
            let result = client.update_model_list().await?;
            progress.finish_success(Some(&format!("+{} -{}", result.added, result.removed)));
            println!("✅ Model list updated");
            println!("  Added: {}", result.added);
            println!("  Removed: {}", result.removed);
            println!("  Updated: {}", result.updated);
        }
    }

    Ok(())
}

// Request/Response types
#[derive(serde::Serialize, serde::Deserialize)]
struct Model {
    id: String,
    name: String,
    provider: String,
    context_length: u64,
    is_default: bool,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ModelStatus {
    id: String,
    name: String,
    status: String,
    availability_percentage: f64,
    avg_latency_ms: u64,
    rate_limit_per_minute: u64,
    provider: String,
    region: String,
    api_version: String,
}

#[derive(serde::Serialize)]
struct ScanOptions {
    source: String,
    ollama_url: String,
    capabilities: Vec<String>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ScannedModel {
    id: String,
    name: String,
    provider: String,
    capabilities: Vec<String>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ModelInfo {
    id: String,
    name: String,
    provider: String,
    context_length: u64,
    training_cutoff: String,
    capabilities: Vec<String>,
    pricing: ModelPricing,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ModelPricing {
    input_per_1m: f64,
    output_per_1m: f64,
}

#[derive(serde::Deserialize)]
struct ComparisonResult {
    model_id: String,
    scores: Vec<(String, f64)>,
}

#[derive(serde::Deserialize)]
struct TestResult {
    response: String,
    latency_ms: u64,
    tokens_generated: u32,
}

#[derive(serde::Deserialize)]
struct ProviderAuth {
    provider: String,
    status: String,
    valid: bool,
    last_verified: String,
}

#[derive(serde::Deserialize)]
struct AuthTestResult {
    valid: bool,
    rate_limit: u64,
    available_models: usize,
    error: Option<String>,
}

#[derive(serde::Deserialize)]
struct ModelAlias {
    name: String,
    target: String,
    created_at: String,
}

#[derive(serde::Deserialize)]
struct FallbackTestResult {
    model_id: String,
    available: bool,
    message: String,
}

#[derive(serde::Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(serde::Serialize)]
struct CompletionRequest {
    prompt: String,
    model_id: Option<String>,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
}

#[derive(serde::Deserialize)]
struct CompletionResponse {
    text: String,
    tokens_generated: u32,
    latency_ms: u64,
}

#[derive(serde::Deserialize)]
struct Embedding {
    dimensions: usize,
    vector: Vec<f32>,
}

#[derive(serde::Deserialize)]
struct ModelUpdateResult {
    added: usize,
    removed: usize,
    updated: usize,
}

use futures::Stream;

// Client extension trait
trait ModelClient {
    async fn list_models(&self, provider: Option<&str>) -> Result<Vec<Model>>;
    async fn get_model_status(&self, id: &str) -> Result<ModelStatus>;
    async fn get_all_model_status(&self) -> Result<Vec<ModelStatus>>;
    async fn set_default_model(&self, id: &str, task_type: &str) -> Result<()>;
    async fn set_default_image_model(&self, id: &str) -> Result<()>;
    async fn scan_models(&self, options: &ScanOptions) -> Result<Vec<ScannedModel>>;
    async fn get_model_info(&self, id: &str) -> Result<ModelInfo>;
    async fn compare_models(
        &self,
        models: &[String],
        dimensions: &[CompareDimension],
    ) -> Result<Vec<ComparisonResult>>;
    async fn test_model(
        &self,
        model: Option<&str>,
        prompt: &str,
        max_tokens: u32,
    ) -> Result<TestResult>;
    async fn add_provider_credentials(&self, provider: &str, key: &str) -> Result<()>;
    async fn get_provider_auth_url(&self, provider: &str) -> Result<String>;
    async fn wait_for_provider_auth(&self, provider: &str) -> Result<()>;
    async fn set_provider_token(&self, provider: &str, token: &str) -> Result<()>;
    async fn list_provider_auths(&self) -> Result<Vec<ProviderAuth>>;
    async fn remove_provider_credentials(&self, provider: &str) -> Result<()>;
    async fn test_provider_auth(&self, provider: &str) -> Result<AuthTestResult>;
    async fn get_provider_order(&self) -> Result<Vec<String>>;
    async fn set_provider_order(&self, providers: &[String]) -> Result<()>;
    async fn add_provider_to_order(&self, provider: &str, position: usize) -> Result<()>;
    async fn remove_provider_from_order(&self, provider: &str) -> Result<()>;
    async fn clear_provider_order(&self) -> Result<()>;
    async fn list_model_aliases(&self) -> Result<Vec<ModelAlias>>;
    async fn add_model_alias(&self, name: &str, target: &str) -> Result<()>;
    async fn remove_model_alias(&self, name: &str) -> Result<()>;
    async fn get_model_alias(&self, name: &str) -> Result<ModelAlias>;
    async fn get_fallback_chain(&self, task_type: &str) -> Result<Vec<String>>;
    async fn add_to_fallback_chain(
        &self,
        model: &str,
        position: Option<usize>,
        task_type: &str,
    ) -> Result<()>;
    async fn remove_from_fallback_chain(&self, model: &str, task_type: &str) -> Result<()>;
    async fn clear_fallback_chain(&self, task_type: &str) -> Result<()>;
    async fn test_fallback_chain(&self, task_type: &str) -> Result<Vec<FallbackTestResult>>;
    async fn chat(&self, model: Option<&str>, messages: &[ChatMessage]) -> Result<ChatMessage>;
    async fn chat_stream(
        &self,
        model: Option<&str>,
        messages: &[ChatMessage],
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>>;
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse>;
    async fn complete_stream(
        &self,
        request: &CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>>;
    async fn generate_embeddings(&self, model: Option<&str>, text: &str) -> Result<Embedding>;
    async fn update_model_list(&self) -> Result<ModelUpdateResult>;
}

use std::pin::Pin;

// Stub implementations
impl ModelClient for crate::client::ApiClient {
    async fn list_models(&self, _provider: Option<&str>) -> Result<Vec<Model>> {
        Ok(vec![])
    }
    async fn get_model_status(&self, _id: &str) -> Result<ModelStatus> {
        anyhow::bail!("Not implemented")
    }
    async fn get_all_model_status(&self) -> Result<Vec<ModelStatus>> {
        Ok(vec![])
    }
    async fn set_default_model(&self, _id: &str, _task_type: &str) -> Result<()> {
        Ok(())
    }
    async fn set_default_image_model(&self, _id: &str) -> Result<()> {
        Ok(())
    }
    async fn scan_models(&self, _options: &ScanOptions) -> Result<Vec<ScannedModel>> {
        Ok(vec![])
    }
    async fn get_model_info(&self, _id: &str) -> Result<ModelInfo> {
        anyhow::bail!("Not implemented")
    }
    async fn compare_models(
        &self,
        _models: &[String],
        _dimensions: &[CompareDimension],
    ) -> Result<Vec<ComparisonResult>> {
        Ok(vec![])
    }
    async fn test_model(
        &self,
        _model: Option<&str>,
        _prompt: &str,
        _max_tokens: u32,
    ) -> Result<TestResult> {
        anyhow::bail!("Not implemented")
    }
    async fn add_provider_credentials(&self, _provider: &str, _key: &str) -> Result<()> {
        Ok(())
    }
    async fn get_provider_auth_url(&self, _provider: &str) -> Result<String> {
        anyhow::bail!("Not implemented")
    }
    async fn wait_for_provider_auth(&self, _provider: &str) -> Result<()> {
        Ok(())
    }
    async fn set_provider_token(&self, _provider: &str, _token: &str) -> Result<()> {
        Ok(())
    }
    async fn list_provider_auths(&self) -> Result<Vec<ProviderAuth>> {
        Ok(vec![])
    }
    async fn remove_provider_credentials(&self, _provider: &str) -> Result<()> {
        Ok(())
    }
    async fn test_provider_auth(&self, _provider: &str) -> Result<AuthTestResult> {
        anyhow::bail!("Not implemented")
    }
    async fn get_provider_order(&self) -> Result<Vec<String>> {
        Ok(vec![])
    }
    async fn set_provider_order(&self, _providers: &[String]) -> Result<()> {
        Ok(())
    }
    async fn add_provider_to_order(&self, _provider: &str, _position: usize) -> Result<()> {
        Ok(())
    }
    async fn remove_provider_from_order(&self, _provider: &str) -> Result<()> {
        Ok(())
    }
    async fn clear_provider_order(&self) -> Result<()> {
        Ok(())
    }
    async fn list_model_aliases(&self) -> Result<Vec<ModelAlias>> {
        Ok(vec![])
    }
    async fn add_model_alias(&self, _name: &str, _target: &str) -> Result<()> {
        Ok(())
    }
    async fn remove_model_alias(&self, _name: &str) -> Result<()> {
        Ok(())
    }
    async fn get_model_alias(&self, _name: &str) -> Result<ModelAlias> {
        anyhow::bail!("Not implemented")
    }
    async fn get_fallback_chain(&self, _task_type: &str) -> Result<Vec<String>> {
        Ok(vec![])
    }
    async fn add_to_fallback_chain(
        &self,
        _model: &str,
        _position: Option<usize>,
        _task_type: &str,
    ) -> Result<()> {
        Ok(())
    }
    async fn remove_from_fallback_chain(&self, _model: &str, _task_type: &str) -> Result<()> {
        Ok(())
    }
    async fn clear_fallback_chain(&self, _task_type: &str) -> Result<()> {
        Ok(())
    }
    async fn test_fallback_chain(&self, _task_type: &str) -> Result<Vec<FallbackTestResult>> {
        Ok(vec![])
    }
    async fn chat(&self, _model: Option<&str>, _messages: &[ChatMessage]) -> Result<ChatMessage> {
        anyhow::bail!("Not implemented")
    }
    async fn chat_stream(
        &self,
        _model: Option<&str>,
        _messages: &[ChatMessage],
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        anyhow::bail!("Not implemented")
    }
    async fn complete(&self, _request: &CompletionRequest) -> Result<CompletionResponse> {
        anyhow::bail!("Not implemented")
    }
    async fn complete_stream(
        &self,
        _request: &CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        anyhow::bail!("Not implemented")
    }
    async fn generate_embeddings(&self, _model: Option<&str>, _text: &str) -> Result<Embedding> {
        anyhow::bail!("Not implemented")
    }
    async fn update_model_list(&self) -> Result<ModelUpdateResult> {
        anyhow::bail!("Not implemented")
    }
}
