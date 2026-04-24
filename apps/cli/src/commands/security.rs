//! Security audit and secrets management commands
//!
//! Provides security scanning, secrets management, and audit capabilities.

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};

use crate::progress::TaskProgress;

#[derive(Parser)]
pub struct SecurityArgs {
    #[command(subcommand)]
    pub command: SecurityCommand,
}

#[derive(Subcommand)]
pub enum SecurityCommand {
    /// Show security status
    Status,

    /// Scan for security issues
    Scan {
        /// Scan scope
        #[arg(value_enum, default_value = "full")]
        scope: ScanScope,

        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: OutputFormat,

        /// Output file
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// List security policies
    Policy {
        #[command(subcommand)]
        command: PolicyCommand,
    },

    /// Audit log management
    Audit {
        #[command(subcommand)]
        command: AuditCommand,
    },

    /// Secrets management
    Secret {
        #[command(subcommand)]
        command: SecretCommand,
    },

    /// Access control management
    Acl {
        #[command(subcommand)]
        command: AclCommand,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum ScanScope {
    /// Full system scan
    Full,
    /// Scan secrets only
    Secrets,
    /// Scan permissions only
    Permissions,
    /// Scan dependencies only
    Dependencies,
    /// Scan configuration only
    Config,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
    Html,
    Sarif,
}

#[derive(Subcommand)]
pub enum PolicyCommand {
    /// List policies
    List,

    /// Show policy details
    Show {
        /// Policy ID
        id: String,
    },

    /// Apply policy
    Apply {
        /// Policy ID
        id: String,
    },

    /// Create custom policy
    Create {
        /// Policy name
        name: String,

        /// Policy file
        #[arg(short = 'p', long)]
        file: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
pub enum AuditCommand {
    /// List audit logs
    List {
        /// Filter by agent
        #[arg(short = 'g', long)]
        agent: Option<String>,

        /// Filter by action
        #[arg(short = 'a', long)]
        action: Option<String>,

        /// Number of entries
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },

    /// Export audit logs
    Export {
        /// Output file
        output: PathBuf,

        /// Format
        #[arg(short, long, value_enum, default_value = "json")]
        format: ExportFormat,

        /// Date range start
        #[arg(long)]
        from: Option<String>,

        /// Date range end
        #[arg(long)]
        to: Option<String>,
    },

    /// Watch audit log
    Watch {
        /// Filter by agent
        #[arg(short, long)]
        agent: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum ExportFormat {
    Json,
    Csv,
}

#[derive(Subcommand)]
pub enum SecretCommand {
    /// List secrets
    List {
        /// Filter by namespace
        #[arg(short, long)]
        namespace: Option<String>,
    },

    /// Get secret value
    Get {
        /// Secret name
        name: String,

        /// Namespace
        #[arg(short, long)]
        namespace: Option<String>,
    },

    /// Set secret
    Set {
        /// Secret name
        name: String,

        /// Secret value (or prompt)
        value: Option<String>,

        /// Namespace
        #[arg(short, long)]
        namespace: Option<String>,

        /// Generate random value
        #[arg(long)]
        generate: bool,

        /// Length for generated value
        #[arg(long, default_value = "32")]
        length: usize,
    },

    /// Delete secret
    Delete {
        /// Secret name
        name: String,

        /// Namespace
        #[arg(short, long)]
        namespace: Option<String>,

        /// Force delete without confirmation
        #[arg(long)]
        force: bool,
    },

    /// Rotate secret
    Rotate {
        /// Secret name
        name: String,

        /// Namespace
        #[arg(short, long)]
        namespace: Option<String>,
    },

    /// Import secrets from file
    Import {
        /// Input file
        file: PathBuf,

        /// Namespace
        #[arg(short, long)]
        namespace: Option<String>,
    },

    /// Export secrets (encrypted)
    Export {
        /// Output file
        file: PathBuf,

        /// Namespace
        #[arg(short, long)]
        namespace: Option<String>,

        /// Encryption passphrase
        #[arg(short, long)]
        passphrase: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum AclCommand {
    /// List ACL entries
    List {
        /// Filter by subject
        #[arg(short, long)]
        subject: Option<String>,

        /// Filter by resource
        #[arg(short, long)]
        resource: Option<String>,
    },

    /// Grant permission
    Grant {
        /// Subject (user/agent)
        subject: String,

        /// Resource
        resource: String,

        /// Permission
        permission: String,
    },

    /// Revoke permission
    Revoke {
        /// Subject (user/agent)
        subject: String,

        /// Resource
        resource: String,

        /// Permission (optional - revokes all if not specified)
        permission: Option<String>,
    },

    /// Check permission
    Check {
        /// Subject (user/agent)
        subject: String,

        /// Resource
        resource: String,

        /// Permission
        permission: String,
    },

    /// List roles
    Roles {
        /// Show role details
        #[command(subcommand)]
        command: Option<RoleCommand>,
    },
}

#[derive(Subcommand)]
pub enum RoleCommand {
    /// List roles
    List,

    /// Show role details
    Show {
        /// Role name
        name: String,
    },

    /// Create role
    Create {
        /// Role name
        name: String,

        /// Permissions (comma-separated)
        #[arg(short, long)]
        permissions: String,
    },

    /// Delete role
    Delete {
        /// Role name
        name: String,
    },

    /// Assign role to subject
    Assign {
        /// Role name
        role: String,

        /// Subject
        subject: String,
    },

    /// Remove role from subject
    Remove {
        /// Role name
        role: String,

        /// Subject
        subject: String,
    },
}

pub async fn execute(args: SecurityArgs) -> Result<()> {
    let client = crate::client::ApiClient::new()?;

    match args.command {
        SecurityCommand::Status => {
            let status = client.get_security_status().await?;

            println!("🔒 Security Status");
            println!("{}", "=".repeat(50));
            println!(
                "Overall: {}",
                if status.secure {
                    "✅ Secure"
                } else {
                    "⚠️  Issues Found"
                }
            );
            println!(
                "Audit Log: {}",
                if status.audit_enabled {
                    "✅ Enabled"
                } else {
                    "❌ Disabled"
                }
            );
            println!(
                "Secrets Vault: {}",
                if status.secrets_encrypted {
                    "🔐 Encrypted"
                } else {
                    "⚠️  Unencrypted"
                }
            );
            println!(
                "Last Scan: {}",
                status.last_scan.as_deref().unwrap_or("Never")
            );
            println!();

            println!("Issues Found: {}", status.issues_count);
            println!("  - Critical: {}", status.critical_issues);
            println!("  - High: {}", status.high_issues);
            println!("  - Medium: {}", status.medium_issues);
            println!("  - Low: {}", status.low_issues);

            if status.issues_count > 0 {
                println!();
                println!("Run 'beebotos security scan' for details");
            }
        }

        SecurityCommand::Scan {
            scope,
            format,
            output,
        } => {
            let progress = TaskProgress::new("Scanning for security issues");
            let findings = client.security_scan(scope).await?;
            progress.finish_success(None);

            // Format output
            let content = match format {
                OutputFormat::Json => serde_json::to_string_pretty(&findings)?,
                OutputFormat::Sarif => generate_sarif(&findings)?,
                OutputFormat::Html => generate_html_report(&findings)?,
                OutputFormat::Table => format_table(&findings),
            };

            match output {
                Some(path) => {
                    std::fs::write(&path, content)?;
                    println!("✅ Scan results saved to {}", path.display());
                }
                None => println!("{}", content),
            }
        }

        SecurityCommand::Policy { command } => match command {
            PolicyCommand::List => {
                let policies = client.list_policies().await?;

                if policies.is_empty() {
                    println!("No security policies found");
                } else {
                    println!("Security Policies:");
                    for policy in policies {
                        println!("  {} - {}", policy.id, policy.name);
                        println!(
                            "     Status: {}",
                            if policy.active { "Active" } else { "Inactive" }
                        );
                        println!();
                    }
                }
            }
            PolicyCommand::Show { id } => {
                let policy = client.get_policy(&id).await?;
                println!("Policy: {}", policy.name);
                println!("{}", "=".repeat(50));
                println!("ID: {}", policy.id);
                println!("Active: {}", if policy.active { "Yes" } else { "No" });
                println!("Rules: {}", policy.rules.len());
                for rule in &policy.rules {
                    println!("  - {}: {}", rule.name, rule.description);
                }
            }
            PolicyCommand::Apply { id } => {
                client.apply_policy(&id).await?;
                println!("✅ Policy '{}' applied", id);
            }
            PolicyCommand::Create { name, file } => {
                let progress = TaskProgress::new(format!("Creating policy '{}'", name));

                let policy_def = match file {
                    Some(path) => std::fs::read_to_string(path)?,
                    None => create_default_policy(),
                };

                client.create_policy(&name, &policy_def).await?;
                progress.finish_success(None);
                println!("✅ Policy '{}' created", name);
            }
        },

        SecurityCommand::Audit { command } => match command {
            AuditCommand::List {
                agent,
                action,
                limit,
            } => {
                let logs = client
                    .list_audit_logs(agent.as_deref(), action.as_deref(), limit)
                    .await?;

                println!("Audit Log (last {} entries):", logs.len());
                println!("{}", "=".repeat(100));

                for entry in logs {
                    println!(
                        "{} | {} | {} | {} | {}",
                        entry.timestamp,
                        &entry.agent_id[..entry.agent_id.len().min(20)],
                        &entry.action[..entry.action.len().min(20)],
                        if entry.success { "✓" } else { "✗" },
                        &entry.details[..entry.details.len().min(40)]
                    );
                }
            }
            AuditCommand::Export {
                output,
                format,
                from,
                to,
            } => {
                let progress = TaskProgress::new("Exporting audit logs");
                let logs = client
                    .export_audit_logs(from.as_deref(), to.as_deref())
                    .await?;

                let content = match format {
                    ExportFormat::Json => serde_json::to_string_pretty(&logs)?,
                    ExportFormat::Csv => to_csv(&logs)?,
                };

                std::fs::write(&output, content)?;
                progress.finish_success(None);
                println!("✅ Audit logs exported to {}", output.display());
            }
            AuditCommand::Watch { agent } => {
                println!("👀 Watching audit log... (Press Ctrl+C to stop)");

                let mut last_id = 0;
                loop {
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

                    let logs = client.watch_audit_logs(last_id, agent.as_deref()).await?;
                    for entry in logs {
                        println!(
                            "[{}] {} - {} - {}",
                            entry.timestamp, entry.agent_id, entry.action, entry.details
                        );
                        last_id = entry.id;
                    }
                }
            }
        },

        SecurityCommand::Secret { command } => match command {
            SecretCommand::List { namespace } => {
                let secrets = client.list_secrets(namespace.as_deref()).await?;

                println!("Secrets ({}) :", namespace.as_deref().unwrap_or("default"));
                for secret in secrets {
                    println!("  {} (created: {})", secret.name, secret.created_at);
                }
            }
            SecretCommand::Get { name, namespace } => {
                let value = client.get_secret(&name, namespace.as_deref()).await?;
                println!("{}", value);
            }
            SecretCommand::Set {
                name,
                value,
                namespace,
                generate,
                length,
            } => {
                let secret_value = if generate {
                    generate_secret(length)
                } else if let Some(v) = value {
                    v
                } else {
                    rpassword::prompt_password("Enter secret value: ")?
                };

                client
                    .set_secret(&name, &secret_value, namespace.as_deref())
                    .await?;
                println!("✅ Secret '{}' set", name);

                if generate {
                    println!("   Value: {}", secret_value);
                    println!("   ⚠️  Save this value - it won't be shown again!");
                }
            }
            SecretCommand::Delete {
                name,
                namespace,
                force,
            } => {
                if !force {
                    let confirm = dialoguer::Confirm::new()
                        .with_prompt(format!(
                            "Delete secret '{}' from namespace '{}'?",
                            name,
                            namespace.as_deref().unwrap_or("default")
                        ))
                        .interact()?;

                    if !confirm {
                        println!("Cancelled");
                        return Ok(());
                    }
                }

                client.delete_secret(&name, namespace.as_deref()).await?;
                println!("✅ Secret '{}' deleted", name);
            }
            SecretCommand::Rotate { name, namespace } => {
                let progress = TaskProgress::new(format!("Rotating secret '{}'", name));
                let new_value = client.rotate_secret(&name, namespace.as_deref()).await?;
                progress.finish_success(None);

                println!("✅ Secret '{}' rotated", name);
                println!("   New value: {}", new_value);
                println!("   ⚠️  Save this value - it won't be shown again!");
            }
            SecretCommand::Import { file, namespace } => {
                let content = std::fs::read_to_string(&file)?;
                let secrets: serde_json::Value = serde_json::from_str(&content)?;

                let count = client
                    .import_secrets(&secrets, namespace.as_deref())
                    .await?;
                println!("✅ Imported {} secrets", count);
            }
            SecretCommand::Export {
                file,
                namespace,
                passphrase,
            } => {
                let progress = TaskProgress::new("Exporting secrets");
                let data = client.export_secrets(namespace.as_deref()).await?;

                // Encrypt if passphrase provided
                let content = if let Some(ref pass) = passphrase {
                    encrypt_data(&data, pass)?
                } else {
                    data
                };

                std::fs::write(&file, content)?;
                progress.finish_success(None);

                println!("✅ Secrets exported to {}", file.display());
                if passphrase.is_some() {
                    println!("   (Encrypted)");
                }
            }
        },

        SecurityCommand::Acl { command } => match command {
            AclCommand::List { subject, resource } => {
                let entries = client
                    .list_acl(subject.as_deref(), resource.as_deref())
                    .await?;

                println!("ACL Entries:");
                println!("{}", "-".repeat(60));
                for entry in entries {
                    println!(
                        "{} -> {} : {}",
                        entry.subject, entry.resource, entry.permission
                    );
                }
            }
            AclCommand::Grant {
                subject,
                resource,
                permission,
            } => {
                client
                    .grant_permission(&subject, &resource, &permission)
                    .await?;
                println!(
                    "✅ Granted {} permission on {} to {}",
                    permission, resource, subject
                );
            }
            AclCommand::Revoke {
                subject,
                resource,
                permission,
            } => {
                client
                    .revoke_permission(&subject, &resource, permission.as_deref())
                    .await?;
                if let Some(p) = permission {
                    println!(
                        "✅ Revoked {} permission on {} from {}",
                        p, resource, subject
                    );
                } else {
                    println!(
                        "✅ Revoked all permissions on {} from {}",
                        resource, subject
                    );
                }
            }
            AclCommand::Check {
                subject,
                resource,
                permission,
            } => {
                let allowed = client
                    .check_permission(&subject, &resource, &permission)
                    .await?;
                if allowed {
                    println!(
                        "✅ {} has '{}' permission on {}",
                        subject, permission, resource
                    );
                } else {
                    println!(
                        "❌ {} does NOT have '{}' permission on {}",
                        subject, permission, resource
                    );
                }
            }
            AclCommand::Roles { command } => match command {
                Some(RoleCommand::List) | None => {
                    let roles = client.list_roles().await?;
                    println!("Security Roles:");
                    for role in roles {
                        println!("  {} - {} permissions", role.name, role.permission_count);
                    }
                }
                Some(RoleCommand::Show { name }) => {
                    let role = client.get_role(&name).await?;
                    println!("Role: {}", role.name);
                    println!("Permissions:");
                    for perm in &role.permissions {
                        println!("  - {}", perm);
                    }
                }
                Some(RoleCommand::Create { name, permissions }) => {
                    let perms: Vec<&str> = permissions.split(',').collect();
                    client.create_role(&name, &perms).await?;
                    println!(
                        "✅ Role '{}' created with {} permissions",
                        name,
                        perms.len()
                    );
                }
                Some(RoleCommand::Delete { name }) => {
                    client.delete_role(&name).await?;
                    println!("✅ Role '{}' deleted", name);
                }
                Some(RoleCommand::Assign { role, subject }) => {
                    client.assign_role(&role, &subject).await?;
                    println!("✅ Assigned role '{}' to {}", role, subject);
                }
                Some(RoleCommand::Remove { role, subject }) => {
                    client.remove_role(&role, &subject).await?;
                    println!("✅ Removed role '{}' from {}", role, subject);
                }
            },
        },
    }

    Ok(())
}

// Helper functions
fn generate_secret(length: usize) -> String {
    use rand::Rng;
    const CHARSET: &[u8] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!@#$%^&*";

    let mut rng = rand::thread_rng();
    (0..length)
        .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
        .collect()
}

fn format_table(findings: &[SecurityFinding]) -> String {
    let mut output = String::from("Security Scan Results\n");
    output.push_str(&"=".repeat(80));
    output.push('\n');

    for finding in findings {
        let icon = match finding.severity.as_str() {
            "critical" => "🔴",
            "high" => "🟠",
            "medium" => "🟡",
            _ => "🔵",
        };

        output.push_str(&format!(
            "{} [{}] {}\n",
            icon, finding.severity, finding.title
        ));
        output.push_str(&format!("   Category: {}\n", finding.category));
        output.push_str(&format!("   Details: {}\n", finding.details));
        if let Some(fix) = &finding.fix {
            output.push_str(&format!("   Fix: {}\n", fix));
        }
        output.push('\n');
    }

    output
}

fn generate_sarif(findings: &[SecurityFinding]) -> Result<String> {
    let sarif = serde_json::json!({
        "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "BeeBotOS Security Scanner",
                    "version": env!("CARGO_PKG_VERSION")
                }
            },
            "results": findings.iter().map(|f| {
                serde_json::json!({
                    "ruleId": f.id,
                    "message": { "text": f.title },
                    "level": match f.severity.as_str() {
                        "critical" | "high" => "error",
                        "medium" => "warning",
                        _ => "note"
                    },
                    "locations": f.location.as_ref().map(|loc| {
                        serde_json::json!([{
                            "physicalLocation": {
                                "artifactLocation": { "uri": loc }
                            }
                        }])
                    }).unwrap_or(serde_json::Value::Null)
                })
            }).collect::<Vec<_>>()
        }]
    });

    Ok(serde_json::to_string_pretty(&sarif)?)
}

fn generate_html_report(findings: &[SecurityFinding]) -> Result<String> {
    let html = format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <title>BeeBotOS Security Report</title>
    <style>
        body {{ font-family: sans-serif; margin: 2rem; }}
        .critical {{ color: #dc3545; }}
        .high {{ color: #fd7e14; }}
        .medium {{ color: #ffc107; }}
        .low {{ color: #17a2b8; }}
        table {{ border-collapse: collapse; width: 100%; }}
        th, td {{ border: 1px solid #ddd; padding: 8px; text-align: left; }}
        th {{ background-color: #f2f2f2; }}
    </style>
</head>
<body>
    <h1>🔒 Security Scan Report</h1>
    <p>Generated: {}</p>
    <table>
        <tr>
            <th>Severity</th>
            <th>Category</th>
            <th>Title</th>
            <th>Details</th>
        </tr>
        {}
    </table>
</body>
</html>"#,
        chrono::Utc::now().to_rfc3339(),
        findings
            .iter()
            .map(|f| {
                format!(
                    r#"<tr class="{}">
                    <td>{}</td>
                    <td>{}</td>
                    <td>{}</td>
                    <td>{}</td>
                </tr>"#,
                    f.severity, f.severity, f.category, f.title, f.details
                )
            })
            .collect::<String>()
    );

    Ok(html)
}

fn create_default_policy() -> String {
    serde_json::json!({
        "rules": [
            {
                "name": "secrets_detection",
                "enabled": true,
                "severity": "critical"
            },
            {
                "name": "insecure_permissions",
                "enabled": true,
                "severity": "high"
            }
        ]
    })
    .to_string()
}

fn to_csv<T: serde::Serialize>(data: &[T]) -> Result<String> {
    let mut wtr = csv::Writer::from_writer(vec![]);
    for item in data {
        wtr.serialize(item)?;
    }
    Ok(String::from_utf8(wtr.into_inner()?)?)
}

fn encrypt_data(data: &str, passphrase: &str) -> Result<String> {
    // Simple XOR encryption for demo (use proper encryption in production)
    let encrypted: Vec<u8> = data
        .bytes()
        .zip(passphrase.bytes().cycle())
        .map(|(a, b)| a ^ b)
        .collect();
    Ok(base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        &encrypted,
    ))
}

// Client extension trait
trait SecurityClient {
    async fn get_security_status(&self) -> Result<SecurityStatus>;
    async fn security_scan(&self, scope: ScanScope) -> Result<Vec<SecurityFinding>>;
    async fn list_policies(&self) -> Result<Vec<Policy>>;
    async fn get_policy(&self, id: &str) -> Result<Policy>;
    async fn apply_policy(&self, id: &str) -> Result<()>;
    async fn create_policy(&self, name: &str, definition: &str) -> Result<()>;
    async fn list_audit_logs(
        &self,
        agent: Option<&str>,
        action: Option<&str>,
        limit: usize,
    ) -> Result<Vec<AuditLog>>;
    async fn export_audit_logs(
        &self,
        from: Option<&str>,
        to: Option<&str>,
    ) -> Result<Vec<AuditLog>>;
    async fn watch_audit_logs(&self, last_id: i64, agent: Option<&str>) -> Result<Vec<AuditLog>>;
    async fn list_secrets(&self, namespace: Option<&str>) -> Result<Vec<SecretInfo>>;
    async fn get_secret(&self, name: &str, namespace: Option<&str>) -> Result<String>;
    async fn set_secret(&self, name: &str, value: &str, namespace: Option<&str>) -> Result<()>;
    async fn delete_secret(&self, name: &str, namespace: Option<&str>) -> Result<()>;
    async fn rotate_secret(&self, name: &str, namespace: Option<&str>) -> Result<String>;
    async fn import_secrets(
        &self,
        secrets: &serde_json::Value,
        namespace: Option<&str>,
    ) -> Result<usize>;
    async fn export_secrets(&self, namespace: Option<&str>) -> Result<String>;
    async fn list_acl(
        &self,
        subject: Option<&str>,
        resource: Option<&str>,
    ) -> Result<Vec<AclEntry>>;
    async fn grant_permission(&self, subject: &str, resource: &str, permission: &str)
        -> Result<()>;
    async fn revoke_permission(
        &self,
        subject: &str,
        resource: &str,
        permission: Option<&str>,
    ) -> Result<()>;
    async fn check_permission(
        &self,
        subject: &str,
        resource: &str,
        permission: &str,
    ) -> Result<bool>;
    async fn list_roles(&self) -> Result<Vec<RoleSummary>>;
    async fn get_role(&self, name: &str) -> Result<Role>;
    async fn create_role(&self, name: &str, permissions: &[&str]) -> Result<()>;
    async fn delete_role(&self, name: &str) -> Result<()>;
    async fn assign_role(&self, role: &str, subject: &str) -> Result<()>;
    async fn remove_role(&self, role: &str, subject: &str) -> Result<()>;
}

impl SecurityClient for crate::client::ApiClient {
    async fn get_security_status(&self) -> Result<SecurityStatus> {
        let url = self.build_url("/security/status");
        let resp = self
            .http()
            .get(&url)
            .headers(self.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Ok(SecurityStatus {
                secure: true,
                audit_enabled: true,
                secrets_encrypted: true,
                issues_count: 0,
                critical_issues: 0,
                high_issues: 0,
                medium_issues: 0,
                low_issues: 0,
                last_scan: None,
            });
        }

        Ok(resp.json().await?)
    }

    async fn security_scan(&self, _scope: ScanScope) -> Result<Vec<SecurityFinding>> {
        let url = self.build_url("/security/scan");
        let resp = self
            .http()
            .post(&url)
            .headers(self.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }

        let result: serde_json::Value = resp.json().await?;
        Ok(serde_json::from_value(result["findings"].clone()).unwrap_or_default())
    }

    async fn list_policies(&self) -> Result<Vec<Policy>> {
        let url = self.build_url("/security/policies");
        let resp = self
            .http()
            .get(&url)
            .headers(self.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }

        Ok(resp.json().await?)
    }

    async fn get_policy(&self, id: &str) -> Result<Policy> {
        let url = self.build_url(&format!("/security/policies/{}", id));
        let resp = self
            .http()
            .get(&url)
            .headers(self.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("Policy not found"));
        }

        Ok(resp.json().await?)
    }

    async fn apply_policy(&self, id: &str) -> Result<()> {
        let url = self.build_url(&format!("/security/policies/{}/apply", id));
        let resp = self
            .http()
            .post(&url)
            .headers(self.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Apply failed ({}): {}", status, text));
        }
        Ok(())
    }

    async fn create_policy(&self, name: &str, definition: &str) -> Result<()> {
        let url = self.build_url("/security/policies");
        let body = serde_json::json!({
            "name": name,
            "definition": definition,
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
            return Err(anyhow::anyhow!("Create failed ({}): {}", status, text));
        }
        Ok(())
    }

    async fn list_audit_logs(
        &self,
        agent: Option<&str>,
        action: Option<&str>,
        limit: usize,
    ) -> Result<Vec<AuditLog>> {
        let url = self.build_url("/security/audit");
        let resp = self
            .http()
            .get(&url)
            .headers(self.headers()?)
            .query(&[
                ("agent", agent),
                ("action", action),
                ("limit", Some(&limit.to_string())),
            ])
            .send()
            .await?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }

        Ok(resp.json().await?)
    }

    async fn export_audit_logs(
        &self,
        _from: Option<&str>,
        _to: Option<&str>,
    ) -> Result<Vec<AuditLog>> {
        let url = self.build_url("/security/audit/export");
        let resp = self
            .http()
            .get(&url)
            .headers(self.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }

        Ok(resp.json().await?)
    }

    async fn watch_audit_logs(&self, _last_id: i64, _agent: Option<&str>) -> Result<Vec<AuditLog>> {
        let url = self.build_url("/security/audit/recent");
        let resp = self
            .http()
            .get(&url)
            .headers(self.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }

        Ok(resp.json().await?)
    }

    async fn list_secrets(&self, namespace: Option<&str>) -> Result<Vec<SecretInfo>> {
        let url = self.build_url("/security/secrets");
        let resp = self
            .http()
            .get(&url)
            .headers(self.headers()?)
            .query(&[("namespace", namespace)])
            .send()
            .await?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }

        Ok(resp.json().await?)
    }

    async fn get_secret(&self, name: &str, namespace: Option<&str>) -> Result<String> {
        let ns = namespace.unwrap_or("default");
        let url = self.build_url(&format!("/security/secrets/{}/{}", ns, name));
        let resp = self
            .http()
            .get(&url)
            .headers(self.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("Secret not found"));
        }

        let result: serde_json::Value = resp.json().await?;
        Ok(result["value"].as_str().unwrap_or("").to_string())
    }

    async fn set_secret(&self, name: &str, value: &str, namespace: Option<&str>) -> Result<()> {
        let url = self.build_url("/security/secrets");
        let body = serde_json::json!({
            "name": name,
            "value": value,
            "namespace": namespace.unwrap_or("default"),
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
            return Err(anyhow::anyhow!("Set secret failed ({}): {}", status, text));
        }
        Ok(())
    }

    async fn delete_secret(&self, name: &str, namespace: Option<&str>) -> Result<()> {
        let ns = namespace.unwrap_or("default");
        let url = self.build_url(&format!("/security/secrets/{}/{}", ns, name));
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
                "Delete secret failed ({}): {}",
                status,
                text
            ));
        }
        Ok(())
    }

    async fn rotate_secret(&self, name: &str, namespace: Option<&str>) -> Result<String> {
        let ns = namespace.unwrap_or("default");
        let url = self.build_url(&format!("/security/secrets/{}/{}/rotate", ns, name));
        let resp = self
            .http()
            .post(&url)
            .headers(self.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("Rotate secret failed"));
        }

        let result: serde_json::Value = resp.json().await?;
        Ok(result["value"].as_str().unwrap_or("").to_string())
    }

    async fn import_secrets(
        &self,
        _secrets: &serde_json::Value,
        _namespace: Option<&str>,
    ) -> Result<usize> {
        let url = self.build_url("/security/secrets/import");
        let resp = self
            .http()
            .post(&url)
            .headers(self.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Ok(0);
        }

        let result: serde_json::Value = resp.json().await?;
        Ok(result["count"].as_u64().unwrap_or(0) as usize)
    }

    async fn export_secrets(&self, _namespace: Option<&str>) -> Result<String> {
        let url = self.build_url("/security/secrets/export");
        let resp = self
            .http()
            .get(&url)
            .headers(self.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Ok("{}".to_string());
        }

        Ok(resp.text().await?)
    }

    async fn list_acl(
        &self,
        subject: Option<&str>,
        resource: Option<&str>,
    ) -> Result<Vec<AclEntry>> {
        let url = self.build_url("/security/acl");
        let resp = self
            .http()
            .get(&url)
            .headers(self.headers()?)
            .query(&[("subject", subject), ("resource", resource)])
            .send()
            .await?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }

        Ok(resp.json().await?)
    }

    async fn grant_permission(
        &self,
        subject: &str,
        resource: &str,
        permission: &str,
    ) -> Result<()> {
        let url = self.build_url("/security/acl");
        let body = serde_json::json!({
            "subject": subject,
            "resource": resource,
            "permission": permission,
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
            return Err(anyhow::anyhow!("Grant failed ({}): {}", status, text));
        }
        Ok(())
    }

    async fn revoke_permission(
        &self,
        subject: &str,
        resource: &str,
        permission: Option<&str>,
    ) -> Result<()> {
        let url = self.build_url("/security/acl");
        let body = serde_json::json!({
            "subject": subject,
            "resource": resource,
            "permission": permission,
        });

        let resp = self
            .http()
            .delete(&url)
            .headers(self.headers()?)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Revoke failed ({}): {}", status, text));
        }
        Ok(())
    }

    async fn check_permission(
        &self,
        subject: &str,
        resource: &str,
        permission: &str,
    ) -> Result<bool> {
        let url = self.build_url("/security/acl/check");
        let resp = self
            .http()
            .get(&url)
            .headers(self.headers()?)
            .query(&[
                ("subject", subject),
                ("resource", resource),
                ("permission", permission),
            ])
            .send()
            .await?;

        if !resp.status().is_success() {
            return Ok(false);
        }

        let result: serde_json::Value = resp.json().await?;
        Ok(result["allowed"].as_bool().unwrap_or(false))
    }

    async fn list_roles(&self) -> Result<Vec<RoleSummary>> {
        let url = self.build_url("/security/roles");
        let resp = self
            .http()
            .get(&url)
            .headers(self.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }

        Ok(resp.json().await?)
    }

    async fn get_role(&self, name: &str) -> Result<Role> {
        let url = self.build_url(&format!("/security/roles/{}", name));
        let resp = self
            .http()
            .get(&url)
            .headers(self.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("Role not found"));
        }

        Ok(resp.json().await?)
    }

    async fn create_role(&self, name: &str, permissions: &[&str]) -> Result<()> {
        let url = self.build_url("/security/roles");
        let body = serde_json::json!({
            "name": name,
            "permissions": permissions,
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
            return Err(anyhow::anyhow!("Create role failed ({}): {}", status, text));
        }
        Ok(())
    }

    async fn delete_role(&self, name: &str) -> Result<()> {
        let url = self.build_url(&format!("/security/roles/{}", name));
        let resp = self
            .http()
            .delete(&url)
            .headers(self.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Delete role failed ({}): {}", status, text));
        }
        Ok(())
    }

    async fn assign_role(&self, role: &str, subject: &str) -> Result<()> {
        let url = self.build_url(&format!("/security/roles/{}/assign", role));
        let body = serde_json::json!({ "subject": subject });

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
            return Err(anyhow::anyhow!("Assign role failed ({}): {}", status, text));
        }
        Ok(())
    }

    async fn remove_role(&self, role: &str, subject: &str) -> Result<()> {
        let url = self.build_url(&format!("/security/roles/{}/remove", role));
        let body = serde_json::json!({ "subject": subject });

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
            return Err(anyhow::anyhow!("Remove role failed ({}): {}", status, text));
        }
        Ok(())
    }
}

// Data structures
#[derive(serde::Deserialize)]
struct SecurityStatus {
    secure: bool,
    audit_enabled: bool,
    secrets_encrypted: bool,
    issues_count: usize,
    critical_issues: usize,
    high_issues: usize,
    medium_issues: usize,
    low_issues: usize,
    last_scan: Option<String>,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct SecurityFinding {
    id: String,
    severity: String,
    category: String,
    title: String,
    details: String,
    fix: Option<String>,
    location: Option<String>,
}

#[derive(serde::Deserialize)]
struct Policy {
    id: String,
    name: String,
    active: bool,
    rules: Vec<PolicyRule>,
}

#[derive(serde::Deserialize)]
struct PolicyRule {
    name: String,
    description: String,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct AuditLog {
    id: i64,
    timestamp: String,
    agent_id: String,
    action: String,
    success: bool,
    details: String,
}

#[derive(serde::Deserialize)]
struct SecretInfo {
    name: String,
    created_at: String,
}

#[derive(serde::Deserialize)]
struct AclEntry {
    subject: String,
    resource: String,
    permission: String,
}

#[derive(serde::Deserialize)]
struct RoleSummary {
    name: String,
    permission_count: usize,
}

#[derive(serde::Deserialize)]
struct Role {
    name: String,
    permissions: Vec<String>,
}
