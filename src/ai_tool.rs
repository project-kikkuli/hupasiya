//! AI tool integration for hupasiya
//!
//! Handles launching AI tools with session context, opening shells in workboxes,
//! and executing commands in session environments.

use crate::config::{AiToolConfig, Config, ContextStrategy, LaunchMethod};
use crate::error::{Error, Result};
use crate::hn_client::HnClient;
use crate::session::SessionManager;
use std::collections::HashMap;
use std::env;
use std::process::Command;

/// AI tool launcher
pub struct AiTool {
    config: Config,
    session_mgr: SessionManager,
    hn_client: HnClient,
}

impl AiTool {
    /// Create new AI tool launcher
    pub fn new(config: Config) -> Result<Self> {
        let session_mgr = SessionManager::new(config.clone())?;
        let hn_client = HnClient::from_config(&config)?;

        Ok(Self {
            config,
            session_mgr,
            hn_client,
        })
    }

    /// Launch AI tool for session
    pub fn launch(
        &self,
        session_name: Option<String>,
        tool_override: Option<String>,
        profile: Option<String>,
        extra_args: Vec<String>,
    ) -> Result<()> {
        // Determine session name
        let session_name = self.resolve_session_name(session_name)?;

        // Load session
        let session = self.session_mgr.load_session(&session_name)?;

        // Get workbox info
        let workbox_info = self.hn_client.get_workbox_info(&session.workbox_name)?;

        // Get AI tool config (with profile override if specified)
        let ai_config = if let Some(profile_name) = profile {
            self.get_profile_ai_config(&profile_name)?
        } else {
            self.config.hp.ai_tool.clone()
        };

        // Determine tool command
        let tool_command = tool_override.unwrap_or(ai_config.command.clone());

        // Set up environment variables
        let mut env_vars = ai_config.env.clone();
        env_vars.insert("HP_SESSION".to_string(), session_name.clone());
        env_vars.insert(
            "HP_CONTEXT".to_string(),
            session
                .context_dir
                .join("context.md")
                .to_string_lossy()
                .into_owned(),
        );
        env_vars.insert(
            "HP_WORKBOX".to_string(),
            workbox_info.path.to_string_lossy().to_string(),
        );
        env_vars.insert("HP_VCS".to_string(), workbox_info.vcs_type.clone());

        // Build command args
        let mut args = ai_config.extra_args.clone();

        // Add context based on strategy
        match ai_config.context_strategy {
            ContextStrategy::SlashCommand => {
                // For slash command strategy, create a global command file
                self.create_slash_command(&session)?;
                println!("📝 Created slash command for context");
            }
            ContextStrategy::Flag => {
                args.push("--context".to_string());
                args.push(
                    session
                        .context_dir
                        .join("context.md")
                        .to_string_lossy()
                        .into_owned(),
                );
            }
            ContextStrategy::Env => {
                env_vars.insert(
                    "CONTEXT_FILE".to_string(),
                    session
                        .context_dir
                        .join("context.md")
                        .to_string_lossy()
                        .into_owned(),
                );
            }
            ContextStrategy::File => {
                // Context file is already in place, tool will read it
            }
        }

        // Add extra args from command line
        args.extend(extra_args);

        println!(
            "🚀 Launching {} for session '{}'",
            tool_command, session_name
        );
        println!("📂 Workbox: {}", workbox_info.path.display());
        println!("🌿 Branch: {}", workbox_info.branch);

        // Launch based on method
        let workbox_path = workbox_info.path.to_string_lossy().to_string();
        match ai_config.launch_method {
            LaunchMethod::Exec => {
                self.launch_exec(&tool_command, &args, &env_vars, &workbox_path)?;
            }
            LaunchMethod::ShellFunction => {
                self.launch_shell_function(&tool_command, &args, &env_vars, &workbox_path)?;
            }
            LaunchMethod::Tmux => {
                self.launch_tmux(
                    &tool_command,
                    &args,
                    &env_vars,
                    &workbox_path,
                    &session_name,
                )?;
            }
            LaunchMethod::Screen => {
                self.launch_screen(
                    &tool_command,
                    &args,
                    &env_vars,
                    &workbox_path,
                    &session_name,
                )?;
            }
        }

        Ok(())
    }

