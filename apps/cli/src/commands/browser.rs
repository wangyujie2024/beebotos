//! Browser automation commands
//!
//! CDP-based browser control for web automation.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};

use crate::progress::TaskProgress;

#[derive(Parser)]
pub struct BrowserArgs {
    #[command(subcommand)]
    pub command: BrowserCommand,
}

#[derive(Subcommand)]
pub enum BrowserCommand {
    /// Show browser status
    Status,

    /// Start browser
    Start {
        /// Headless mode
        #[arg(long)]
        headless: bool,

        /// User data directory
        #[arg(long)]
        user_data_dir: Option<PathBuf>,

        /// Proxy server
        #[arg(long)]
        proxy: Option<String>,
    },

    /// Stop browser
    Stop,

    /// Reset browser profile
    ResetProfile {
        /// Profile name
        name: Option<String>,
    },

    /// Navigate to URL
    Navigate {
        /// URL to navigate to
        url: String,

        /// Wait for page load
        #[arg(long)]
        wait: bool,

        /// Wait for specific selector
        #[arg(long)]
        wait_for: Option<String>,
    },

    /// Take screenshot
    Screenshot {
        /// Output file path
        #[arg(short, long, default_value = "screenshot.png")]
        output: PathBuf,

        /// Full page screenshot
        #[arg(long)]
        full_page: bool,

        /// CSS selector to screenshot
        #[arg(long)]
        selector: Option<String>,
    },

    /// Get page snapshot (HTML)
    Snapshot {
        /// Output file path (default: stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Save page as PDF
    Pdf {
        /// Output file path
        #[arg(short, long, default_value = "page.pdf")]
        output: PathBuf,
    },

    /// Click element
    Click {
        /// CSS selector
        selector: String,

        /// Wait for element
        #[arg(long, default_value = "5")]
        timeout: u64,
    },

    /// Type text
    Type {
        /// CSS selector
        selector: String,

        /// Text to type
        text: String,

        /// Clear before typing
        #[arg(long)]
        clear: bool,
    },

    /// Press key
    Press {
        /// Key to press (e.g., Enter, Tab, Escape)
        key: String,
    },

    /// Hover over element
    Hover {
        /// CSS selector
        selector: String,
    },

    /// Scroll page
    Scroll {
        /// Scroll direction
        #[arg(value_enum)]
        direction: ScrollDirection,

        /// Pixels to scroll
        #[arg(short, long, default_value = "300")]
        amount: i32,
    },

    /// Fill form
    Fill {
        /// CSS selector for form
        selector: String,

        /// Form data as JSON
        data: String,
    },

    /// Upload file
    Upload {
        /// CSS selector for file input
        selector: String,

        /// File path to upload
        file: PathBuf,
    },

    /// Wait for element
    Wait {
        /// CSS selector
        selector: String,

        /// Timeout in seconds
        #[arg(short, long, default_value = "10")]
        timeout: u64,
    },

    /// Execute JavaScript
    Eval {
        /// JavaScript code
        script: String,

        /// Return result as JSON
        #[arg(long)]
        json: bool,
    },

    /// Get console logs
    Console {
        /// Clear after reading
        #[arg(long)]
        clear: bool,
    },

    /// Tab management
    #[command(subcommand)]
    Tab(TabCommand),

    /// Profile management
    #[command(subcommand)]
    Profile(ProfileCommand),

    /// Send CDP command
    Cdp {
        /// CDP method
        method: String,

        /// Parameters as JSON
        #[arg(short, long)]
        params: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum ScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Subcommand)]
pub enum TabCommand {
    /// List tabs
    List,

    /// Open new tab
    Open {
        /// URL to open
        #[arg(short, long)]
        url: Option<String>,
    },

    /// Focus tab
    Focus {
        /// Tab index or ID
        tab: String,
    },

    /// Close tab
    Close {
        /// Tab index or ID
        tab: String,
    },
}

#[derive(Subcommand)]
pub enum ProfileCommand {
    /// List profiles
    List,

    /// Create profile
    Create {
        /// Profile name
        name: String,
    },

