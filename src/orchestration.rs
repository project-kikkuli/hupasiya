//! Multi-agent orchestration: cascade and gather operations

use crate::config::Config;
use crate::context::ContextManager;
use crate::error::{Error, Result};
use crate::hn_client::HnClient;
use crate::models::{ActivityType, Session, SnapshotTrigger};
use crate::session::SessionManager;
use colored::Colorize;

/// Orchestration engine for multi-agent coordination
pub struct Orchestrator {
    config: Config,
    session_mgr: SessionManager,
    context_mgr: ContextManager,
    hn_client: HnClient,
}

impl Orchestrator {
    /// Create a new orchestrator
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

    /// Cascade: Sync parent changes to all children
    pub fn cascade(&self, parent_name: &str, dry_run: bool) -> Result<()> {
        let parent = self.session_mgr.load_session(parent_name)?;

        if parent.children.is_empty() {
            println!("{}", "No child sessions to cascade to.".yellow());
            return Ok(());
        }

        println!(
            "{} Cascading '{}' to {} children...",
            "→".cyan(),
            parent_name.bold(),
            parent.children.len()
        );
        println!();

        let mut cascaded = 0;
        let mut skipped = 0;
        let mut failed = 0;

        for child_name in &parent.children {
            match self.cascade_to_child(&parent, child_name, dry_run) {
                Ok(true) => cascaded += 1,
                Ok(false) => skipped += 1,
                Err(e) => {
                    eprintln!(
                        "  {} Failed to cascade to '{}': {}",
                        "✗".red(),
                        child_name,
                        e
                    );
                    failed += 1;
                }
            }
        }

        if failed > 0 {
            return Err(Error::Other(format!(
                "Cascade failed for {} child session(s); {} completed, {} skipped",
                failed, cascaded, skipped
            )));
        }
        println!();
        if dry_run {
            println!("{} Dry run complete", "ℹ".blue());
        } else {
            println!(
                "{} Cascade complete: {} cascaded, {} skipped",
                "✓".green(),
                cascaded,
                skipped
            );
        }

        Ok(())
    }

    /// Cascade to a single child
    fn cascade_to_child(&self, parent: &Session, child_name: &str, dry_run: bool) -> Result<bool> {
        // Load child session
        let mut child = self.session_mgr.load_session(child_name)?;

        println!("  {} {}", "→".cyan(), child_name);

        // Get child workbox info
        let child_wb = self.hn_client.get_workbox_info(&child.workbox_name)?;

        // Determine merge command based on VCS
        let merge_cmd = match child_wb.vcs_type.as_str() {
            "git" => format!("git merge {}", parent.branch),
            "hg" | "mercurial" => format!("hg merge {}", parent.branch),
            "jj" | "jujutsu" => format!("jj rebase -d {}", parent.branch),
            _ => {
                return Err(Error::Other(format!(
                    "Unknown VCS type: {}",
                    child_wb.vcs_type
                )))
            }
        };

        if dry_run {
            println!("    Would run: {}", merge_cmd.yellow());
            return Ok(false);
        }

        // Create snapshot before cascade if enabled
        if self.config.hp.sessions.auto_snapshot {
            let _ = self.context_mgr.create_snapshot(
                &child,
                "before-cascade",
                SnapshotTrigger::BeforeCascade,
                Some(format!("Before cascading from {}", parent.name)),
            );
        }

        // Execute merge in workbox
        match self
            .hn_client
            .exec_in_workbox(&child.workbox_name, &merge_cmd)
        {
            Ok(output) => {
                if output.contains("conflict") || output.contains("CONFLICT") {
                    println!(
                        "    {} Conflicts detected - manual resolution required",
                        "⚠".yellow()
                    );
                    println!(
                        "    Run: cd {} && resolve conflicts",
                        child_wb.path.display()
                    );
                } else {
                    println!("    {} Merged successfully", "✓".green());
                }
            }
            Err(e) => {
                return Err(Error::Other(format!("Merge failed: {}", e)));
            }
        }

        // Update child activity log
        child.log_activity(
            ActivityType::Cascaded,
            format!("Cascaded changes from parent '{}'", parent.name),
        );
        self.session_mgr.save_session(&child)?;

        Ok(true)
    }

    /// Gather: Collect all children back to parent
    pub fn gather(&self, parent_name: &str, dry_run: bool) -> Result<()> {
        let mut parent = self.session_mgr.load_session(parent_name)?;

        if parent.children.is_empty() {
            println!("{}", "No child sessions to gather from.".yellow());
            return Ok(());
        }

        println!(
            "{} Gathering {} children to '{}'...",
            "←".cyan(),
            parent.children.len(),
            parent_name.bold()
        );
        println!();

        // Create snapshot before gather if enabled
        if !dry_run && self.config.hp.sessions.auto_snapshot {
            let _ = self.context_mgr.create_snapshot(
                &parent,
                "before-gather",
                SnapshotTrigger::BeforeGather,
                Some("Before gathering children".to_string()),
            );
        }

        let mut gathered = 0;
        let mut skipped = 0;
        let mut failed = 0;

        for child_name in parent.children.clone() {
            match self.gather_from_child(&parent, &child_name, dry_run) {
                Ok(true) => gathered += 1,
                Ok(false) => skipped += 1,
                Err(e) => {
                    eprintln!(
                        "  {} Failed to gather from '{}': {}",
                        "✗".red(),
                        child_name,
                        e
                    );
                    failed += 1;
                }
            }
        }

        if failed > 0 {
            return Err(Error::Other(format!(
                "Gather failed for {} child session(s); {} completed, {} skipped",
                failed, gathered, skipped
            )));
        }
        println!();
        if dry_run {
            println!("{} Dry run complete", "ℹ".blue());
        } else {
            // Update parent activity
            parent.log_activity(
                ActivityType::Gathered,
                format!("Gathered {} children", gathered),
            );
            self.session_mgr.save_session(&parent)?;

            println!(
                "{} Gather complete: {} gathered, {} skipped",
                "✓".green(),
                gathered,
                skipped
            );
        }

        Ok(())
    }

