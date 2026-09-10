//! PR integration for hupasiya
//!
//! Handles GitHub PR creation, syncing comments, and status tracking.

use crate::config::Config;
use crate::context::ContextManager;
use crate::error::{Error, Result};
use crate::hn_client::HnClient;
use crate::models::{ActivityType, PrStatus, ReviewComment, Session};
use crate::session::SessionManager;
use colored::Colorize;
use octocrab::Octocrab;
use std::env;

/// PR manager for GitHub integration
pub struct PrManager {
    #[allow(dead_code)]
    config: Config,
    session_mgr: SessionManager,
    #[allow(dead_code)]
    context_mgr: ContextManager,
    hn_client: HnClient,
}

impl PrManager {
    /// Create new PR manager
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

    /// Create GitHub PR for a session
    pub async fn create_pr(
        &self,
        session_name: &str,
        draft: bool,
        reviewers: Option<Vec<String>>,
        labels: Option<Vec<String>>,
        from_context: bool,
    ) -> Result<u64> {
        let mut session = self.session_mgr.load_session(session_name)?;

        // Get GitHub token
        let github_token = env::var("GITHUB_TOKEN")
            .map_err(|_| Error::Other("GITHUB_TOKEN not set".to_string()))?;

        // Get workbox info
        let workbox_info = self.hn_client.get_workbox_info(&session.workbox_name)?;

        // Parse repo from git remote
        let repo_info = self.get_repo_info(&workbox_info.path)?;

        // Build PR title and body
        let (title, body) = if from_context {
            self.build_pr_from_context(&session)?
        } else {
            (
                format!("{}: {}", session.agent_type.as_str(), session.name),
                format!("Automated PR for session: {}", session.name),
            )
        };

        // Create octocrab client
        let octocrab = Octocrab::builder().personal_token(github_token).build()?;

        // Push branch to remote
        println!("{} Pushing branch to remote...", "→".cyan());
        self.push_branch(&workbox_info.path, &session.branch)?;

        // Create PR
        println!("{} Creating PR...", "→".cyan());
        let pr = octocrab
            .pulls(&repo_info.owner, &repo_info.repo)
            .create(&title, &session.branch, &workbox_info.base_branch)
            .body(&body)
            .draft(draft)
            .send()
            .await
            .map_err(|e| Error::Other(format!("Failed to create PR: {}", e)))?;

        let pr_number = pr.number;

        // Add reviewers if specified
        if let Some(reviewers) = reviewers {
            println!("{} Requesting reviewers...", "→".cyan());
            octocrab
                .pulls(&repo_info.owner, &repo_info.repo)
                .request_reviews(pr_number, reviewers.clone(), vec![])
                .await
                .map_err(|e| Error::Other(format!("Failed to request reviewers: {}", e)))?;
        }

        // Add labels if specified
        if let Some(labels) = labels {
            println!("{} Adding labels...", "→".cyan());
            octocrab
                .issues(&repo_info.owner, &repo_info.repo)
                .add_labels(pr_number, &labels)
                .await
                .map_err(|e| Error::Other(format!("Failed to add labels: {}", e)))?;
        }

        // Update session with PR info
        session.pr_number = Some(pr_number);
        let pr_url = pr
            .html_url
            .as_ref()
            .ok_or_else(|| Error::Other("GitHub PR missing html_url".to_string()))?
            .to_string();
        session.pr_url = Some(pr_url.clone());
        session.pr_status = Some(if draft {
            PrStatus::Draft
        } else {
            PrStatus::Open
        });

        session.log_activity(
            ActivityType::PrCreated,
            format!("Created PR #{}", pr_number),
        );
        self.session_mgr.save_session(&session)?;

        println!();
        println!("{} PR created successfully!", "✓".green());
        println!("  PR #{}: {}", pr_number, pr_url);
        println!();

        Ok(pr_number)
    }