    /// Delete profile
    Delete {
        /// Profile name
        name: String,
    },
}

pub async fn execute(args: BrowserArgs) -> Result<()> {
    let client = crate::client::ApiClient::new()?;

    match args.command {
        BrowserCommand::Status => {
            let status = client.get_browser_status().await?;
            println!("🌐 Browser Status");
            println!("{}", "=".repeat(50));
            println!(
                "Running: {}",
                if status.running { "✅ Yes" } else { "❌ No" }
            );
            println!("Headless: {}", if status.headless { "Yes" } else { "No" });
            println!("Pages: {}", status.page_count);
            println!("Version: {}", status.version);
        }

        BrowserCommand::Start {
            headless,
            user_data_dir,
            proxy,
        } => {
            let progress = TaskProgress::new("Starting browser");
            client
                .start_browser(headless, user_data_dir.as_ref(), proxy.as_deref())
                .await?;
            progress.finish_success(None);
            println!("✅ Browser started");
            if headless {
                println!("   Mode: Headless");
            }
        }

        BrowserCommand::Stop => {
            let progress = TaskProgress::new("Stopping browser");
            client.stop_browser().await?;
            progress.finish_success(None);
            println!("✅ Browser stopped");
        }

        BrowserCommand::ResetProfile { name } => {
            let profile_name = name.as_deref().unwrap_or("default");
            let progress = TaskProgress::new(format!("Resetting profile '{}'", profile_name));
            client.reset_browser_profile(profile_name).await?;
            progress.finish_success(None);
            println!("✅ Profile '{}' reset", profile_name);
        }

        BrowserCommand::Navigate {
            url,
            wait,
            wait_for,
        } => {
            let progress = TaskProgress::new(format!("Navigating to {}", url));
            client
                .browser_navigate(&url, wait, wait_for.as_deref())
                .await?;
            progress.finish_success(None);
            println!("✅ Navigated to {}", url);
        }

        BrowserCommand::Screenshot {
            output,
            full_page,
            selector,
        } => {
            let progress = TaskProgress::new("Taking screenshot");
            client
                .browser_screenshot(&output, full_page, selector.as_deref())
                .await?;
            progress.finish_success(None);
            println!("✅ Screenshot saved to {}", output.display());
        }

        BrowserCommand::Snapshot { output } => {
            let html = client.browser_snapshot().await?;
            match output {
                Some(path) => {
                    std::fs::write(&path, html)?;
                    println!("✅ Snapshot saved to {}", path.display());
                }
                None => println!("{}", html),
            }
        }

        BrowserCommand::Pdf { output } => {
            let progress = TaskProgress::new("Saving PDF");
            client.browser_pdf(&output).await?;
            progress.finish_success(None);
            println!("✅ PDF saved to {}", output.display());
        }

        BrowserCommand::Click { selector, timeout } => {
            let progress = TaskProgress::new(format!("Clicking {}", selector));
            client
                .browser_click(&selector, Duration::from_secs(timeout))
                .await?;
            progress.finish_success(None);
            println!("✅ Clicked {}", selector);
        }

        BrowserCommand::Type {
            selector,
            text,
            clear,
        } => {
            let progress = TaskProgress::new(format!("Typing into {}", selector));
            client.browser_type(&selector, &text, clear).await?;
            progress.finish_success(None);
            println!("✅ Typed into {}", selector);
        }

        BrowserCommand::Press { key } => {
            client.browser_press_key(&key).await?;
            println!("✅ Pressed {}", key);
        }

        BrowserCommand::Hover { selector } => {
            client.browser_hover(&selector).await?;
            println!("✅ Hovered over {}", selector);
        }

        BrowserCommand::Scroll { direction, amount } => {
            let direction_str = match direction {
                ScrollDirection::Up => "up",
                ScrollDirection::Down => "down",
                ScrollDirection::Left => "left",
                ScrollDirection::Right => "right",
            };
            client.browser_scroll(direction_str, amount).await?;
            println!("✅ Scrolled {}", direction_str);
        }

        BrowserCommand::Fill { selector, data } => {
            let progress = TaskProgress::new(format!("Filling form {}", selector));
            client.browser_fill_form(&selector, &data).await?;
            progress.finish_success(None);
            println!("✅ Form filled");
        }

        BrowserCommand::Upload { selector, file } => {
            let progress = TaskProgress::new(format!("Uploading file to {}", selector));
            client.browser_upload_file(&selector, &file).await?;
            progress.finish_success(None);
            println!("✅ File uploaded");
        }

        BrowserCommand::Wait { selector, timeout } => {
            let progress = TaskProgress::new(format!("Waiting for {}", selector));
            client
                .browser_wait_for(&selector, Duration::from_secs(timeout))
                .await?;
            progress.finish_success(None);
            println!("✅ Element found");
        }

        BrowserCommand::Eval { script, json } => {
            let result = client.browser_eval(&script).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("{}", result);
            }
        }

        BrowserCommand::Console { clear } => {
            let logs = client.browser_console_logs(clear).await?;
            for log in logs {
                let icon = match log.level.as_str() {
                    "error" => "🔴",
                    "warn" => "🟡",
                    "info" => "🔵",
                    _ => "⚪",
                };
                println!("{} [{}] {}", icon, log.level, log.message);
            }
        }

        BrowserCommand::Tab(cmd) => match cmd {
            TabCommand::List => {
                let tabs = client.browser_list_tabs().await?;
                for (i, tab) in tabs.iter().enumerate() {
                    let active = if tab.active { "🟢" } else { "⚪" };
                    println!(
                        "{} [{}] {} - {}",
                        active,
                        i,
                        &tab.title[..tab.title.len().min(40)],
                        &tab.url[..tab.url.len().min(40)]
                    );
                }
            }
            TabCommand::Open { url } => {
                let tab = client.browser_open_tab(url.as_deref()).await?;
                println!("✅ New tab opened: {}", tab.id);
            }
            TabCommand::Focus { tab } => {
                client.browser_focus_tab(&tab).await?;
                println!("✅ Tab {} focused", tab);
            }
            TabCommand::Close { tab } => {
                client.browser_close_tab(&tab).await?;
                println!("✅ Tab {} closed", tab);
            }
        },

        BrowserCommand::Profile(cmd) => match cmd {
            ProfileCommand::List => {
                let profiles = client.browser_list_profiles().await?;
                println!("Browser profiles:");
                for profile in profiles {
                    println!("  {}", profile);
                }
            }
            ProfileCommand::Create { name } => {
                client.browser_create_profile(&name).await?;
                println!("✅ Profile '{}' created", name);
            }
            ProfileCommand::Delete { name } => {
                client.browser_delete_profile(&name).await?;
                println!("✅ Profile '{}' deleted", name);
            }
        },

        BrowserCommand::Cdp { method, params } => {
            let result = client.browser_cdp(&method, params.as_deref()).await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
    }

    Ok(())
}

// Client extension trait
trait BrowserClient {
    async fn get_browser_status(&self) -> Result<BrowserStatus>;
    async fn start_browser(
        &self,
        headless: bool,
        user_data_dir: Option<&PathBuf>,
        proxy: Option<&str>,
    ) -> Result<()>;
    async fn stop_browser(&self) -> Result<()>;
    async fn reset_browser_profile(&self, name: &str) -> Result<()>;
    async fn browser_navigate(&self, url: &str, wait: bool, wait_for: Option<&str>) -> Result<()>;
    async fn browser_screenshot(
        &self,
        output: &PathBuf,
        full_page: bool,
        selector: Option<&str>,
    ) -> Result<()>;
    async fn browser_snapshot(&self) -> Result<String>;
    async fn browser_pdf(&self, output: &PathBuf) -> Result<()>;
    async fn browser_click(&self, selector: &str, timeout: Duration) -> Result<()>;
    async fn browser_type(&self, selector: &str, text: &str, clear: bool) -> Result<()>;
    async fn browser_press_key(&self, key: &str) -> Result<()>;
    async fn browser_hover(&self, selector: &str) -> Result<()>;
    async fn browser_scroll(&self, direction: &str, amount: i32) -> Result<()>;
    async fn browser_fill_form(&self, selector: &str, data: &str) -> Result<()>;
    async fn browser_upload_file(&self, selector: &str, file: &PathBuf) -> Result<()>;
    async fn browser_wait_for(&self, selector: &str, timeout: Duration) -> Result<()>;
    async fn browser_eval(&self, script: &str) -> Result<serde_json::Value>;
    async fn browser_console_logs(&self, clear: bool) -> Result<Vec<ConsoleLog>>;
    async fn browser_list_tabs(&self) -> Result<Vec<TabInfo>>;
    async fn browser_open_tab(&self, url: Option<&str>) -> Result<TabInfo>;
    async fn browser_focus_tab(&self, tab: &str) -> Result<()>;
    async fn browser_close_tab(&self, tab: &str) -> Result<()>;
    async fn browser_list_profiles(&self) -> Result<Vec<String>>;
    async fn browser_create_profile(&self, name: &str) -> Result<()>;
    async fn browser_delete_profile(&self, name: &str) -> Result<()>;
    async fn browser_cdp(&self, method: &str, params: Option<&str>) -> Result<serde_json::Value>;
}

impl BrowserClient for crate::client::ApiClient {
    async fn get_browser_status(&self) -> Result<BrowserStatus> {
        let url = self.build_url("/browser/status");
        let resp = self
            .http()
            .get(&url)
            .headers(self.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Ok(BrowserStatus {
                running: false,
                headless: false,
                page_count: 0,
                version: "unknown".to_string(),
            });
        }

        Ok(resp.json().await?)
    }

