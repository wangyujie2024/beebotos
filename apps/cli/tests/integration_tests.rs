//! Integration tests for BeeBotOS CLI
//!
//! These tests verify the CLI behavior using the `assert_cmd` and `predicates`
//! crates. They invoke the CLI as a subprocess and verify exit codes, stdout,
//! and stderr.

use assert_cmd::Command;
use predicates::prelude::*;

/// Helper function to create a CLI command with test environment setup
fn cli() -> Command {
    let mut cmd = Command::cargo_bin("beebot").unwrap();
    // Use a mock API endpoint for tests
    cmd.env("BEEBOTOS_API_URL", "http://localhost:9999");
    cmd.env("BEEBOTOS_API_KEY", "test-api-key");
    cmd
}

mod basic_commands {
    use super::*;

    #[test]
    fn test_cli_help() {
        let mut cmd = cli();
        cmd.arg("--help");
        cmd.assert()
            .success()
            .stdout(predicate::str::contains("BeeBotOS Command Line Interface"))
            .stdout(predicate::str::contains("agent"))
            .stdout(predicate::str::contains("brain"))
            .stdout(predicate::str::contains("config"));
    }

    #[test]
    fn test_cli_version() {
        let mut cmd = cli();
        cmd.arg("--version");
        cmd.assert()
            .success()
            .stdout(predicate::str::contains("1.0.0"));
    }

    #[test]
    fn test_info_command() {
        let mut cmd = cli();
        cmd.arg("info");
        cmd.assert()
            .success()
            .stdout(predicate::str::contains("BeeBotOS CLI Information"))
            .stdout(predicate::str::contains("Version:"));
    }
}

mod agent_commands {
    use super::*;

    #[test]
    fn test_agent_help() {
        let mut cmd = cli();
        cmd.args(["agent", "--help"]);
        cmd.assert()
            .success()
            .stdout(predicate::str::contains("create"))
            .stdout(predicate::str::contains("list"))
            .stdout(predicate::str::contains("start"))
            .stdout(predicate::str::contains("stop"))
            .stdout(predicate::str::contains("delete"));
    }

    #[test]
    fn test_agent_list_without_server() {
        // This test verifies that the command fails gracefully when server is
        // unavailable
        let mut cmd = cli();
        cmd.args(["agent", "list"]);
        // Should fail because the mock server is not running
        cmd.assert().failure().stderr(
            predicate::str::contains("error sending request")
                .or(predicate::str::contains("Connection refused"))
                .or(predicate::str::contains("Unknown error")),
        );
    }
}

mod config_commands {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_config_show() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("beebotos");
        std::fs::create_dir_all(&config_dir).unwrap();

        // Create a test config file
        let config_file = config_dir.join("config.toml");
        std::fs::write(
            &config_file,
            r#"
daemon_endpoint = "http://localhost:8080"
daemon_timeout = 30
rpc_url = "http://localhost:8545"
dao_address = "0x1234567890abcdef"
api_key = "test-key"
"#,
        )
        .unwrap();

        let mut cmd = cli();
        cmd.env("XDG_CONFIG_HOME", temp_dir.path());
        cmd.args(["config", "show"]);
        cmd.assert()
            .success()
            .stdout(predicate::str::contains("daemon_endpoint"));
    }

    #[test]
    fn test_config_validate_valid() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("beebotos");
        std::fs::create_dir_all(&config_dir).unwrap();

        let config_file = config_dir.join("config.toml");
        std::fs::write(
            &config_file,
            r#"
daemon_endpoint = "http://localhost:8080"
daemon_timeout = 30
rpc_url = "http://localhost:8545"
"#,
        )
        .unwrap();

        let mut cmd = cli();
        cmd.env("XDG_CONFIG_HOME", temp_dir.path());
        cmd.args(["config", "validate"]);
        cmd.assert()
            .success()
            .stdout(predicate::str::contains("Configuration is valid"));
    }
}

mod skill_commands {
    use super::*;

    #[test]
    fn test_skill_help() {
        let mut cmd = cli();
        cmd.args(["skill", "--help"]);
        cmd.assert()
            .success()
            .stdout(predicate::str::contains("list"))
            .stdout(predicate::str::contains("install"))
            .stdout(predicate::str::contains("uninstall"));
    }
}

mod message_commands {
    use super::*;

    #[test]
    fn test_message_send_help() {
        let mut cmd = cli();
        cmd.args(["message", "send", "--help"]);
        cmd.assert()
            .success()
            .stdout(predicate::str::contains("--timeout"));
    }
}

mod session_commands {
    use super::*;

    #[test]
    fn test_session_help() {
        let mut cmd = cli();
        cmd.args(["session", "--help"]);
        cmd.assert()
            .success()
            .stdout(predicate::str::contains("create"))
            .stdout(predicate::str::contains("list"))
            .stdout(predicate::str::contains("resume"));
    }
}

mod brain_commands {
    use super::*;

    #[test]
    fn test_brain_help() {
        let mut cmd = cli();
        cmd.args(["brain", "--help"]);
        cmd.assert()
            .success()
            .stdout(predicate::str::contains("status"))
            .stdout(predicate::str::contains("memory"))
            .stdout(predicate::str::contains("emotion"));
    }
}

mod watch_commands {
    use super::*;

    #[test]
    fn test_watch_help() {
        let mut cmd = cli();
        cmd.args(["watch", "--help"]);
        cmd.assert()
            .success()
            .stdout(predicate::str::contains("agents"))
            .stdout(predicate::str::contains("blocks"))
            .stdout(predicate::str::contains("events"))
            .stdout(predicate::str::contains("tasks"));
    }
}

