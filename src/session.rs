//! Session management

use crate::config::Config;
use crate::error::{Error, Result};
use crate::hn_client::{HnClient, WorkboxOptions};
use crate::models::{ActivityType, AgentType, Session, SessionStatus};
use std::fs;
use std::path::{Path, PathBuf};

/// Session manager
#[allow(dead_code)]
pub struct SessionManager {
    config: Config,
    hn_client: HnClient,
    sessions_dir: PathBuf,
}

#[allow(dead_code)]
impl SessionManager {
    /// Create a new session manager
    pub fn new(config: Config) -> Result<Self> {
        let hn_client = HnClient::from_config(&config)?;
        let sessions_dir = config.hp.sessions.metadata_dir.clone();

        // Ensure sessions directory exists
        fs::create_dir_all(&sessions_dir)?;

        Ok(Self {
            config,
            hn_client,
            sessions_dir,
        })
    }

    /// Create a new session manager with custom client (for testing)
    pub fn with_client(config: Config, hn_client: HnClient) -> Result<Self> {
        let sessions_dir = config.hp.sessions.metadata_dir.clone();
        fs::create_dir_all(&sessions_dir)?;

        Ok(Self {
            config,
            hn_client,
            sessions_dir,
        })
    }

    /// Create a new session
    pub fn create_session(
        &self,
        name: &str,
        agent_type: AgentType,
        workbox_opts: WorkboxOptions,
    ) -> Result<Session> {
        // Validate session name
        self.validate_session_name(name)?;

        // Check if session already exists
        if self.session_exists(name) {
            return Err(Error::SessionAlreadyExists(name.to_string()));
        }

        // Create workbox via hannahanna
        let workbox_info = self.hn_client.create_workbox(name, &workbox_opts)?;

        // Get repository name from current directory
        let repo_name = self.get_repo_name()?;

        // Create session
        let mut session = Session::new(
            name.to_string(),
            agent_type,
            workbox_info.name.clone(),
            workbox_info.path,
            workbox_info.branch,
            workbox_info.base_branch,
            repo_name,
            workbox_info.vcs_type,
        );

        session.context_dir = self
            .config
            .hp
            .sessions
            .context_dir
            .join(&session.repo_name)
            .join(name);

        // Log activity
        session.log_activity(
            ActivityType::SessionCreated,
            format!("Created session '{}'", name),
        );

        // Save session
        self.save_session(&session)?;

        Ok(session)
    }

    /// Load a session by name
    pub fn load_session(&self, name: &str) -> Result<Session> {
        let session_path = self.get_session_path(name);

        if !session_path.exists() {
            return Err(Error::SessionNotFound(name.to_string()));
        }

        let content = fs::read_to_string(&session_path)
            .map_err(|e| Error::FileSystemError(format!("Failed to read session file: {}", e)))?;

        let mut session: Session = serde_yaml::from_str(&content)?;
        if session.context_dir.is_relative() && !self.config.repository_root.as_os_str().is_empty()
        {
            session.context_dir = self.config.repository_root.join(&session.context_dir);
        }

        Ok(session)
    }

    /// Save a session
    pub fn save_session(&self, session: &Session) -> Result<()> {
        let session_path = self.get_session_path(&session.name);

        let content = serde_yaml::to_string(session)?;

        fs::write(&session_path, content)
            .map_err(|e| Error::FileSystemError(format!("Failed to write session file: {}", e)))?;

        Ok(())
    }

    /// List all sessions
    pub fn list_sessions(&self) -> Result<Vec<Session>> {
        let mut sessions = Vec::new();

        let entries = fs::read_dir(&self.sessions_dir).map_err(|e| {
            Error::FileSystemError(format!("Failed to read sessions directory: {}", e))
        })?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("yaml") {
                let content = fs::read_to_string(&path)?;
                if let Ok(session) = serde_yaml::from_str::<Session>(&content) {
                    sessions.push(session);
                }
            }
        }

        // Sort by last active (most recent first)
        sessions.sort_by_key(|session| std::cmp::Reverse(session.last_active));