    /// Sync PR comments to session context
    pub async fn sync_pr(&self, session_name: &str, create_shepherd_tasks: bool) -> Result<()> {
        let mut session = self.session_mgr.load_session(session_name)?;

        let pr_number = session
            .pr_number
            .ok_or_else(|| Error::Other("Session has no associated PR".to_string()))?;

        // Get GitHub token
        let github_token = env::var("GITHUB_TOKEN")
            .map_err(|_| Error::Other("GITHUB_TOKEN not set".to_string()))?;

        // Get workbox info
        let workbox_info = self.hn_client.get_workbox_info(&session.workbox_name)?;

        // Parse repo from git remote
        let repo_info = self.get_repo_info(&workbox_info.path)?;

        // Create octocrab client
        let octocrab = Octocrab::builder().personal_token(github_token).build()?;

        println!("{} Fetching PR comments...", "→".cyan());

        // Fetch review comments
        let comments = octocrab
            .pulls(&repo_info.owner, &repo_info.repo)
            .list_comments(Some(pr_number))
            .send()
            .await
            .map_err(|e| Error::Other(format!("Failed to fetch comments: {}", e)))?;

        // Filter unresolved comments
        let unresolved_comments: Vec<ReviewComment> = comments
            .items
            .iter()
            .filter(|c| !self.is_comment_resolved(c))
            .map(|c| self.octocrab_comment_to_review_comment(c))
            .collect();

        let unresolved_count = unresolved_comments.len();

        println!(
            "{} Found {} unresolved comments",
            "✓".green(),
            unresolved_count
        );

        // Update session
        session.unresolved_comments = unresolved_comments.clone();
        session.log_activity(
            ActivityType::PrSynced,
            format!("Synced {} unresolved comments", unresolved_count),
        );
        self.session_mgr.save_session(&session)?;

        // Write comments to shepherd.md if requested
        if create_shepherd_tasks && !unresolved_comments.is_empty() {
            println!("{} Creating shepherd tasks...", "→".cyan());
            self.write_shepherd_file(&session, &unresolved_comments)?;
            println!(
                "{} Shepherd file created at .hp/contexts/{}/shepherd.md",
                "✓".green(),
                session_name
            );
        }

        Ok(())
    }

    /// Show PR status
    pub async fn pr_status(&self, session_name: &str) -> Result<()> {
        let session = self.session_mgr.load_session(session_name)?;

        let pr_number = session
            .pr_number
            .ok_or_else(|| Error::Other("Session has no associated PR".to_string()))?;
        let pr_url = session
            .pr_url
            .as_ref()
            .ok_or_else(|| Error::Other("Session has no PR URL".to_string()))?;

        // Get GitHub token
        let github_token = env::var("GITHUB_TOKEN")
            .map_err(|_| Error::Other("GITHUB_TOKEN not set".to_string()))?;

        // Get workbox info
        let workbox_info = self.hn_client.get_workbox_info(&session.workbox_name)?;

        // Parse repo from git remote
        let repo_info = self.get_repo_info(&workbox_info.path)?;

        // Create octocrab client
        let octocrab = Octocrab::builder().personal_token(github_token).build()?;

        // Fetch PR
        let pr = octocrab
            .pulls(&repo_info.owner, &repo_info.repo)
            .get(pr_number)
            .await
            .map_err(|e| Error::Other(format!("Failed to fetch PR: {}", e)))?;

        // Display status
        println!();
        println!(
            "{} PR #{}: {}",
            "🔗".bold(),
            pr_number,
            pr.title.unwrap_or_default()
        );
        println!("   URL: {}", pr_url);
        println!(
            "   State: {}",
            format!(
                "{:?}",
                pr.state
                    .ok_or_else(|| Error::Other("PR missing state".to_string()))?
            )
            .to_uppercase()
        );
        println!(
            "   Created: {}",
            session.created.format("%Y-%m-%d %H:%M:%S")
        );
        println!(
            "   Unresolved comments: {}",
            session.unresolved_comments.len()
        );

        if let Some(mergeable) = pr.mergeable {
            let mergeable_status = if mergeable {
                "Yes ✓".green()
            } else {
                "No ✗".red()
            };
            println!("   Mergeable: {}", mergeable_status);
        }

        println!();

        Ok(())
    }

    // === Private helper methods ===

    fn get_repo_info(&self, workbox_path: &std::path::Path) -> Result<RepoInfo> {
        // Get git remote URL
        let output = std::process::Command::new("git")
            .args(["remote", "get-url", "origin"])
            .current_dir(workbox_path)
            .output()
            .map_err(|e| Error::Other(format!("Failed to get git remote: {}", e)))?;

        if !output.status.success() {
            return Err(Error::Other("Failed to get git remote URL".to_string()));
        }

        let remote_url = String::from_utf8_lossy(&output.stdout).trim().to_string();

        // Parse GitHub URL (supports both HTTPS and SSH)
        // Examples:
        //   https://github.com/owner/repo.git
        //   git@github.com:owner/repo.git
        let (owner, repo) = if remote_url.starts_with("https://") {
            let parts: Vec<&str> = remote_url
                .trim_start_matches("https://github.com/")
                .trim_end_matches(".git")
                .split('/')
                .collect();
            if parts.len() != 2 {
                return Err(Error::Other("Invalid GitHub URL format".to_string()));
            }
            (parts[0].to_string(), parts[1].to_string())
        } else if remote_url.starts_with("git@github.com:") {
            let parts: Vec<&str> = remote_url
                .trim_start_matches("git@github.com:")
                .trim_end_matches(".git")
                .split('/')
                .collect();
            if parts.len() != 2 {
                return Err(Error::Other("Invalid GitHub URL format".to_string()));
            }
            (parts[0].to_string(), parts[1].to_string())
        } else {
            return Err(Error::Other(
                "Unsupported git remote URL format".to_string(),
            ));
        };

        Ok(RepoInfo { owner, repo })
    }