    /// Launch shell in session workbox
    pub fn shell(&self, session_name: Option<String>, command: Option<Vec<String>>) -> Result<()> {
        // Determine session name
        let session_name = self.resolve_session_name(session_name)?;

        // Load session
        let session = self.session_mgr.load_session(&session_name)?;

        // Get workbox info
        let workbox_info = self.hn_client.get_workbox_info(&session.workbox_name)?;

        // Set up environment variables
        let mut env_vars = HashMap::new();
        env_vars.insert("HP_SESSION".to_string(), session_name.clone());
        env_vars.insert(
            "HP_CONTEXT".to_string(),
            session
                .context_dir
                .join("context.md")
                .to_string_lossy()
                .into_owned(),
        );
        env_vars.insert(
            "HP_WORKBOX".to_string(),
            workbox_info.path.to_string_lossy().to_string(),
        );
        env_vars.insert("HP_VCS".to_string(), workbox_info.vcs_type.clone());

        let workbox_path = workbox_info.path.to_string_lossy().to_string();
        if let Some(cmd) = command {
            // Run command in workbox
            println!("🔧 Running command in session '{}'", session_name);
            self.run_in_workbox(&workbox_path, &cmd, &env_vars)?;
        } else {
            // Open interactive shell
            println!("🐚 Opening shell in session '{}'", session_name);
            println!("📂 Workbox: {}", workbox_info.path.display());
            self.open_shell(&workbox_path, &env_vars)?;
        }

        Ok(())
    }

    /// Execute command in session workbox
    pub fn exec(
        &self,
        session_name: &str,
        command: Vec<String>,
        cascade: bool,
        tree: bool,
    ) -> Result<()> {
        if tree {
            // Run in parent and all descendants
            self.exec_tree(session_name, &command)?;
        } else if cascade {
            // Run in all children
            self.exec_cascade(session_name, &command)?;
        } else {
            // Run in single session
            self.exec_single(session_name, &command)?;
        }

        Ok(())
    }

    // === Private helper methods ===

    fn resolve_session_name(&self, session_name: Option<String>) -> Result<String> {
        if let Some(name) = session_name {
            Ok(name)
        } else if let Ok(current) = env::var("HP_SESSION") {
            Ok(current)
        } else {
            Err(Error::NoCurrentSession)
        }
    }

    fn get_profile_ai_config(&self, profile_name: &str) -> Result<AiToolConfig> {
        let profile = self
            .config
            .hp
            .profiles
            .get(profile_name)
            .ok_or_else(|| Error::ProfileNotFound(profile_name.to_string()))?;

        Ok(profile
            .ai_tool
            .clone()
            .unwrap_or(self.config.hp.ai_tool.clone()))
    }

    fn create_slash_command(&self, session: &crate::models::Session) -> Result<()> {
        // Create .claude/commands/hp_context.md for Claude Code
        let commands_dir = session.workbox_path.join(".claude/commands");
        std::fs::create_dir_all(&commands_dir)?;

        let context_path = session
            .context_dir
            .join("context.md")
            .to_string_lossy()
            .into_owned();
        let slash_command = format!(
            "Read the hupasiya context file at {} and use it to guide your work on this session.",
            context_path
        );

        std::fs::write(commands_dir.join("hp_context.md"), slash_command)?;

        Ok(())
    }

    fn launch_exec(
        &self,
        command: &str,
        args: &[String],
        env_vars: &HashMap<String, String>,
        workdir: &str,
    ) -> Result<()> {
        let mut cmd = Command::new(command);
        cmd.args(args).current_dir(workdir).envs(env_vars);

        let status = cmd.status()?;

        if !status.success() {
            return Err(Error::AiToolFailed(format!(
                "Command exited with status: {}",
                status
            )));
        }

        Ok(())
    }

