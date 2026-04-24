use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::progress::TaskProgress;

#[derive(Parser)]
pub struct SkillArgs {
    #[command(subcommand)]
    pub command: SkillCommand,
}

#[derive(Subcommand)]
pub enum SkillCommand {
    /// List available skills
    List {
        /// Filter by category
        #[arg(short, long)]
        category: Option<String>,

        /// Search query
        #[arg(short, long)]
        search: Option<String>,
    },

    /// Show skill details
    Show {
        /// Skill ID
        id: String,
    },

    /// Install a skill
    Install {
        /// Skill ID or path
        source: String,

        /// Target agent
        #[arg(long)]
        agent: Option<String>,

        /// Version constraint
        #[arg(long)]
        version: Option<String>,
    },

    /// Uninstall a skill
    Uninstall {
        /// Skill ID
        id: String,

        /// Target agent
        #[arg(long)]
        agent: Option<String>,
    },

    /// Update a skill
    Update {
        /// Skill ID
        id: String,

        /// Target agent
        #[arg(long)]
        agent: Option<String>,
    },

    /// Create a new skill from template
    Create {
        /// Skill name
        name: String,

        /// Template to use
        #[arg(short, long, default_value = "rust")]
        template: String,

        /// Output directory
        #[arg(short, long)]
        output: Option<String>,
    },

    /// Publish a skill to the marketplace
    Publish {
        /// Skill package path
        path: String,

        /// Registry to publish to
        #[arg(short, long, default_value = "default")]
        registry: String,

        /// Skip confirmation
        #[arg(short, long)]
        yes: bool,
    },
}

pub async fn execute(args: SkillArgs) -> Result<()> {
    let client = crate::client::ApiClient::new()?;

    match args.command {
        SkillCommand::List { category, search } => {
            let progress = TaskProgress::new("Listing skills");
            let skills = client
                .list_skills(category.as_deref(), search.as_deref())
                .await?;
            progress.finish_success(Some(&format!("{} skills found", skills.len())));
            println!(
                "{:<30} {:<15} {:<10} {:<20}",
                "Name", "Category", "Version", "Author"
            );
            println!("{}", "-".repeat(75));
            for skill in skills {
                println!(
                    "{:<30} {:<15} {:<10} {:<20}",
                    skill.name, skill.category, skill.version, skill.author
                );
            }
        }

        SkillCommand::Show { id } => {
            let progress = TaskProgress::new("Fetching skill details");
            let skill = client.get_skill(&id).await?;
            progress.finish_success(None);
            println!("Skill: {}", skill.name);
            println!("ID: {}", skill.id);
            println!("Version: {}", skill.version);
            println!("Category: {}", skill.category);
            println!("Author: {}", skill.author);
            println!("Description: {}", skill.description);
            println!("Downloads: {}", skill.downloads);
            println!("Rating: {}/5", skill.rating);
            println!("\nCapabilities:");
            for cap in skill.capabilities {
                println!("  - {}", cap);
            }
        }

        SkillCommand::Install {
            source,
            agent,
            version,
        } => {
            let progress = TaskProgress::new(format!("Installing skill {}", source));
            let skill = client
                .install_skill(&source, agent.as_deref(), version.as_deref())
                .await?;
            progress.finish_success(Some(&skill.version));
            if let Some(agent_id) = agent {
                println!("Installed on agent: {}", agent_id);
            }
        }

        SkillCommand::Uninstall { id, agent } => {
            let progress = TaskProgress::new(format!("Uninstalling skill {}", id));
            client.uninstall_skill(&id, agent.as_deref()).await?;
            progress.finish_success(None);
        }

        SkillCommand::Update { id, agent } => {
            let progress = TaskProgress::new(format!("Updating skill {}", id));
            let skill = client.update_skill(&id, agent.as_deref()).await?;
            progress.finish_success(Some(&format!("v{}", skill.version)));
        }

        SkillCommand::Create {
            name,
            template,
            output,
        } => {
            let dir = output.unwrap_or_else(|| name.clone());
            let progress = TaskProgress::new(format!("Creating skill {}", name));
            client.create_skill_template(&name, &template, &dir).await?;
            progress.finish_success(Some(&format!("in {}", dir)));
        }

        SkillCommand::Publish {
            path,
            registry,
            yes,
        } => {
            if !yes {
                print!("Are you sure you want to publish this skill? [y/N] ");
                std::io::Write::flush(&mut std::io::stdout())?;
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                if !input.trim().eq_ignore_ascii_case("y") {
                    println!("Cancelled.");
                    return Ok(());
                }
            }
            let progress = TaskProgress::new(format!("Publishing to {}", registry));
            let result = client.publish_skill(&path, &registry).await?;
            progress.finish_success(Some(&format!("ID: {}", result.id)));
            println!("Version: {}", result.version);
        }
    }

    Ok(())
}
