//! BeeBotOS CLI
//!
//! Command line interface for BeeBotOS.

use std::io::IsTerminal;

use clap::{Parser, Subcommand};

mod client;
mod commands;
mod completion;
mod config;
mod error;
mod logging;
mod network;
mod output;
mod progress;
mod secure_storage;
mod websocket;

use error::print_error;
use logging::{LogFormat, LogLevel, LoggerConfig};

#[derive(Parser)]
#[command(name = "beebot")]
#[command(about = "BeeBotOS Command Line Interface")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// API endpoint
    #[arg(
        short,
        long,
        global = true,
        env = "BEEBOTOS_API_URL",
        default_value = "http://localhost:8080"
    )]
    endpoint: String,

    /// API key
    #[arg(short = 'k', long, global = true, env = "BEEBOTOS_API_KEY")]
    api_key: Option<String>,

    /// Output format
    #[arg(short, long, global = true, value_enum, default_value = "pretty")]
    format: OutputFormat,

    /// Enable verbose output (use -v, -vv, -vvv for more detail)
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Log format (text or json)
    #[arg(long, global = true, value_enum, default_value = "text")]
    log_format: LogFormatArg,

    /// Disable colored output
    #[arg(long, global = true, env = "NO_COLOR")]
    no_color: bool,

    /// Proxy URL (overrides environment variables)
    #[arg(long, global = true, env = "HTTP_PROXY")]
    proxy: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Agent management
    Agent(commands::agent::AgentArgs),

    /// Cognitive functions
    Brain(commands::brain::BrainArgs),

    /// Browser automation
    Browser(commands::browser::BrowserArgs),

    /// Blockchain operations
    Chain(commands::chain::ChainArgs),

    /// Channel management (Telegram, Discord, WeChat, etc.)
    Channel(commands::channel::ChannelArgs),

    /// Configuration management
    Config(commands::config::ConfigArgs),

    /// Contract deployment
    Deploy(commands::deploy::DeployArgs),

    /// Health diagnosis
    Doctor(commands::doctor::DoctorArgs),

    /// Gateway management
    Gateway(commands::gateway::GatewayArgs),

    /// System information
    Info(commands::info::InfoArgs),

    /// AI inference capabilities (text, image, audio, video)
    Infer(commands::infer::InferArgs),

    /// Interactive shell
    Interactive(commands::interactive::InteractiveArgs),

    /// View logs
    Logs(commands::logs::LogsArgs),

    /// Memory management (STM, LTM, EM)
    Memory(commands::memory::MemoryArgs),

    /// Message operations
    Message(commands::message::MessageArgs),

    /// Payment operations
    Payment(commands::payment::PaymentArgs),

    /// LLM model management
    Model(commands::model::ModelArgs),

    /// Create proposal
    Propose(commands::propose::ProposeArgs),

    /// Security audit and secrets
    Security(commands::security::SecurityArgs),

    /// Session management
    Session(commands::session::SessionArgs),

    /// First-time setup
    Setup(commands::setup::SetupArgs),

    /// Skill management
    Skill(commands::skill::SkillArgs),

    /// Cast vote
    Vote(commands::vote::VoteArgs),

    /// Watch resources
    Watch(commands::watch::WatchArgs),

    /// Generate shell completions
    Completion {
        /// Shell type
        shell: String,
    },
}

#[derive(Clone, Copy, Debug, Default, clap::ValueEnum)]
pub enum OutputFormat {
    #[default]
    Pretty,
    Json,
    Yaml,
}

#[derive(Clone, Copy, Debug, Default, clap::ValueEnum)]
pub enum LogFormatArg {
    #[default]
    Text,
    Json,
}

impl From<LogFormatArg> for LogFormat {
    fn from(arg: LogFormatArg) -> Self {
        match arg {
            LogFormatArg::Text => LogFormat::Text,
            LogFormatArg::Json => LogFormat::Json,
        }
    }
}

/// Initialize logging based on CLI arguments
fn init_logging(verbose: u8, format: LogFormatArg, no_color: bool) {
    let level = match verbose {
        0 => LogLevel::Warn,
        1 => LogLevel::Info,
        2 => LogLevel::Debug,
        _ => LogLevel::Trace,
    };

    let config = LoggerConfig {
        level,
        format: format.into(),
        colors: !no_color && std::io::stderr().is_terminal(),
        timestamp: verbose > 0,
        target: verbose > 1,
    };

    logging::init(config);
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    init_logging(cli.verbose, cli.log_format, cli.no_color);

    // Log startup information
    log_debug!("Starting BeeBotOS CLI v{}", env!("CARGO_PKG_VERSION"));
    log_debug!("API endpoint: {}", cli.endpoint);

    // Set up API key from CLI arg or env
    if let Some(api_key) = &cli.api_key {
        std::env::set_var("BEEBOTOS_API_KEY", api_key);
        log_debug!("API key set from command line");
    }

    // Set proxy if provided
    if let Some(proxy) = &cli.proxy {
        std::env::set_var("HTTP_PROXY", proxy);
        log_info!("Using proxy: {}", proxy);
    }

    let result = match cli.command {
        Commands::Agent(args) => commands::agent::execute(args).await,
        Commands::Brain(args) => commands::brain::execute(args).await,
        Commands::Browser(args) => commands::browser::execute(args).await,
        Commands::Chain(args) => commands::chain::execute(args).await,
        Commands::Channel(args) => commands::channel::execute(args).await,
        Commands::Config(args) => {
            let config = config::Config::load()?;
            commands::config::run(args, config).await
        }
        Commands::Doctor(args) => commands::doctor::execute(args).await,
        Commands::Deploy(args) => {
            let config = config::Config::load()?;
            commands::deploy::run(args, config).await
        }
        Commands::Gateway(args) => commands::gateway::execute(args).await,
        Commands::Info(args) => commands::info::execute(args).await,
        Commands::Infer(args) => commands::infer::execute(args).await,
        Commands::Interactive(args) => commands::interactive::execute(args).await,
        Commands::Logs(args) => {
            let config = config::Config::load()?;
            commands::logs::run(args, config).await
        }
        Commands::Memory(args) => commands::memory::execute(args).await,
        Commands::Message(args) => commands::message::execute(args).await,
        Commands::Model(args) => commands::model::execute(args).await,
        Commands::Payment(args) => commands::payment::execute(args).await,
        Commands::Propose(args) => {
            let config = config::Config::load()?;
            commands::propose::run(args, config).await
        }
        Commands::Security(args) => commands::security::execute(args).await,
        Commands::Session(args) => commands::session::execute(args).await,
        Commands::Setup(args) => commands::setup::execute(args).await,
        Commands::Skill(args) => commands::skill::execute(args).await,
        Commands::Vote(args) => {
            let config = config::Config::load()?;
            commands::vote::run(args, config).await
        }
        Commands::Watch(args) => commands::watch::execute(args).await,
        Commands::Completion { shell } => {
            let mut cmd = <Cli as clap::CommandFactory>::command();
            completion::generate(&shell, &mut cmd)
        }
    };

    match result {
        Ok(_) => {
            log_debug!("Command completed successfully");
            Ok(())
        }
        Err(e) => {
            log_error!("Command failed: {}", e);
            let cli_err = error::CliError::from_anyhow(e);
            print_error(&cli_err);
            std::process::exit(1);
        }
    }
}