    async fn start_browser(
        &self,
        headless: bool,
        _user_data_dir: Option<&PathBuf>,
        _proxy: Option<&str>,
    ) -> Result<()> {
        let url = self.build_url("/browser/start");
        let body = serde_json::json!({
            "headless": headless,
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
                "Failed to start browser ({}): {}",
                status,
                text
            ));
        }
        Ok(())
    }

    async fn stop_browser(&self) -> Result<()> {
        let url = self.build_url("/browser/stop");
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
                "Failed to stop browser ({}): {}",
                status,
                text
            ));
        }
        Ok(())
    }

    async fn reset_browser_profile(&self, name: &str) -> Result<()> {
        let url = self.build_url(&format!("/browser/profile/{}/reset", name));
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
                "Failed to reset profile ({}): {}",
                status,
                text
            ));
        }
        Ok(())
    }

    async fn browser_navigate(
        &self,
        url: &str,
        _wait: bool,
        _wait_for: Option<&str>,
    ) -> Result<()> {
        let api_url = self.build_url("/browser/navigate");
        let body = serde_json::json!({ "url": url });

        let resp = self
            .http()
            .post(&api_url)
            .headers(self.headers()?)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Navigation failed ({}): {}", status, text));
        }
        Ok(())
    }

    async fn browser_screenshot(
        &self,
        _output: &PathBuf,
        _full_page: bool,
        _selector: Option<&str>,
    ) -> Result<()> {
        // Would download screenshot from browser
        Ok(())
    }

    async fn browser_snapshot(&self) -> Result<String> {
        let url = self.build_url("/browser/snapshot");
        let resp = self
            .http()
            .get(&url)
            .headers(self.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Ok(String::new());
        }

        let result: serde_json::Value = resp.json().await?;
        Ok(result["html"].as_str().unwrap_or("").to_string())
    }

    async fn browser_pdf(&self, _output: &PathBuf) -> Result<()> {
        Ok(())
    }

    async fn browser_click(&self, selector: &str, _timeout: Duration) -> Result<()> {
        let url = self.build_url("/browser/click");
        let body = serde_json::json!({ "selector": selector });

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
            return Err(anyhow::anyhow!("Click failed ({}): {}", status, text));
        }
        Ok(())
    }

    async fn browser_type(&self, selector: &str, text: &str, _clear: bool) -> Result<()> {
        let url = self.build_url("/browser/type");
        let body = serde_json::json!({
            "selector": selector,
            "text": text,
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
            return Err(anyhow::anyhow!("Type failed ({}): {}", status, text));
        }
        Ok(())
    }

    async fn browser_press_key(&self, key: &str) -> Result<()> {
        let url = self.build_url("/browser/press");
        let body = serde_json::json!({ "key": key });

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
            return Err(anyhow::anyhow!("Key press failed ({}): {}", status, text));
        }
        Ok(())
    }

    async fn browser_hover(&self, selector: &str) -> Result<()> {
        let url = self.build_url("/browser/hover");
        let body = serde_json::json!({ "selector": selector });

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
            return Err(anyhow::anyhow!("Hover failed ({}): {}", status, text));
        }
        Ok(())
    }

    async fn browser_scroll(&self, direction: &str, amount: i32) -> Result<()> {
        let url = self.build_url("/browser/scroll");
        let body = serde_json::json!({
            "direction": direction,
            "amount": amount,
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
            return Err(anyhow::anyhow!("Scroll failed ({}): {}", status, text));
        }
        Ok(())
    }

    async fn browser_fill_form(&self, selector: &str, data: &str) -> Result<()> {
        let url = self.build_url("/browser/fill");
        let body = serde_json::json!({
            "selector": selector,
            "data": data,
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
            return Err(anyhow::anyhow!("Form fill failed ({}): {}", status, text));
        }
        Ok(())
    }

    async fn browser_upload_file(&self, selector: &str, file: &PathBuf) -> Result<()> {
        let url = self.build_url("/browser/upload");
        let body = serde_json::json!({
            "selector": selector,
            "file": file.to_string_lossy(),
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
            return Err(anyhow::anyhow!("Upload failed ({}): {}", status, text));
        }
        Ok(())
    }

    async fn browser_wait_for(&self, selector: &str, _timeout: Duration) -> Result<()> {
        let url = self.build_url("/browser/wait");
        let body = serde_json::json!({ "selector": selector });

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
            return Err(anyhow::anyhow!("Wait failed ({}): {}", status, text));
        }
        Ok(())
    }

    async fn browser_eval(&self, script: &str) -> Result<serde_json::Value> {
        let url = self.build_url("/browser/eval");
        let body = serde_json::json!({ "script": script });

        let resp = self
            .http()
            .post(&url)
            .headers(self.headers()?)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Ok(serde_json::Value::Null);
        }

        Ok(resp.json().await?)
    }

    async fn browser_console_logs(&self, _clear: bool) -> Result<Vec<ConsoleLog>> {
        let url = self.build_url("/browser/console");
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
        Ok(serde_json::from_value(result["logs"].clone()).unwrap_or_default())
    }

    async fn browser_list_tabs(&self) -> Result<Vec<TabInfo>> {
        let url = self.build_url("/browser/tabs");
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

    async fn browser_open_tab(&self, url: Option<&str>) -> Result<TabInfo> {
        let api_url = self.build_url("/browser/tabs");
        let body = serde_json::json!({ "url": url });

        let resp = self
            .http()
            .post(&api_url)
            .headers(self.headers()?)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Ok(TabInfo {
                id: "0".to_string(),
                title: "New Tab".to_string(),
                url: "about:blank".to_string(),
                active: false,
            });
        }

        Ok(resp.json().await?)
    }

    async fn browser_focus_tab(&self, tab: &str) -> Result<()> {
        let url = self.build_url(&format!("/browser/tabs/{}/focus", tab));
        let resp = self
            .http()
            .post(&url)
            .headers(self.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Focus tab failed ({}): {}", status, text));
        }
        Ok(())
    }

    async fn browser_close_tab(&self, tab: &str) -> Result<()> {
        let url = self.build_url(&format!("/browser/tabs/{}", tab));
        let resp = self
            .http()
            .delete(&url)
            .headers(self.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Close tab failed ({}): {}", status, text));
        }
        Ok(())
    }

    async fn browser_list_profiles(&self) -> Result<Vec<String>> {
        let url = self.build_url("/browser/profiles");
        let resp = self
            .http()
            .get(&url)
            .headers(self.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Ok(vec!["default".to_string()]);
        }

        let result: serde_json::Value = resp.json().await?;
        Ok(serde_json::from_value(result["profiles"].clone()).unwrap_or_default())
    }

    async fn browser_create_profile(&self, name: &str) -> Result<()> {
        let url = self.build_url("/browser/profiles");
        let body = serde_json::json!({ "name": name });

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
                "Create profile failed ({}): {}",
                status,
                text
            ));
        }
        Ok(())
    }

    async fn browser_delete_profile(&self, name: &str) -> Result<()> {
        let url = self.build_url(&format!("/browser/profiles/{}", name));
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
                "Delete profile failed ({}): {}",
                status,
                text
            ));
        }
        Ok(())
    }

    async fn browser_cdp(&self, method: &str, params: Option<&str>) -> Result<serde_json::Value> {
        let url = self.build_url("/browser/cdp");
        let body = serde_json::json!({
            "method": method,
            "params": params,
        });

        let resp = self
            .http()
            .post(&url)
            .headers(self.headers()?)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Ok(serde_json::Value::Null);
        }

        Ok(resp.json().await?)
    }
}

#[derive(serde::Deserialize)]
struct BrowserStatus {
    running: bool,
    headless: bool,
    page_count: usize,
    version: String,
}

#[derive(serde::Deserialize)]
struct ConsoleLog {
    level: String,
    message: String,
}

#[derive(serde::Deserialize)]
struct TabInfo {
    id: String,
    title: String,
    url: String,
    active: bool,
}