        Ok(sessions)
    }

    /// List sessions filtered by status
    pub fn list_sessions_by_status(&self, status: SessionStatus) -> Result<Vec<Session>> {
        let sessions = self.list_sessions()?;
        Ok(sessions
            .into_iter()
            .filter(|s| s.status == status)
            .collect())
    }

    /// List sessions filtered by agent type
    pub fn list_sessions_by_type(&self, agent_type: AgentType) -> Result<Vec<Session>> {
        let sessions = self.list_sessions()?;
        Ok(sessions
            .into_iter()
            .filter(|s| s.agent_type == agent_type)
            .collect())
    }

    /// Update a session
    pub fn update_session(&self, session: &Session) -> Result<()> {
        self.save_session(session)
    }

    /// Delete a session
    pub fn delete_session(&self, name: &str) -> Result<()> {
        let session_path = self.get_session_path(name);

        if !session_path.exists() {
            return Err(Error::SessionNotFound(name.to_string()));
        }

        fs::remove_file(&session_path)
            .map_err(|e| Error::FileSystemError(format!("Failed to delete session file: {}", e)))?;

        Ok(())
    }

    /// Check if a session exists
    pub fn session_exists(&self, name: &str) -> bool {
        self.get_session_path(name).exists()
    }

    /// Link a child session to a parent
    pub fn link_parent_child(&self, parent_name: &str, child_name: &str) -> Result<()> {
        // Load parent
        let mut parent = self.load_session(parent_name)?;

        // Load child
        let mut child = self.load_session(child_name)?;

        // Update relationships
        parent.add_child(child_name.to_string());
        child.parent = Some(parent_name.to_string());

        // Log activities
        parent.log_activity(
            ActivityType::ChildAdded,
            format!("Added child session '{}'", child_name),
        );
        child.log_activity(
            ActivityType::ParentLinked,
            format!("Linked to parent session '{}'", parent_name),
        );

        // Save both
        self.save_session(&parent)?;
        self.save_session(&child)?;

        Ok(())
    }

    /// Unlink a child session from its parent
    pub fn unlink_parent_child(&self, child_name: &str) -> Result<()> {
        // Load child
        let mut child = self.load_session(child_name)?;

        if let Some(parent_name) = child.parent.clone() {
            // Load parent
            let mut parent = self.load_session(&parent_name)?;

            // Remove relationship
            parent.remove_child(child_name);
            child.parent = None;

            // Save both
            self.save_session(&parent)?;
            self.save_session(&child)?;
        }

        Ok(())
    }

    /// Get children of a session
    pub fn get_children(&self, name: &str) -> Result<Vec<Session>> {
        let parent = self.load_session(name)?;

        let mut children = Vec::new();
        for child_name in &parent.children {
            if let Ok(child) = self.load_session(child_name) {
                children.push(child);
            }
        }

        Ok(children)
    }

    /// Get parent of a session
    pub fn get_parent(&self, name: &str) -> Result<Option<Session>> {
        let child = self.load_session(name)?;

        if let Some(parent_name) = &child.parent {
            Ok(Some(self.load_session(parent_name)?))
        } else {
            Ok(None)
        }
    }

    /// Get session tree (parent and all descendants)
    pub fn get_session_tree(&self, name: &str) -> Result<Vec<Session>> {
        let mut tree = Vec::new();
        let session = self.load_session(name)?;
        tree.push(session.clone());

        // Recursively get children
        for child_name in &session.children {
            if let Ok(child_tree) = self.get_session_tree(child_name) {
                tree.extend(child_tree);
            }
        }

        Ok(tree)
    }

    /// Close a session (mark as integrated/archived)
    pub fn close_session(
        &self,
        name: &str,
        status: SessionStatus,
        remove_workbox: bool,
    ) -> Result<()> {
        let mut session = self.load_session(name)?;

        // Remove workbox if requested
        if remove_workbox {
            self.hn_client
                .remove_workbox(&session.workbox_name, false)?;
        }

        // Update status
        session.status = status.clone();
        session.log_activity(
            ActivityType::StatusChanged,
            format!("Session closed with status: {:?}", status),
        );

        // Save session
        self.save_session(&session)?;

        Ok(())
    }

    /// Clone a session (for parallel work)
    pub fn clone_session(
        &self,
        name: &str,
        new_name: &str,
        new_agent_type: Option<AgentType>,
    ) -> Result<Session> {
        // Load original session
        let original = self.load_session(name)?;

        // Check if new name is available
        if self.session_exists(new_name) {
            return Err(Error::SessionAlreadyExists(new_name.to_string()));
        }

        // Create workbox options
        let opts = WorkboxOptions {
            from: Some(original.branch.clone()),
            vcs: Some(original.vcs_type.clone()),
            ..Default::default()
        };

        // Create new workbox
        let workbox_info = self.hn_client.create_workbox(new_name, &opts)?;

        // Create new session based on original
        let mut new_session = original.clone();
        new_session.id = uuid::Uuid::new_v4();
        new_session.name = new_name.to_string();
        new_session.workbox_name = workbox_info.name;
        new_session.workbox_path = workbox_info.path;
        new_session.branch = workbox_info.branch;
        new_session.parent = None; // Clones don't maintain parent relationship
        new_session.children.clear();
        new_session.created = chrono::Utc::now();
        new_session.last_active = chrono::Utc::now();
        new_session.activity_log.clear();
        new_session.context_dir = self
            .config
            .hp
            .sessions
            .context_dir
            .join(&new_session.repo_name)
            .join(new_name);

        // Override agent type if specified
        if let Some(agent_type) = new_agent_type {
            new_session.agent_type = agent_type;
        }

        // Log activity
        new_session.log_activity(
            ActivityType::SessionCreated,
            format!("Cloned from session '{}'", name),
        );

        // Save new session
        self.save_session(&new_session)?;

        Ok(new_session)
    }

    // === Private helpers ===

    fn get_session_path(&self, name: &str) -> PathBuf {
        self.sessions_dir.join(format!("{}.yaml", name))
    }

    fn validate_session_name(&self, name: &str) -> Result<()> {
        if name.is_empty() {
            return Err(Error::InvalidSessionName(
                name.to_string(),
                "Session name cannot be empty".to_string(),
            ));
        }

        if name.contains('/') || name.contains('\\') {
            return Err(Error::InvalidSessionName(
                name.to_string(),
                "Session name cannot contain slashes".to_string(),
            ));
        }

        Ok(())
    }

    fn get_repo_name(&self) -> Result<String> {
        if let Some(name) = self.config.repository_root.file_name() {
            return Ok(name.to_string_lossy().into_owned());
        }
        // Try to get from git
        let output = std::process::Command::new("git")
            .arg("rev-parse")
            .arg("--show-toplevel")
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout);
                let path = path.trim();
                if let Some(name) = Path::new(path).file_name() {
                    return Ok(name.to_string_lossy().to_string());
                }
            }
        }

        // Fallback: use current directory name
        let current_dir = std::env::current_dir()?;
        Ok(current_dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_manager() -> (SessionManager, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let mut config = Config::default();
        config.hp.sessions.metadata_dir = temp_dir.path().join("sessions");

        // Use a mock hn client for testing
        let hn_client = HnClient::with_command("echo".to_string());

        let manager = SessionManager::with_client(config, hn_client).unwrap();
        (manager, temp_dir)
    }

    #[test]
    fn test_validate_session_name() {
        let (manager, _temp) = create_test_manager();

        assert!(manager.validate_session_name("valid-name").is_ok());
        assert!(manager.validate_session_name("valid_name").is_ok());
        assert!(manager.validate_session_name("valid123").is_ok());

        assert!(manager.validate_session_name("").is_err());
        assert!(manager.validate_session_name("invalid/name").is_err());
        assert!(manager.validate_session_name("invalid\\name").is_err());
    }

    #[test]
    fn test_session_exists() {
        let (manager, _temp) = create_test_manager();

        assert!(!manager.session_exists("nonexistent"));

        // Create a session manually
        let session = Session::new(
            "test".to_string(),
            AgentType::Feature,
            "test".to_string(),
            PathBuf::from("/tmp/test"),
            "main".to_string(),
            "main".to_string(),
            "repo".to_string(),
            "git".to_string(),
        );

        manager.save_session(&session).unwrap();
        assert!(manager.session_exists("test"));
    }

    #[test]
    fn test_save_and_load_session() {
        let (manager, _temp) = create_test_manager();

        let session = Session::new(
            "test-session".to_string(),
            AgentType::Feature,
            "test-wb".to_string(),
            PathBuf::from("/tmp/test"),
            "feature/test".to_string(),
            "main".to_string(),
            "myrepo".to_string(),
            "git".to_string(),
        );

        // Save session
        manager.save_session(&session).unwrap();

        // Load session
        let loaded = manager.load_session("test-session").unwrap();

        assert_eq!(loaded.name, session.name);
        assert_eq!(loaded.id, session.id);
        assert_eq!(loaded.agent_type, session.agent_type);
    }

    #[test]
    fn test_list_sessions() {
        let (manager, _temp) = create_test_manager();

        // Create multiple sessions
        for i in 0..3 {
            let session = Session::new(
                format!("session-{}", i),
                AgentType::Feature,
                format!("wb-{}", i),
                PathBuf::from(format!("/tmp/test-{}", i)),
                "main".to_string(),
                "main".to_string(),
                "repo".to_string(),
                "git".to_string(),
            );
            manager.save_session(&session).unwrap();
        }

        let sessions = manager.list_sessions().unwrap();
        assert_eq!(sessions.len(), 3);
    }

    #[test]
    fn test_list_sessions_by_status() {
        let (manager, _temp) = create_test_manager();

        let mut session1 = Session::new(
            "active".to_string(),
            AgentType::Feature,
            "active".to_string(),
            PathBuf::from("/tmp/active"),
            "main".to_string(),
            "main".to_string(),
            "repo".to_string(),
            "git".to_string(),
        );
        session1.status = SessionStatus::Active;

        let mut session2 = Session::new(
            "paused".to_string(),
            AgentType::Feature,
            "paused".to_string(),
            PathBuf::from("/tmp/paused"),
            "main".to_string(),
            "main".to_string(),
            "repo".to_string(),
            "git".to_string(),
        );
        session2.status = SessionStatus::Paused;

        manager.save_session(&session1).unwrap();
        manager.save_session(&session2).unwrap();

        let active_sessions = manager
            .list_sessions_by_status(SessionStatus::Active)
            .unwrap();
        assert_eq!(active_sessions.len(), 1);
        assert_eq!(active_sessions[0].name, "active");
    }

    #[test]
    fn test_link_parent_child() {
        let (manager, _temp) = create_test_manager();

        let parent = Session::new(
            "parent".to_string(),
            AgentType::Feature,
            "parent".to_string(),
            PathBuf::from("/tmp/parent"),
            "main".to_string(),
            "main".to_string(),
            "repo".to_string(),
            "git".to_string(),
        );

        let child = Session::new(
            "child".to_string(),
            AgentType::Test,
            "child".to_string(),
            PathBuf::from("/tmp/child"),
            "main".to_string(),
            "main".to_string(),
            "repo".to_string(),
            "git".to_string(),
        );

        manager.save_session(&parent).unwrap();
        manager.save_session(&child).unwrap();

        // Link them
        manager.link_parent_child("parent", "child").unwrap();

        // Verify relationship
        let parent = manager.load_session("parent").unwrap();
        let child = manager.load_session("child").unwrap();

        assert_eq!(parent.children.len(), 1);
        assert_eq!(parent.children[0], "child");
        assert_eq!(child.parent, Some("parent".to_string()));
    }

    #[test]
    fn test_get_children() {
        let (manager, _temp) = create_test_manager();

        let parent = Session::new(
            "parent".to_string(),
            AgentType::Feature,
            "parent".to_string(),
            PathBuf::from("/tmp/parent"),
            "main".to_string(),
            "main".to_string(),
            "repo".to_string(),
            "git".to_string(),
        );

        let child1 = Session::new(
            "child1".to_string(),
            AgentType::Test,
            "child1".to_string(),
            PathBuf::from("/tmp/child1"),
            "main".to_string(),
            "main".to_string(),
            "repo".to_string(),
            "git".to_string(),
        );

        let child2 = Session::new(
            "child2".to_string(),
            AgentType::Docs,
            "child2".to_string(),
            PathBuf::from("/tmp/child2"),
            "main".to_string(),
            "main".to_string(),
            "repo".to_string(),
            "git".to_string(),
        );

        manager.save_session(&parent).unwrap();
        manager.save_session(&child1).unwrap();
        manager.save_session(&child2).unwrap();

        manager.link_parent_child("parent", "child1").unwrap();
        manager.link_parent_child("parent", "child2").unwrap();

        let children = manager.get_children("parent").unwrap();
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn test_delete_session() {
        let (manager, _temp) = create_test_manager();

        let session = Session::new(
            "delete-me".to_string(),
            AgentType::Feature,
            "delete-me".to_string(),
            PathBuf::from("/tmp/delete"),
            "main".to_string(),
            "main".to_string(),
            "repo".to_string(),
            "git".to_string(),
        );

        manager.save_session(&session).unwrap();
        assert!(manager.session_exists("delete-me"));

        manager.delete_session("delete-me").unwrap();
        assert!(!manager.session_exists("delete-me"));
    }
}