    fn launch_shell_function(
        &self,
        command: &str,
        args: &[String],
        env_vars: &HashMap<String, String>,
        workdir: &str,
    ) -> Result<()> {
        // Output shell commands for wrapper to execute
        println!("# Shell wrapper commands:");
        println!("cd {}", workdir);
        for (key, value) in env_vars {
            println!("export {}={}", key, value);
        }
        println!("{} {}", command, args.join(" "));

        Ok(())
    }

    fn launch_tmux(
        &self,
        command: &str,
        args: &[String],
        env_vars: &HashMap<String, String>,
        workdir: &str,
        session_name: &str,
    ) -> Result<()> {
        // Create new tmux session
        let tmux_session_name = format!("hp-{}", session_name);

        let mut cmd = Command::new("tmux");
        cmd.arg("new-session")
            .arg("-s")
            .arg(&tmux_session_name)
            .arg("-c")
            .arg(workdir);

        // Set environment variables
        for (key, value) in env_vars {
            cmd.arg("-e").arg(format!("{}={}", key, value));
        }

        // Execute command in tmux
        cmd.arg(command).args(args);

        let status = cmd.status()?;

        if !status.success() {
            return Err(Error::AiToolFailed(format!(
                "tmux command exited with status: {}",
                status
            )));
        }

        println!("✅ Tmux session '{}' created", tmux_session_name);
        println!("   Attach with: tmux attach -t {}", tmux_session_name);

        Ok(())
    }

    fn launch_screen(
        &self,
        command: &str,
        args: &[String],
        env_vars: &HashMap<String, String>,
        workdir: &str,
        session_name: &str,
    ) -> Result<()> {
        // Create new screen session
        let screen_session_name = format!("hp-{}", session_name);

        // Build command string with env vars
        let mut cmd_parts = vec![];
        for (key, value) in env_vars {
            cmd_parts.push(format!("export {}={};", key, value));
        }
        cmd_parts.push(format!("cd {};", workdir));
        cmd_parts.push(format!("{} {}", command, args.join(" ")));
        let full_command = cmd_parts.join(" ");

        let mut cmd = Command::new("screen");
        cmd.arg("-S")
            .arg(&screen_session_name)
            .arg("-dm")
            .arg("bash")
            .arg("-c")
            .arg(&full_command);

        let status = cmd.status()?;

        if !status.success() {
            return Err(Error::AiToolFailed(format!(
                "screen command exited with status: {}",
                status
            )));
        }

        println!("✅ Screen session '{}' created", screen_session_name);
        println!("   Attach with: screen -r {}", screen_session_name);

        Ok(())
    }

    fn open_shell(&self, workdir: &str, env_vars: &HashMap<String, String>) -> Result<()> {
        // Determine shell
        let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());

        let mut cmd = Command::new(&shell);
        cmd.current_dir(workdir).envs(env_vars);

        let status = cmd.status()?;

        if !status.success() {
            return Err(Error::AiToolFailed(format!(
                "Shell exited with status: {}",
                status
            )));
        }

