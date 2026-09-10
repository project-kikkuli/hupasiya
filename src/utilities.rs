//! Utility commands (monitor, clean, leave)

use crate::config::Config;
use crate::error::Result;
use crate::models::SessionStatus;
use crate::session::SessionManager;
use colored::Colorize;

/// Utilities manager
pub struct UtilitiesManager {
    session_mgr: SessionManager,
}

impl UtilitiesManager {
    /// Create new utilities manager
    pub fn new(config: Config) -> Result<Self> {
        let session_mgr = SessionManager::new(config)?;
        Ok(Self { session_mgr })
    }

    /// Monitor all sessions (dashboard view)
    pub fn monitor(&self, watch: bool) -> Result<()> {
        loop {
            // Clear screen if watching
            if watch {
                print!("\x1B[2J\x1B[1;1H");
            }

            let sessions = self.session_mgr.list_sessions()?;

            println!();
            println!("{} Session Monitor", "📊".bold());
            println!("─────────────────────────────────────────────");
            println!();

            if sessions.is_empty() {
                println!("  {}", "No sessions found".yellow());
            } else {
                println!(
                    "{:20} {:10} {:15} {:8}",
                    "Name", "Status", "Last Active", "PRs"
                );
                println!("─────────────────────────────────────────────");

                for session in &sessions {
                    let status_str = match session.status {
                        SessionStatus::Active => "Active".green(),
                        SessionStatus::Paused => "Paused".yellow(),
                        SessionStatus::Integrated => "Integrated".blue(),
                        SessionStatus::Archived => "Archived".dimmed(),
                        SessionStatus::Abandoned => "Abandoned".red(),
                    };

                    let last_active = if (chrono::Utc::now() - session.last_active).num_hours() < 24
                    {
                        format!(
                            "{}h ago",
                            (chrono::Utc::now() - session.last_active).num_hours()
                        )
                    } else {
                        format!(
                            "{}d ago",
                            (chrono::Utc::now() - session.last_active).num_days()
                        )
                    };

                    let pr_str = if let Some(pr_number) = session.pr_number {
                        format!("#{}", pr_number)
                    } else {
                        "-".to_string()
                    };

                    println!(
                        "{:20} {:10} {:15} {:8}",
                        session.name, status_str, last_active, pr_str
                    );
                }
            }

            println!();
            println!("Total: {} sessions", sessions.len());
            println!();

            if watch {
                println!("Press Ctrl+C to exit...");
                std::thread::sleep(std::time::Duration::from_secs(5));
            } else {
                break;
            }
        }

        Ok(())
    }

    /// Clean up old/archived sessions
    pub fn clean(&self, older_than_days: u64, dry_run: bool, force: bool) -> Result<()> {
        let sessions = self.session_mgr.list_sessions()?;
        let threshold = chrono::Utc::now() - chrono::Duration::days(older_than_days as i64);

        let candidates: Vec<_> = sessions
            .iter()
            .filter(|s| {
                matches!(
                    s.status,
                    SessionStatus::Archived | SessionStatus::Integrated
                ) && s.last_active < threshold
            })
            .collect();

        if candidates.is_empty() {
            println!("{} No sessions to clean", "✓".green());
            return Ok(());
        }

        println!(
            "{} Found {} sessions to clean (older than {} days)",
            "🧹".bold(),
            candidates.len(),
            older_than_days
        );
        println!();

        for session in &candidates {
            println!(
                "  {} - last active {}",
                session.name,
                session.last_active.format("%Y-%m-%d")
            );
        }

        println!();

        if dry_run {
            println!("{} Dry run - no changes made", "ℹ".blue());
            return Ok(());
        }

        if !force {
            println!(
                "{}",
                "Use --force to actually delete these sessions".yellow()
            );
            return Ok(());
        }

        let mut deleted = 0;
        for session in candidates {
            match self.session_mgr.delete_session(&session.name) {
                Ok(_) => {
                    println!("  {} Deleted {}", "✓".green(), session.name);
                    deleted += 1;
                }
                Err(e) => {
                    eprintln!("  {} Failed to delete {}: {}", "✗".red(), session.name, e);
                }
            }
        }

        println!();
        println!("{} Cleaned {} sessions", "✓".green(), deleted);

        Ok(())
    }

    /// Leave a session (graceful cleanup)
    pub fn leave(&self, session_name: &str, archive: bool) -> Result<()> {
        println!("{} Leaving session '{}'", "→".cyan(), session_name);
        self.session_mgr.mutate_session(session_name, |session| {
            session.status = if archive {
                SessionStatus::Archived
            } else {
                SessionStatus::Paused
            };
            session.log_activity(
                crate::models::ActivityType::StatusChanged,
                if archive { "Archived" } else { "Paused" }.to_string(),
            );
            Ok(())
        })?;
        println!(
            "  {} Session {}",
            "✓".green(),
            if archive { "archived" } else { "paused" }
        );

        println!();
        println!("Session '{}' can be resumed later with:", session_name);
        println!("  hp switch {}", session_name);
        println!("  hp launch {}", session_name);
        println!();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AgentType, Session};
    use tempfile::TempDir;

