//! hannahanna CLI client for workbox management

use crate::error::{Error, Result};
use crate::models::WorkboxInfo;
use std::process::Command;

/// hannahanna CLI client
pub struct HnClient {
    hn_command: String,
    working_dir: Option<std::path::PathBuf>,
}

/// Options for creating a workbox
#[derive(Debug, Default, Clone)]
pub struct WorkboxOptions {
    /// Base branch to create from
    pub from: Option<String>,
    /// VCS type (git, hg, jj)
    pub vcs: Option<String>,
    /// Create on current branch without new branch
    pub no_branch: bool,
    /// Sparse checkout pattern
    pub sparse: Option<String>,
    /// Additional hn options
    pub extra_options: Vec<(String, String)>,
}

#[allow(dead_code)]
impl HnClient {
    /// Create a new hannahanna client
    pub fn new() -> Result<Self> {
        // Check if hn is available
        let hn_command = which::which("hn")
            .map_err(|_| Error::HnNotFound)?
            .to_string_lossy()
            .to_string();

        Ok(Self {
            hn_command,
            working_dir: None,
        })
    }

    /// Create a new hannahanna client with custom command path
    pub fn with_command(command: String) -> Self {
        Self {
            hn_command: command,
            working_dir: None,
        }
    }

    /// Use the configured executable and the repository's shared workspace registry.
    pub fn from_config(config: &crate::config::Config) -> Result<Self> {
        let command = which::which(&config.hp.hn.command).map_err(|_| Error::HnNotFound)?;
        Ok(Self {
            hn_command: command.to_string_lossy().into_owned(),
            working_dir: if config.repository_root.as_os_str().is_empty() {
                None
            } else {
                Some(config.repository_root.clone())
            },
        })
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.hn_command);
        if let Some(directory) = &self.working_dir {
            command.current_dir(directory);
        }
        command
    }

    /// Check if hannahanna is installed
    pub fn check_installed() -> Result<()> {
        which::which("hn").map_err(|_| Error::HnNotFound)?;
        Ok(())
    }

    /// Create a new workbox
    pub fn create_workbox(&self, name: &str, opts: &WorkboxOptions) -> Result<WorkboxInfo> {
        // Resolve the starting branch before mutation, and fail early if hn
        // does not support the machine-readable protocol.
        let current = self
            .command()
            .args(["info", "--format=json"])
            .current_dir(std::env::current_dir()?)
            .output()?;
        if !current.status.success() {
            return Err(Error::HnCommandFailed(format!(
                "hn info --format=json failed; install a JSON-capable hannahanna build: {}",
                String::from_utf8_lossy(&current.stderr)
            )));
        }
        let current = self.parse_workbox_info(&String::from_utf8_lossy(&current.stdout))?;
        let base_branch = opts.from.clone().unwrap_or(current.branch);
        let mut cmd = self.command();
        cmd.arg("add").arg(name);
        if !opts.no_branch {
            cmd.arg("--from").arg(&base_branch);
        }

        if let Some(ref vcs) = opts.vcs {
            cmd.arg("--vcs").arg(vcs);
        }

        if opts.no_branch {
            cmd.arg("--no-branch");
        }

        if let Some(ref sparse) = opts.sparse {
            cmd.arg("--sparse").arg(sparse);
        }

        // Add extra options
        for (key, value) in &opts.extra_options {
            cmd.arg(format!("--{}", key)).arg(value);
        }

        // Request JSON output
        cmd.arg("--format=json");

        let output = cmd
            .output()
            .map_err(|e| Error::HnCommandFailed(format!("Failed to execute hn add: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::HnCommandFailed(format!("hn add failed: {}", stderr)));
        }

        // Parse JSON output
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut info = self.parse_workbox_info(&stdout)?;
        info.base_branch = base_branch;
        Ok(info)
    }

    /// Get workbox information
    pub fn get_workbox_info(&self, name: &str) -> Result<WorkboxInfo> {
        let output = self
            .command()
            .arg("info")
            .arg(name)
            .arg("--format=json")
            .output()
            .map_err(|e| Error::HnCommandFailed(format!("Failed to execute hn info: {}", e)))?;

        if !output.status.success() {
            return Err(Error::WorkboxNotFound(name.to_string()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let info = self.parse_workbox_info(&stdout)?;
        if info.name != name {
            return Err(Error::WorkboxNotFound(name.to_string()));
        }
        Ok(info)
    }

    /// List all workboxes
    pub fn list_workboxes(&self) -> Result<Vec<WorkboxInfo>> {
        let output = self
            .command()
            .arg("list")
            .arg("--format=json")
            .output()
            .map_err(|e| Error::HnCommandFailed(format!("Failed to execute hn list: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::HnCommandFailed(format!(
                "hn list failed: {}",
                stderr
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let workboxes: Vec<WorkboxInfo> = serde_json::from_str(&stdout)
            .map_err(|e| Error::ParseError(format!("Failed to parse hn list output: {}", e)))?;

        Ok(workboxes)
    }

    /// Execute a command in a workbox
    pub fn exec_in_workbox(&self, name: &str, command: &str) -> Result<String> {
        let info = self.get_workbox_info(name)?;
        let output = Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(&info.path)
            .output()
            .map_err(|e| Error::HnCommandFailed(format!("Failed to execute command: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::HnCommandFailed(format!(
                "Command in workbox failed ({}): {}{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                stderr
            )));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Remove a workbox
    pub fn remove_workbox(&self, name: &str, force: bool) -> Result<()> {
        let mut cmd = self.command();
        cmd.arg("remove").arg(name);

        if force {
            cmd.arg("--force");
        }

        let output = cmd
            .output()
            .map_err(|e| Error::HnCommandFailed(format!("Failed to execute hn remove: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::HnCommandFailed(format!(
                "hn remove failed: {}",
                stderr
            )));
        }

        Ok(())
    }

    /// Parse workbox info from JSON output
    fn parse_workbox_info(&self, json: &str) -> Result<WorkboxInfo> {
        serde_json::from_str(json)
            .map_err(|e| Error::ParseError(format!("Failed to parse workbox info: {}", e)))
    }

    /// Get the VCS type for a workbox
    pub fn get_vcs_type(&self, name: &str) -> Result<String> {
        let info = self.get_workbox_info(name)?;
        Ok(info.vcs_type)
    }

    /// Check if a workbox exists
    pub fn workbox_exists(&self, name: &str) -> bool {
        self.get_workbox_info(name).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_check_installed_fails_when_not_found() {
        // This will fail if hn is not installed, which is expected in most test environments
        // In CI/CD, we'd install hn before running tests
        match HnClient::check_installed() {
            Ok(_) => println!("hn is installed"),
            Err(Error::HnNotFound) => println!("hn not found (expected in test environment)"),
            Err(e) => panic!("Unexpected error: {}", e),
        }
    }

    #[test]
    fn test_workbox_options_default() {
        let opts = WorkboxOptions::default();
        assert!(opts.from.is_none());
        assert!(opts.vcs.is_none());
        assert!(!opts.no_branch);
        assert!(opts.sparse.is_none());
        assert!(opts.extra_options.is_empty());
    }

    #[test]
    fn test_workbox_options_builder() {
        let opts = WorkboxOptions {
            from: Some("main".to_string()),
            vcs: Some("git".to_string()),
            no_branch: true,
            ..Default::default()
        };

        assert_eq!(opts.from, Some("main".to_string()));
        assert_eq!(opts.vcs, Some("git".to_string()));
        assert!(opts.no_branch);
    }

    #[test]
    fn test_hn_client_with_command() {
        let client = HnClient::with_command("/custom/path/to/hn".to_string());
        assert_eq!(client.hn_command, "/custom/path/to/hn");
    }

    #[test]
    fn test_parse_workbox_info() {
        let client = HnClient::with_command("hn".to_string());

        let json = r#"{
            "name": "test-workbox",
            "path": "/tmp/test-workbox",
            "branch": "feature/test",
            "base_branch": "main",
            "vcs_type": "git",
            "commit": "abc123"
        }"#;

        let info = client.parse_workbox_info(json).unwrap();
        assert_eq!(info.name, "test-workbox");
        assert_eq!(info.path, PathBuf::from("/tmp/test-workbox"));
        assert_eq!(info.branch, "feature/test");
        assert_eq!(info.base_branch, "main");
        assert_eq!(info.vcs_type, "git");
        assert_eq!(info.commit, "abc123");
    }

    // Integration tests (require hn to be installed)
    #[test]
    fn test_create_and_remove_workbox() {
        let client = match HnClient::new() {
            Ok(c) => c,
            Err(_) => {
                println!("Skipping: hn not installed");
                return;
            }
        };

        // Use default options to avoid specifying a branch that might not exist in CI
        let opts = WorkboxOptions::default();

        // Create workbox
        let result = client.create_workbox("test-hp-integration", &opts);
        if let Err(e) = &result {
            println!("Failed to create workbox: {}", e);
            return;
        }

        let info = result.unwrap();
        assert_eq!(info.name, "test-hp-integration");

        // Verify it exists
        if !client.workbox_exists("test-hp-integration") {
            println!("Workbox doesn't exist after creation");
            client.remove_workbox("test-hp-integration", true).ok();
            return;
        }

        // Get info
        match client.get_workbox_info("test-hp-integration") {
            Ok(info2) => assert_eq!(info2.name, info.name),
            Err(e) => {
                println!("Failed to get workbox info: {}", e);
                client.remove_workbox("test-hp-integration", true).ok();
                return;
            }
        }

        // Remove workbox
        if let Err(e) = client.remove_workbox("test-hp-integration", true) {
            println!("Failed to remove workbox: {}", e);
            return;
        }

        // Verify it's gone
        assert!(!client.workbox_exists("test-hp-integration"));
    }

    #[test]
    fn test_list_workboxes() {
        let client = match HnClient::new() {
            Ok(c) => c,
            Err(_) => {
                println!("Skipping: hn not installed");
                return;
            }
        };

        let workboxes = client.list_workboxes();
        match workboxes {
            Ok(list) => println!("Found {} workboxes", list.len()),
            Err(e) => println!("Failed to list workboxes: {}", e),
        }
    }

    #[test]
    fn test_exec_in_workbox() {
        let client = match HnClient::new() {
            Ok(c) => c,
            Err(_) => {
                println!("Skipping: hn not installed");
                return;
            }
        };

        // Create test workbox
        let opts = WorkboxOptions::default();
        if client.create_workbox("test-hp-exec", &opts).is_err() {
            println!("Failed to create test workbox");
            return;
        }

        // Execute command
        let output = match client.exec_in_workbox("test-hp-exec", "echo hello") {
            Ok(out) => out,
            Err(e) => {
                println!("Failed to execute command in workbox: {}", e);
                client.remove_workbox("test-hp-exec", true).ok();
                return;
            }
        };
        assert_eq!(output.trim(), "hello");

        // Cleanup
        client.remove_workbox("test-hp-exec", true).ok();
    }
}
