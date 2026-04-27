use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::progress::TaskProgress;

#[derive(Parser)]
pub struct SessionArgs {
    #[command(subcommand)]
    pub command: SessionCommand,
}

#[derive(Subcommand)]
pub enum SessionCommand {
    /// Create a new session
    Create {
        /// Agent ID
        #[arg(long)]
        agent: String,

        /// Session name
        #[arg(short, long)]
        name: Option<String>,
    },

    /// List sessions
    List {
        /// Filter by agent
        #[arg(long)]
        agent: Option<String>,

        /// Show only active sessions
        #[arg(long)]
        active: bool,
    },

    /// Resume a session
    Resume {
        /// Session ID
        id: String,
    },

    /// Show session details
    Show {
        /// Session ID
        id: String,

        /// Show transcript
        #[arg(short, long)]
        transcript: bool,
    },

    /// Archive a session
    Archive {
        /// Session ID
        id: String,
    },

    /// Delete a session
    Delete {
        /// Session ID
        id: String,

        /// Force delete without confirmation
        #[arg(long)]
        force: bool,
    },
}

pub async fn execute(args: SessionArgs) -> Result<()> {
    let client = crate::client::ApiClient::new()?;

    match args.command {
        SessionCommand::Create { agent, name } => {
            let progress = TaskProgress::new("Creating session");
            let session = client.create_session(&agent, name.as_deref()).await?;
            progress.finish_success(Some(&session.id));
            println!("Session key: {}", session.key);
        }

        SessionCommand::List { agent, active } => {
            let progress = TaskProgress::new("Listing sessions");
            let sessions = client.list_sessions(agent.as_deref(), active).await?;
            progress.finish_success(Some(&format!("{} sessions", sessions.len())));
            println!(
                "{:<36} {:<20} {:<15} {:<20}",
                "ID", "Name", "Status", "Last Active"
            );
            println!("{}", "-".repeat(91));
            for session in sessions {
                println!(
                    "{:<36} {:<20} {:<15} {:<20}",
                    session.id, session.name, session.status, session.last_active
                );
            }
        }

        SessionCommand::Resume { id } => {
            let progress = TaskProgress::new(format!("Resuming session {}", id));
            let session = client.resume_session(&id).await?;
            progress.finish_success(None);
            println!("Agent: {}", session.agent_id);
            println!("Context restored with {} items.", session.context_items);
        }

        SessionCommand::Show { id, transcript } => {
            let progress = TaskProgress::new("Fetching session details");
            let session = client.get_session(&id).await?;
            progress.finish_success(None);
            println!("Session: {}", session.id);
            println!("Name: {}", session.name);
            println!("Agent: {}", session.agent_id);
            println!("Status: {}", session.status);
            println!("Created: {}", session.created_at);

            if transcript {
                println!("\n--- Transcript ---");
                for entry in session.transcript {
                    println!("[{}] {}: {}", entry.timestamp, entry.role, entry.content);
                }
            }
        }

        SessionCommand::Archive { id } => {
            let progress = TaskProgress::new(format!("Archiving session {}", id));
            client.archive_session(&id).await?;
            progress.finish_success(None);
        }

        SessionCommand::Delete { id, force } => {
            if !force {
                print!("Are you sure you want to delete session '{}'? [y/N] ", id);
                std::io::Write::flush(&mut std::io::stdout())?;
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                if !input.trim().eq_ignore_ascii_case("y") {
                    println!("Cancelled.");
                    return Ok(());
                }
            }
            let progress = TaskProgress::new(format!("Deleting session {}", id));
            client.delete_session(&id).await?;
            progress.finish_success(None);
        }
    }

    Ok(())
}