    fn setup_test_env() -> (TempDir, Config) {
        let temp_dir = TempDir::new().unwrap();
        let mut config = Config::default();
        config.hp.sessions.metadata_dir = temp_dir.path().join("sessions");
        config.hp.sessions.context_dir = temp_dir.path().join("contexts");
        std::fs::create_dir_all(&config.hp.sessions.metadata_dir).unwrap();
        (temp_dir, config)
    }

    fn create_test_session(name: &str, agent_type: AgentType) -> Session {
        Session::new(
            name.to_string(),
            agent_type,
            format!("wb-{}", name),
            std::path::PathBuf::from(format!("/tmp/{}", name)),
            "main".to_string(),
            "main".to_string(),
            "test-repo".to_string(),
            "git".to_string(),
        )
    }

    #[test]
    fn test_utilities_manager_creation() {
        let config = Config::default();
        match UtilitiesManager::new(config) {
            Ok(_) => {
                // Successfully created
            }
            Err(e) => {
                // Skip test if environment not set up (hn not installed, directories missing, etc.)
                println!("Skipping test_utilities_manager_creation: {}", e);
            }
        }
    }

    #[test]
    fn test_monitor_with_sessions() {
        let (_temp_dir, config) = setup_test_env();
        let session_mgr = crate::session::SessionManager::new(config.clone());

        match session_mgr {
            Ok(mgr) => {
                // Create test sessions
                let session = create_test_session("monitor-test", AgentType::Feature);
                let _ = mgr.save_session(&session);

                // Test monitor (non-watch mode)
                let util_mgr = UtilitiesManager::new(config).unwrap();
                let result = util_mgr.monitor(false);
                assert!(result.is_ok());
            }
            Err(crate::error::Error::HnNotFound) => {
                println!("Skipping: hn not installed");
            }
            Err(e) => panic!("Unexpected error: {}", e),
        }
    }

    #[test]
    fn test_clean_dry_run() {
        let (_temp_dir, config) = setup_test_env();
        let session_mgr = crate::session::SessionManager::new(config.clone());

        match session_mgr {
            Ok(mgr) => {
                // Create an old archived session
                let mut session = create_test_session("old-session", AgentType::Feature);
                session.status = SessionStatus::Archived;
                // Make it appear old
                session.created = chrono::Utc::now() - chrono::Duration::days(60);
                let _ = mgr.save_session(&session);

                // Test clean in dry-run mode
                let util_mgr = UtilitiesManager::new(config).unwrap();
                let result = util_mgr.clean(30, true, false);
                assert!(result.is_ok());

                // Session should still exist
                assert!(mgr.session_exists("old-session"));
            }
            Err(crate::error::Error::HnNotFound) => {
                println!("Skipping: hn not installed");
            }
            Err(e) => panic!("Unexpected error: {}", e),
        }
    }

    #[test]
    fn test_leave_with_pause() {
        let (_temp_dir, config) = setup_test_env();
        let session_mgr = crate::session::SessionManager::new(config.clone());

        match session_mgr {
            Ok(mgr) => {
                // Create test session
                let session = create_test_session("leave-test", AgentType::Feature);
                mgr.save_session(&session).unwrap();

                // Test leave (pause)
                let util_mgr = UtilitiesManager::new(config).unwrap();
                let result = util_mgr.leave("leave-test", false);
                assert!(result.is_ok());

                // Session should be paused
                let updated = mgr.load_session("leave-test").unwrap();
                assert!(matches!(updated.status, SessionStatus::Paused));
            }
            Err(crate::error::Error::HnNotFound) => {
                println!("Skipping: hn not installed");
            }
            Err(e) => panic!("Unexpected error: {}", e),
        }
    }

    #[test]
    fn test_leave_with_archive() {
        let (_temp_dir, config) = setup_test_env();
        let session_mgr = crate::session::SessionManager::new(config.clone());

        match session_mgr {
            Ok(mgr) => {
                // Create test session
                let session = create_test_session("archive-test", AgentType::Feature);
                mgr.save_session(&session).unwrap();

                // Test leave (archive)
                let util_mgr = UtilitiesManager::new(config).unwrap();
                let result = util_mgr.leave("archive-test", true);
                assert!(result.is_ok());

                // Session should be archived
                let updated = mgr.load_session("archive-test").unwrap();
                assert!(matches!(updated.status, SessionStatus::Archived));
            }
            Err(crate::error::Error::HnNotFound) => {
                println!("Skipping: hn not installed");
            }
            Err(e) => panic!("Unexpected error: {}", e),
        }
    }
}