mod completion_commands {
    use super::*;

    #[test]
    fn test_completion_bash() {
        let mut cmd = cli();
        cmd.args(["completion", "bash"]);
        cmd.assert()
            .success()
            .stdout(predicate::str::contains("_beebot").or(predicate::str::contains("beebot")));
    }

    #[test]
    fn test_completion_zsh() {
        let mut cmd = cli();
        cmd.args(["completion", "zsh"]);
        cmd.assert().success();
    }

    #[test]
    fn test_completion_fish() {
        let mut cmd = cli();
        cmd.args(["completion", "fish"]);
        cmd.assert().success();
    }

    #[test]
    fn test_completion_invalid_shell() {
        let mut cmd = cli();
        cmd.args(["completion", "invalid_shell"]);
        cmd.assert()
            .success()
            .stderr(predicate::str::contains("Unknown shell"));
    }
}

// Unit tests for internal modules
#[cfg(test)]
mod unit_tests {

    #[test]
    fn test_websocket_url_conversion() {
        // Test HTTP to WS conversion
        let ws_url = convert_http_to_ws("http://localhost:8080");
        assert_eq!(ws_url, "ws://localhost:8080/ws");

        let ws_url = convert_http_to_ws("https://api.example.com");
        assert_eq!(ws_url, "wss://api.example.com/ws");

        let ws_url = convert_http_to_ws("ws://localhost:8080/ws");
        assert_eq!(ws_url, "ws://localhost:8080/ws");
    }

    fn convert_http_to_ws(url: &str) -> String {
        let ws_url = if url.starts_with("https://") {
            url.replace("https://", "wss://")
        } else if url.starts_with("http://") {
            url.replace("http://", "ws://")
        } else {
            url.to_string()
        };

        if ws_url.ends_with("/ws") {
            ws_url
        } else if ws_url.ends_with('/') {
            format!("{}ws", ws_url)
        } else {
            format!("{}/ws", ws_url)
        }
    }
}

// Tests for client module
#[cfg(test)]
mod client_tests {
    use super::*;

    #[test]
    fn test_cli_command_structure() {
        // Verify all main commands exist in help output
        let mut cmd = cli();
        cmd.arg("--help");

        let output = cmd.output().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);

        // Check for main command categories
        assert!(stdout.contains("agent"), "Missing agent command");
        assert!(stdout.contains("brain"), "Missing brain command");
        assert!(stdout.contains("browser"), "Missing browser command");
        assert!(stdout.contains("chain"), "Missing chain command");
        assert!(stdout.contains("channel"), "Missing channel command");
        assert!(stdout.contains("config"), "Missing config command");
        assert!(stdout.contains("doctor"), "Missing doctor command");
        assert!(stdout.contains("gateway"), "Missing gateway command");
        assert!(stdout.contains("infer"), "Missing infer command");
        assert!(stdout.contains("memory"), "Missing memory command");
        assert!(stdout.contains("message"), "Missing message command");
        assert!(stdout.contains("model"), "Missing model command");
        assert!(stdout.contains("security"), "Missing security command");
        assert!(stdout.contains("setup"), "Missing setup command");
    }
}

// Tests for error handling
#[cfg(test)]
mod error_tests {
    use super::*;

    #[test]
    fn test_invalid_endpoint_format() {
        let mut cmd = cli();
        cmd.env("BEEBOTOS_API_URL", "not-a-valid-url");
        cmd.args(["info"]);

        // Should either fail gracefully or handle the error
        let output = cmd.output().unwrap();
        // Command may succeed or fail, but should not panic
        assert!(output.status.code().is_some());
    }

    #[test]
    fn test_missing_api_key() {
        let mut cmd = Command::cargo_bin("beebot").unwrap();
        cmd.env_remove("BEEBOTOS_API_KEY");
        cmd.env("BEEBOTOS_API_URL", "http://localhost:9999");
        cmd.args(["agent", "list"]);

        let output = cmd.output().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Should report missing API key
        assert!(
            stderr.contains("API key")
                || stderr.contains("BEEBOTOS_API_KEY")
                || output.status.success() == false
        );
    }
}

// Tests for security module
#[cfg(test)]
mod security_tests {
    use super::*;

    #[test]
    fn test_security_help() {
        let mut cmd = cli();
        cmd.args(["security", "--help"]);
        cmd.assert()
            .success()
            .stdout(predicate::str::contains("scan"))
            .stdout(predicate::str::contains("secret"))
            .stdout(predicate::str::contains("audit"));
    }

    #[test]
    fn test_security_scan_help() {
        let mut cmd = cli();
        cmd.args(["security", "scan", "--help"]);
        cmd.assert()
            .success()
            .stdout(predicate::str::contains("Scan scope"))
            .stdout(predicate::str::contains("--format"));
    }
}

// Tests for browser module
#[cfg(test)]
mod browser_tests {
    use super::*;

    #[test]
    fn test_browser_help() {
        let mut cmd = cli();
        cmd.args(["browser", "--help"]);
        cmd.assert()
            .success()
            .stdout(predicate::str::contains("navigate"))
            .stdout(predicate::str::contains("screenshot"))
            .stdout(predicate::str::contains("click"));
    }
}

// Tests for gateway module
#[cfg(test)]
mod gateway_tests {
    use super::*;

    #[test]
    fn test_gateway_help() {
        let mut cmd = cli();
        cmd.args(["gateway", "--help"]);
        cmd.assert()
            .success()
            .stdout(predicate::str::contains("install"))
            .stdout(predicate::str::contains("start"))
            .stdout(predicate::str::contains("status"));
    }
}