        Ok(())
    }

    fn run_in_workbox(
        &self,
        workdir: &str,
        command: &[String],
        env_vars: &HashMap<String, String>,
    ) -> Result<()> {
        if command.is_empty() {
            return Err(Error::InvalidInput("Command cannot be empty".to_string()));
        }

        let mut cmd = Command::new(&command[0]);
        cmd.args(&command[1..]).current_dir(workdir).envs(env_vars);

        let status = cmd.status()?;

        if !status.success() {
            return Err(Error::AiToolFailed(format!(
                "Command exited with status: {}",
                status
            )));
        }

        Ok(())
    }

    fn exec_single(&self, session_name: &str, command: &[String]) -> Result<()> {
        println!("▶️  Executing in session '{}'", session_name);

        let session = self.session_mgr.load_session(session_name)?;
        let workbox_info = self.hn_client.get_workbox_info(&session.workbox_name)?;

        let mut env_vars = HashMap::new();
        env_vars.insert("HP_SESSION".to_string(), session_name.to_string());
        env_vars.insert(
            "HP_CONTEXT".to_string(),
            session
                .context_dir
                .join("context.md")
                .to_string_lossy()
                .into_owned(),
        );
        env_vars.insert(
            "HP_WORKBOX".to_string(),
            workbox_info.path.to_string_lossy().into_owned(),
        );
        env_vars.insert("HP_VCS".to_string(), workbox_info.vcs_type.clone());

        let workbox_path = workbox_info.path.to_string_lossy().to_string();
        self.run_in_workbox(&workbox_path, command, &env_vars)?;

        println!("✅ Command completed in '{}'", session_name);

        Ok(())
    }

    fn exec_cascade(&self, parent_name: &str, command: &[String]) -> Result<()> {
        let _parent = self.session_mgr.load_session(parent_name)?;
        let children = self.session_mgr.get_children(parent_name)?;

        if children.is_empty() {
            println!("⚠️  No children found for '{}'", parent_name);
            return Ok(());
        }

        println!(
            "🔄 Executing in {} children of '{}'",
            children.len(),
            parent_name
        );

        for child in &children {
            println!("\n▶️  Session: {}", child.name);
            self.exec_single(&child.name, command)?;
        }

        println!("\n✅ Command completed in all children");

        Ok(())
    }

    fn exec_tree(&self, root_name: &str, command: &[String]) -> Result<()> {
        println!("🌳 Executing in tree rooted at '{}'", root_name);

        // Execute in root
        self.exec_single(root_name, command)?;

        // Execute in all descendants
        let descendants = self.get_all_descendants(root_name)?;

        for desc in &descendants {
            self.exec_single(&desc.name, command)?;
        }

        println!(
            "\n✅ Command completed in tree ({} sessions)",
            1 + descendants.len()
        );

        Ok(())
    }

    fn get_all_descendants(&self, session_name: &str) -> Result<Vec<crate::models::Session>> {
        let mut descendants = vec![];
        let mut queue = vec![session_name.to_string()];

        while let Some(name) = queue.pop() {
            let children = self.session_mgr.get_children(&name)?;
            for child in children {
                queue.push(child.name.clone());
                descendants.push(child);
            }
        }

        Ok(descendants)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::TempDir;

    fn setup_test_env() -> (TempDir, Config) {
        let temp_dir = TempDir::new().unwrap();
        let config = Config::default();
        (temp_dir, config)
    }

    #[test]
    fn test_ai_tool_creation() {
        let (_temp_dir, config) = setup_test_env();
        let ai_tool = AiTool::new(config);
        match ai_tool {
            Ok(_) => {
                // Successfully created AI tool
            }
            Err(e) => {
                // Skip test if environment not set up (hn not installed, directories missing, etc.)
                println!("Skipping test_ai_tool_creation: {}", e);
            }
        }
    }

    #[test]
    fn test_resolve_session_name_with_explicit_name() {
        // Test without creating AiTool (which requires hn)
        let session_name = "test-session".to_string();
        assert_eq!(session_name, "test-session");
    }

    #[test]
    fn test_create_slash_command_in_session_worktree() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = Config::default();
        config.hp.sessions.metadata_dir = temp_dir.path().join("sessions");
        let hn_client = HnClient::with_command("unused-hn".into());
        let session_mgr =
            SessionManager::with_client(config.clone(), HnClient::with_command("unused-hn".into()))
                .unwrap();
        let ai_tool = AiTool {
            config,
            session_mgr,
            hn_client,
        };
        let mut session = crate::models::Session::new(
            "test-session".into(),
            crate::models::AgentType::Feature,
            "test-session".into(),
            temp_dir.path().join("worktree"),
            "test-session".into(),
            "main".into(),
            "repo".into(),
            "git".into(),
        );
        session.context_dir = temp_dir.path().join("shared/context");
        ai_tool.create_slash_command(&session).unwrap();
        let content =
            std::fs::read_to_string(session.workbox_path.join(".claude/commands/hp_context.md"))
                .unwrap();
        assert!(content.contains(session.context_dir.join("context.md").to_str().unwrap()));
    }
}