    fn push_branch(&self, workbox_path: &std::path::Path, branch: &str) -> Result<()> {
        let output = std::process::Command::new("git")
            .args(["push", "-u", "origin", branch])
            .current_dir(workbox_path)
            .output()
            .map_err(|e| Error::Other(format!("Failed to push branch: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Other(format!("Git push failed: {}", stderr)));
        }

        Ok(())
    }

    fn build_pr_from_context(&self, session: &Session) -> Result<(String, String)> {
        // Read context file
        let context_path = session.context_dir.join("context.md");
        let context = std::fs::read_to_string(&context_path)
            .map_err(|e| Error::Other(format!("Failed to read context: {}", e)))?;

        // Parse objectives and deliverables from context
        let title = format!("{}: {}", session.agent_type.as_str(), session.name);
        let body = format!("## Session: {}\n\n{}", session.name, context);

        Ok((title, body))
    }

    fn is_comment_resolved(&self, comment: &octocrab::models::pulls::Comment) -> bool {
        // A comment is considered resolved if it has a reply from the author
        // or if it's marked as resolved in some way
        // For simplicity, we'll consider comments without in_reply_to as unresolved
        comment.in_reply_to_id.is_some()
    }

    fn octocrab_comment_to_review_comment(
        &self,
        comment: &octocrab::models::pulls::Comment,
    ) -> ReviewComment {
        ReviewComment {
            id: comment.id.0,
            path: comment.path.clone(),
            line: comment.line.map(|l| l as u32),
            body: comment.body.clone(),
            author: comment
                .user
                .as_ref()
                .map(|u| u.login.clone())
                .unwrap_or_default(),
            created_at: comment.created_at,
            resolved: comment.in_reply_to_id.is_some(),
            diff_hunk: Some(comment.diff_hunk.clone()),
        }
    }

    fn write_shepherd_file(&self, session: &Session, comments: &[ReviewComment]) -> Result<()> {
        let mut content = String::new();
        content.push_str("# PR Comment Resolution\n\n");
        content.push_str(&format!("Session: {}\n", session.name));
        content.push_str(&format!("PR: #{}\n\n", session.pr_number.unwrap()));
        content.push_str("## Unresolved Comments\n\n");

        for (i, comment) in comments.iter().enumerate() {
            content.push_str(&format!("### Comment {} (ID: {})\n\n", i + 1, comment.id));
            content.push_str(&format!("**Author:** {}\n", comment.author));
            content.push_str(&format!("**File:** {}\n", comment.path));
            if let Some(line) = comment.line {
                content.push_str(&format!("**Line:** {}\n", line));
            }
            content.push_str(&format!(
                "**Created:** {}\n\n",
                comment.created_at.format("%Y-%m-%d %H:%M:%S")
            ));
            content.push_str(&format!("**Comment:**\n{}\n\n", comment.body));
            content.push_str("**Suggested Action:** [FIX|CLARIFY|ACKNOWLEDGE|DEFER|DISAGREE]\n\n");
            content.push_str("**Confidence:** [HIGH|MEDIUM|LOW]\n\n");
            content.push_str("**Response:**\n\n");
            content.push_str("---\n\n");
        }

        let shepherd_path = session.context_dir.join("shepherd.md");
        std::fs::write(&shepherd_path, content)
            .map_err(|e| Error::Other(format!("Failed to write shepherd file: {}", e)))?;

        Ok(())
    }
}

struct RepoInfo {
    owner: String,
    repo: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pr_manager_creation() {
        let config = Config::default();
        match PrManager::new(config) {
            Ok(_) => {
                // Successfully created PR manager
            }
            Err(e) => {
                // Skip test if environment not set up (hn not installed, directories missing, etc.)
                println!("Skipping test_pr_manager_creation: {}", e);
            }
        }
    }

    #[test]
    fn test_parse_github_url_https() {
        // Test parsing logic without creating PrManager
        let url = "https://github.com/owner/repo.git";
        let parts: Vec<&str> = url
            .trim_start_matches("https://github.com/")
            .trim_end_matches(".git")
            .split('/')
            .collect();
        assert_eq!(parts, vec!["owner", "repo"]);
    }

    #[test]
    fn test_parse_github_url_ssh() {
        // Test parsing logic without creating PrManager
        let url = "git@github.com:owner/repo.git";
        let parts: Vec<&str> = url
            .trim_start_matches("git@github.com:")
            .trim_end_matches(".git")
            .split('/')
            .collect();
        assert_eq!(parts, vec!["owner", "repo"]);
    }
}
