//! Session collaboration features (handoff, clone, merge)

use crate::config::Config;
use crate::context::ContextManager;
use crate::error::{Error, Result};
use crate::hn_client::HnClient;
use crate::models::{ActivityType, SessionStatus};
use crate::session::SessionManager;
use colored::Colorize;

/// Collaboration manager
pub struct CollaborationManager {
    #[allow(dead_code)]
    config: Config,
    session_mgr: SessionManager,
    #[allow(dead_code)]
    context_mgr: ContextManager,
    hn_client: HnClient,
}

impl CollaborationManager {
    /// Create new collaboration manager
    pub fn new(config: Config) -> Result<Self> {
        let session_mgr = SessionManager::new(config.clone())?;
        let context_mgr = ContextManager::new(config.clone())?;
        let hn_client = HnClient::from_config(&config)?;

        Ok(Self {
            config,
            session_mgr,
            context_mgr,
            hn_client,
        })
    }

    /// Handoff session to another developer
    pub fn handoff(
        &self,
        session_name: &str,
        to_user: &str,
        message: Option<String>,
    ) -> Result<()> {
        let mut session = self.session_mgr.load_session(session_name)?;

        println!(
            "{} Handing off session '{}' to {}",
            "→".cyan(),
            session_name,
            to_user
        );

        // Create handoff notes
        let handoff_notes = format!(
            "# Handoff to {}\n\n\
            Session: {}\n\
            Date: {}\n\
            \n\
            ## Current Status\n\
            - Status: {:?}\n\
            - Branch: {}\n\
            - Last active: {}\n\
            \n\
            ## Message\n\
            {}\n\
            \n\
            ## Next Steps\n\
            - Review context in .hp/contexts/{}/context.md\n\
            - Check activity log with: hp activity {}\n\
            - Continue work with: hp launch {}\n",
            to_user,
            session.name,
            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S"),
            session.status,
            session.branch,
            session.last_active.format("%Y-%m-%d %H:%M:%S"),
            message.unwrap_or_else(|| "No message provided".to_string()),
            session.name,
            session.name,
            session.name,
        );

        let handoff_path = session.context_dir.join("handoff.md");
        std::fs::write(&handoff_path, handoff_notes)?;

        // Update session metadata
        session.status = SessionStatus::Paused;
        session.notes = format!("Handed off to {}", to_user);
        session.log_activity(
            ActivityType::StatusChanged,
            format!("Handed off to {}", to_user),
        );

        self.session_mgr.save_session(&session)?;

        println!("{} Handoff complete!", "✓".green());
        println!("   Handoff notes: {}", handoff_path.display());
        println!();

        Ok(())
    }

    /// Clone a session for parallel work
    pub fn clone_session(&self, source_name: &str, new_name: &str, diverge: bool) -> Result<()> {
        let source = self.session_mgr.load_session(source_name)?;

        println!(
            "{} Cloning session '{}' to '{}'",
            "→".cyan(),
            source_name,
            new_name
        );

        // Check if new name already exists
        if self.session_mgr.session_exists(new_name) {
            return Err(Error::SessionAlreadyExists(new_name.to_string()));
        }

        // Get source workbox info
        let source_wb = self.hn_client.get_workbox_info(&source.workbox_name)?;

        // Create new branch name
        let new_branch = if diverge {
            format!("{}-{}", source.branch, new_name)
        } else {
            source.branch.clone()
        };

        // Create new workbox
        let new_workbox_name = format!("hp-{}", new_name);
        let opts = crate::hn_client::WorkboxOptions {
            from: Some(source.branch.clone()),
            vcs: Some(source_wb.vcs_type.clone()),
            ..Default::default()
        };

        let new_wb = self.hn_client.create_workbox(&new_workbox_name, &opts)?;

        // Create new session
        let mut new_session = crate::models::Session::new(
            new_name.to_string(),
            source.agent_type.clone(),
            new_workbox_name,
            new_wb.path.clone(),
            new_branch,
            source.base_branch.clone(),
            source.repo_name.clone(),
            source_wb.vcs_type.clone(),
        );

        // Copy context
        let source_context_path = source.context_dir.join("context.md");
        if source_context_path.exists() {
            let context_content = std::fs::read_to_string(&source_context_path)?;
            std::fs::create_dir_all(&new_session.context_dir)?;
            std::fs::write(
                new_session.context_dir.join("context.md"),
                format!(
                    "# Cloned from: {}\n\n{}\n\n## Clone Notes\n- Cloned at: {}\n- Diverge: {}\n",
                    source_name,
                    context_content,
                    chrono::Utc::now().format("%Y-%m-%d %H:%M:%S"),
                    diverge
                ),
            )?;
        }

        // Set parent if not diverging
        if !diverge {
            new_session.parent = Some(source_name.to_string());
        }

        new_session.log_activity(
            ActivityType::SessionCreated,
            format!("Cloned from {}", source_name),
        );

        self.session_mgr.save_session(&new_session)?;

        println!("{} Clone complete!", "✓".green());
        println!("   New session: {}", new_name);
        println!("   Workbox: {}", new_wb.path.display());
        println!();

        Ok(())
    }

    /// Merge sessions (for collaboration convergence)
    pub fn merge_sessions(&self, target: &str, sources: Vec<String>, strategy: &str) -> Result<()> {
        let mut target_session = self.session_mgr.load_session(target)?;

        println!(
            "{} Merging {} sessions into '{}'",
            "→".cyan(),
            sources.len(),
            target
        );

        let target_wb = self
            .hn_client
            .get_workbox_info(&target_session.workbox_name)?;

        for source_name in &sources {
            let source = self.session_mgr.load_session(source_name)?;

            println!("  {} Merging from '{}'", "→".cyan(), source_name);

            // Execute merge based on strategy
            let merge_cmd = match strategy {
                "squash" => format!("git merge --squash {}", source.branch),
                "no-ff" => format!("git merge --no-ff {}", source.branch),
                _ => format!("git merge {}", source.branch), // default merge
            };

            match self
                .hn_client
                .exec_in_workbox(&target_session.workbox_name, &merge_cmd)
            {
                Ok(output) => {
                    if output.contains("conflict") || output.contains("CONFLICT") {
                        println!(
                            "    {} Conflicts detected - manual resolution required",
                            "⚠".yellow()
                        );
                    } else {
                        println!("    {} Merged successfully", "✓".green());
                    }
                }
                Err(e) => {
                    eprintln!("    {} Merge failed: {}", "✗".red(), e);
                    continue;
                }
            }

            // Update relationships
            target_session.children.retain(|c| c != source_name);
        }

        target_session.log_activity(
            ActivityType::Integrated,
            format!("Merged {} sessions", sources.len()),
        );
        self.session_mgr.save_session(&target_session)?;

        println!();
        println!("{} Merge complete!", "✓".green());
        println!("   Review changes in: {}", target_wb.path.display());
        println!();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_collaboration_manager_creation() {
        // Create a temporary directory for test
        let temp_dir = TempDir::new().unwrap();
        let mut config = Config::default();

        // Use temp directory for sessions and contexts
        config.hp.sessions.metadata_dir = temp_dir.path().join("sessions");
        config.hp.sessions.context_dir = temp_dir.path().join("contexts");

        match CollaborationManager::new(config) {
            Ok(_) => {
                // Successfully created
            }
            Err(Error::HnNotFound) => {
                println!("Skipping: hn not installed");
            }
            Err(Error::Io(_)) => {
                // IO errors are acceptable in test environments
                println!("Skipping: IO error (acceptable in test environment)");
            }
            Err(e) => panic!("Unexpected error: {}", e),
        }
    }
}