    /// Gather from a single child
    fn gather_from_child(&self, parent: &Session, child_name: &str, dry_run: bool) -> Result<bool> {
        let child = self.session_mgr.load_session(child_name)?;

        println!("  {} {}", "←".cyan(), child_name);

        // Get parent workbox info
        let parent_wb = self.hn_client.get_workbox_info(&parent.workbox_name)?;

        // Determine merge command based on VCS
        let merge_cmd = match parent_wb.vcs_type.as_str() {
            "git" => format!("git merge {}", child.branch),
            "hg" | "mercurial" => format!("hg merge {}", child.branch),
            "jj" | "jujutsu" => format!("jj rebase -s {} -d {}", child.branch, parent.branch),
            _ => {
                return Err(Error::Other(format!(
                    "Unknown VCS type: {}",
                    parent_wb.vcs_type
                )))
            }
        };

        if dry_run {
            println!("    Would run: {}", merge_cmd.yellow());
            return Ok(false);
        }

        // Execute merge in parent workbox
        match self
            .hn_client
            .exec_in_workbox(&parent.workbox_name, &merge_cmd)
        {
            Ok(output) => {
                if output.contains("conflict") || output.contains("CONFLICT") {
                    println!(
                        "    {} Conflicts detected - manual resolution required",
                        "⚠".yellow()
                    );
                    println!(
                        "    Run: cd {} && resolve conflicts",
                        parent_wb.path.display()
                    );
                } else {
                    println!("    {} Merged successfully", "✓".green());
                }
            }
            Err(e) => {
                return Err(Error::Other(format!("Merge failed: {}", e)));
            }
        }

        Ok(true)
    }

    /// Show session tree
    pub fn show_tree(&self, root_name: Option<String>) -> Result<()> {
        let sessions = self.session_mgr.list_sessions()?;

        if sessions.is_empty() {
            println!("{}", "No sessions found.".yellow());
            return Ok(());
        }

        let roots: Vec<_> = if let Some(name) = root_name {
            vec![self.session_mgr.load_session(&name)?]
        } else {
            sessions
                .iter()
                .filter(|s| s.parent.is_none())
                .cloned()
                .collect()
        };

        println!();
        for root in roots {
            self.print_tree_node(&root, &sessions, 0)?;
        }
        println!();

        Ok(())
    }

    /// Print a tree node recursively
    #[allow(clippy::only_used_in_recursion)]
    fn print_tree_node(
        &self,
        session: &Session,
        all_sessions: &[Session],
        depth: usize,
    ) -> Result<()> {
        let indent = "  ".repeat(depth);
        let prefix = if depth == 0 { "" } else { "└─ " };

        let status_icon = match session.status {
            crate::models::SessionStatus::Active => "●".green(),
            crate::models::SessionStatus::Paused => "◐".yellow(),
            _ => "○".normal(),
        };

        println!(
            "{}{}{} {} ({:?}) - {}",
            indent,
            prefix,
            status_icon,
            session.name.bold(),
            session.agent_type,
            session.branch
        );

        // Print children
        for child_name in &session.children {
            if let Some(child) = all_sessions.iter().find(|s| s.name == *child_name) {
                self.print_tree_node(child, all_sessions, depth + 1)?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::TempDir;

    fn create_test_orchestrator() -> Result<(Orchestrator, TempDir)> {
        let temp_dir = TempDir::new().unwrap();
        let mut config = Config::default();
        config.hp.sessions.metadata_dir = temp_dir.path().join("sessions");
        config.hp.sessions.context_dir = temp_dir.path().join("contexts");

        let orchestrator = Orchestrator::new(config)?;
        Ok((orchestrator, temp_dir))
    }

    #[test]
    fn test_orchestrator_creation() {
        match create_test_orchestrator() {
            Ok((_orch, _temp)) => {
                // Successfully created orchestrator
            }
            Err(Error::HnNotFound) => {
                println!("Skipping: hn not installed");
            }
            Err(e) => panic!("Unexpected error: {}", e),
        }
    }

    #[test]
    fn test_show_tree_empty() {
        let (orch, _temp) = match create_test_orchestrator() {
            Ok(x) => x,
            Err(Error::HnNotFound) => {
                println!("Skipping: hn not installed");
                return;
            }
            Err(e) => panic!("Unexpected error: {}", e),
        };
        let result = orch.show_tree(None);
        assert!(result.is_ok());
    }

    // Integration tests
    #[test]
    fn test_cascade_integration() {
        // This test requires hn to be installed
        // Test cascade operation with real workboxes
    }

    #[test]
    fn test_gather_integration() {
        // This test requires hn to be installed
        // Test gather operation with real workboxes
    }
}
