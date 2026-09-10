//! Data models for hupasiya

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// Type of AI agent
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AgentType {
    /// Building new feature
    Feature,
    /// Fixing bugs
    Bugfix,
    /// Code review
    Review,
    /// Investigation/spike
    Research,
    /// Refactoring code
    Refactor,
    /// Writing tests
    Test,
    /// Documentation
    Docs,
    /// PR comment resolution
    Shepherd,
    /// Custom agent type
    Custom(String),
}

#[allow(dead_code)]
impl AgentType {
    /// Parse agent type from string
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "feature" => Ok(Self::Feature),
            "bugfix" => Ok(Self::Bugfix),
            "review" => Ok(Self::Review),
            "research" => Ok(Self::Research),
            "refactor" => Ok(Self::Refactor),
            "test" => Ok(Self::Test),
            "docs" => Ok(Self::Docs),
            "shepherd" => Ok(Self::Shepherd),
            other => Ok(Self::Custom(other.to_string())),
        }
    }

    /// Get default context template for this agent type
    pub fn default_template(&self) -> &str {
        match self {
            Self::Feature => "feature",
            Self::Bugfix => "bugfix",
            Self::Review => "review",
            Self::Research => "research",
            Self::Refactor => "refactor",
            Self::Test => "test",
            Self::Docs => "docs",
            Self::Shepherd => "shepherd",
            Self::Custom(name) => name,
        }
    }

    /// Convert to string
    pub fn as_str(&self) -> &str {
        match self {
            Self::Feature => "feature",
            Self::Bugfix => "bugfix",
            Self::Review => "review",
            Self::Research => "research",
            Self::Refactor => "refactor",
            Self::Test => "test",
            Self::Docs => "docs",
            Self::Shepherd => "shepherd",
            Self::Custom(name) => name,
        }
    }
}

/// Current status of a session
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    /// Session is actively being worked on
    Active,
    /// Session is paused
    Paused,
    /// Work has been integrated into base branch
    Integrated,
    /// Session has been archived
    Archived,
    /// Session has been abandoned
    Abandoned,
}

/// Session metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    // === Identity ===
    /// Unique session identifier
    pub id: Uuid,
    /// Human-readable session name
    pub name: String,
    /// Timestamp when session was created
    pub created: DateTime<Utc>,
    /// Timestamp of last activity
    pub last_active: DateTime<Utc>,

    // === Type & Status ===
    /// Type of agent
    pub agent_type: AgentType,
    /// Current status
    pub status: SessionStatus,

    // === Workbox (managed by hannahanna) ===
    /// Workbox name (used for hn commands)
    pub workbox_name: String,
    /// Cached workbox path
    pub workbox_path: PathBuf,
    /// Branch name
    pub branch: String,
    /// Base branch
    pub base_branch: String,
    /// Repository name
    pub repo_name: String,
    /// VCS type: "git", "hg", or "jj"
    pub vcs_type: String,

    // === Relationships (Multi-agent) ===
    /// Parent session name (if child)
    pub parent: Option<String>,
    /// Child session names
    pub children: Vec<String>,

    // === Context (managed by hupasiya) ===
    /// Context directory path
    pub context_dir: PathBuf,
    /// Context snapshots
    pub context_snapshots: Vec<SnapshotInfo>,

    // === PR Integration ===
    /// Associated PR number
    pub pr_number: Option<u64>,
    /// PR URL
    pub pr_url: Option<String>,
    /// PR status
    pub pr_status: Option<PrStatus>,
    /// Unresolved PR review comments
    pub unresolved_comments: Vec<ReviewComment>,

    // === Activity & Metrics ===
    /// Activity log
    pub activity_log: Vec<ActivityEvent>,
    /// Session metrics
    pub metrics: SessionMetrics,

    // === Metadata ===
    /// User-defined tags
    pub tags: Vec<String>,
    /// User notes
    pub notes: String,
    /// Lock info (username@hostname)
    pub locked_by: Option<String>,
}

#[allow(dead_code)]
impl Session {
    /// Create a new session
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        agent_type: AgentType,
        workbox_name: String,
        workbox_path: PathBuf,
        branch: String,
        base_branch: String,
        repo_name: String,
        vcs_type: String,
    ) -> Self {
        let now = Utc::now();
        let context_dir = PathBuf::from(format!(".hp/contexts/{}/{}", repo_name, name));

        Self {
            id: Uuid::new_v4(),
            name,
            created: now,
            last_active: now,
            agent_type,
            status: SessionStatus::Active,
            workbox_name,
            workbox_path,
            branch,
            base_branch,
            repo_name,
            vcs_type,
            parent: None,
            children: Vec::new(),
            context_dir,
            context_snapshots: Vec::new(),
            pr_number: None,
            pr_url: None,
            pr_status: None,
            unresolved_comments: Vec::new(),
            activity_log: Vec::new(),
            metrics: SessionMetrics::default(),
            tags: Vec::new(),
            notes: String::new(),
            locked_by: None,
        }
    }

    /// Log an activity event
    pub fn log_activity(&mut self, event_type: ActivityType, details: String) {
        self.activity_log.push(ActivityEvent {
            timestamp: Utc::now(),
            event_type,
            details,
        });
        self.touch();
    }

    /// Update last active timestamp
    pub fn touch(&mut self) {
        self.last_active = Utc::now();
    }

    /// Check if session is locked
    pub fn is_locked(&self) -> bool {
        self.locked_by.is_some()
    }

    /// Add a child session
    pub fn add_child(&mut self, child_name: String) {
        if !self.children.contains(&child_name) {
            self.children.push(child_name);
        }
    }

    /// Remove a child session
    pub fn remove_child(&mut self, child_name: &str) {
        self.children.retain(|c| c != child_name);
    }
}

/// Information about a workbox from hannahanna
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkboxInfo {
    /// Workbox name
    pub name: String,
    /// Path to workbox
    pub path: PathBuf,
    /// Current branch
    pub branch: String,
    /// Base branch
    #[serde(default)]
    pub base_branch: String,
    /// VCS type
    pub vcs_type: String,
    /// Current commit
    pub commit: String,
}

/// Snapshot metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotInfo {
    /// Snapshot name
    pub name: String,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// File path
    pub path: PathBuf,
    /// Description
    pub description: Option<String>,
    /// Trigger that created this snapshot
    pub trigger: SnapshotTrigger,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotTrigger {
    Manual,
    BeforeCascade,
    BeforeGather,
    AfterPrReview,
    OnPause,
}

/// Activity event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityEvent {
    /// When the event occurred
    pub timestamp: DateTime<Utc>,
    /// Type of event
    pub event_type: ActivityType,
    /// Event details
    pub details: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ActivityType {
    SessionCreated,
    ContextEdited,
    AiLaunched,
    CommitMade,
    PrCreated,
    PrSynced,
    PrCommentReceived,
    ShepherdRun,
    Cascaded,
    Gathered,
    Integrated,
    StatusChanged,
    ParentLinked,
    ChildAdded,
}

/// Session metrics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionMetrics {
    /// Total active time in seconds
    pub total_time_secs: i64,
    /// Number of commits
    pub commits: u32,
    /// Lines added
    pub lines_added: u32,
    /// Lines removed
    pub lines_removed: u32,
    /// Files changed
    pub files_changed: u32,
    /// Number of AI interactions
    pub ai_interactions: u32,
    /// Total tokens used
    pub tokens_used: u64,
}

#[allow(dead_code)]
impl SessionMetrics {
    /// Update metrics from git diff stats
    pub fn update_from_git_stats(&mut self, added: u32, removed: u32, files: u32) {
        self.lines_added += added;
        self.lines_removed += removed;
        self.files_changed = files;
    }

    /// Record AI interaction
    pub fn record_ai_interaction(&mut self, tokens: u64) {
        self.ai_interactions += 1;
        self.tokens_used += tokens;
    }

    /// Add time
    pub fn add_time(&mut self, duration_secs: i64) {
        self.total_time_secs += duration_secs;
    }

    /// Increment commit count
    pub fn increment_commits(&mut self) {
        self.commits += 1;
    }
}

/// PR status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PrStatus {
    Open,
    Draft,
    Merged,
    Closed,
}

/// PR review comment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewComment {
    /// Comment ID
    pub id: u64,
    /// File path
    pub path: String,
    /// Line number
    pub line: Option<u32>,
    /// Comment body
    pub body: String,
    /// Author
    pub author: String,
    /// Timestamp
    pub created_at: DateTime<Utc>,
    /// Is resolved
    pub resolved: bool,
    /// Diff hunk
    pub diff_hunk: Option<String>,
}

/// Shepherd analysis of a PR comment
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct ShepherdAnalysis {
    /// Comment ID
    pub comment_id: u64,
    /// Recommended action
    pub action: ShepherdAction,
    /// Confidence level
    pub confidence: ConfidenceLevel,
    /// Summary
    pub summary: String,
    /// Assessment
    pub assessment: String,
    /// Code changes (if action is Fix)
    pub changes: Option<String>,
    /// Response to post
    pub response: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum ShepherdAction {
    /// Fix the issue with code changes
    Fix,
    /// Clarify requirements
    Clarify,
    /// Acknowledge without changes
    Acknowledge,
    /// Defer to later
    Defer,
    /// Disagree with comment
    Disagree,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ConfidenceLevel {
    High,
    Medium,
    Low,
}

impl ConfidenceLevel {
    pub fn as_str(&self) -> &str {
        match self {
            Self::High => "HIGH",
            Self::Medium => "MEDIUM",
            Self::Low => "LOW",
        }
    }
}

/// Template metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct Template {
    /// Template name
    pub name: String,
    /// Template content
    pub content: String,
    /// Template variables
    pub variables: Vec<String>,
    /// Template metadata
    pub metadata: TemplateMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct TemplateMetadata {
    /// Author
    pub author: String,
    /// Version
    pub version: String,
    /// Description
    pub description: String,
    /// Agent types this template is for
    pub agent_types: Vec<AgentType>,
    /// Tags
    pub tags: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_type_from_str() {
        assert_eq!(AgentType::from_str("feature").unwrap(), AgentType::Feature);
        assert_eq!(AgentType::from_str("BUGFIX").unwrap(), AgentType::Bugfix);
        assert_eq!(AgentType::from_str("test").unwrap(), AgentType::Test);

        // Custom type
        let custom = AgentType::from_str("my-custom-type").unwrap();
        assert_eq!(custom, AgentType::Custom("my-custom-type".to_string()));
    }

    #[test]
    fn test_agent_type_default_template() {
        assert_eq!(AgentType::Feature.default_template(), "feature");
        assert_eq!(AgentType::Bugfix.default_template(), "bugfix");
        assert_eq!(AgentType::Test.default_template(), "test");

        let custom = AgentType::Custom("mytype".to_string());
        assert_eq!(custom.default_template(), "mytype");
    }

    #[test]
    fn test_agent_type_as_str() {
        assert_eq!(AgentType::Feature.as_str(), "feature");
        assert_eq!(AgentType::Shepherd.as_str(), "shepherd");
    }

    #[test]
    fn test_session_creation() {
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

        assert_eq!(session.name, "test-session");
        assert_eq!(session.agent_type, AgentType::Feature);
        assert_eq!(session.status, SessionStatus::Active);
        assert_eq!(session.workbox_name, "test-wb");
        assert_eq!(session.branch, "feature/test");
        assert_eq!(session.base_branch, "main");
        assert_eq!(session.repo_name, "myrepo");
        assert_eq!(session.vcs_type, "git");
        assert!(session.parent.is_none());
        assert!(session.children.is_empty());
        assert!(!session.is_locked());
    }

    #[test]
    fn test_session_touch() {
        let mut session = Session::new(
            "test".to_string(),
            AgentType::Feature,
            "test".to_string(),
            PathBuf::from("/tmp/test"),
            "main".to_string(),
            "main".to_string(),
            "repo".to_string(),
            "git".to_string(),
        );

        let original_time = session.last_active;
        std::thread::sleep(std::time::Duration::from_millis(10));
        session.touch();

        assert!(session.last_active > original_time);
    }

    #[test]
    fn test_session_locking() {
        let mut session = Session::new(
            "test".to_string(),
            AgentType::Feature,
            "test".to_string(),
            PathBuf::from("/tmp/test"),
            "main".to_string(),
            "main".to_string(),
            "repo".to_string(),
            "git".to_string(),
        );

        assert!(!session.is_locked());

        session.locked_by = Some("alice@laptop".to_string());
        assert!(session.is_locked());

        session.locked_by = None;
        assert!(!session.is_locked());
    }

    #[test]
    fn test_session_add_child() {
        let mut parent = Session::new(
            "parent".to_string(),
            AgentType::Feature,
            "parent".to_string(),
            PathBuf::from("/tmp/parent"),
            "main".to_string(),
            "main".to_string(),
            "repo".to_string(),
            "git".to_string(),
        );

        parent.add_child("child1".to_string());
        assert_eq!(parent.children.len(), 1);
        assert_eq!(parent.children[0], "child1");

        parent.add_child("child2".to_string());
        assert_eq!(parent.children.len(), 2);

        // Adding duplicate should not increase count
        parent.add_child("child1".to_string());
        assert_eq!(parent.children.len(), 2);
    }

    #[test]
    fn test_session_remove_child() {
        let mut parent = Session::new(
            "parent".to_string(),
            AgentType::Feature,
            "parent".to_string(),
            PathBuf::from("/tmp/parent"),
            "main".to_string(),
            "main".to_string(),
            "repo".to_string(),
            "git".to_string(),
        );

        parent.add_child("child1".to_string());
        parent.add_child("child2".to_string());
        assert_eq!(parent.children.len(), 2);

        parent.remove_child("child1");
        assert_eq!(parent.children.len(), 1);
        assert_eq!(parent.children[0], "child2");
    }

    #[test]
    fn test_session_serialization() {
        let session = Session::new(
            "test".to_string(),
            AgentType::Feature,
            "test".to_string(),
            PathBuf::from("/tmp/test"),
            "feature/test".to_string(),
            "main".to_string(),
            "myrepo".to_string(),
            "git".to_string(),
        );

        // Serialize to YAML
        let yaml = serde_yaml::to_string(&session).unwrap();
        assert!(yaml.contains("name: test"));
        assert!(yaml.contains("agent_type: feature"));

        // Deserialize back
        let deserialized: Session = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(deserialized.name, session.name);
        assert_eq!(deserialized.id, session.id);
    }

    #[test]
    fn test_session_log_activity() {
        let mut session = Session::new(
            "test".to_string(),
            AgentType::Feature,
            "test".to_string(),
            PathBuf::from("/tmp/test"),
            "main".to_string(),
            "main".to_string(),
            "repo".to_string(),
            "git".to_string(),
        );

        session.log_activity(ActivityType::CommitMade, "Initial commit".to_string());
        assert_eq!(session.activity_log.len(), 1);
        assert_eq!(session.activity_log[0].event_type, ActivityType::CommitMade);
        assert_eq!(session.activity_log[0].details, "Initial commit");
    }

    #[test]
    fn test_session_metrics() {
        let mut metrics = SessionMetrics::default();
        assert_eq!(metrics.commits, 0);
        assert_eq!(metrics.lines_added, 0);

        metrics.update_from_git_stats(100, 50, 5);
        assert_eq!(metrics.lines_added, 100);
        assert_eq!(metrics.lines_removed, 50);
        assert_eq!(metrics.files_changed, 5);

        metrics.record_ai_interaction(1234);
        assert_eq!(metrics.ai_interactions, 1);
        assert_eq!(metrics.tokens_used, 1234);

        metrics.increment_commits();
        assert_eq!(metrics.commits, 1);

        metrics.add_time(3600);
        assert_eq!(metrics.total_time_secs, 3600);
    }

    #[test]
    fn test_pr_status() {
        let status = PrStatus::Open;
        let yaml = serde_yaml::to_string(&status).unwrap();
        assert_eq!(yaml.trim(), "open");
    }

    #[test]
    fn test_shepherd_action() {
        let action = ShepherdAction::Fix;
        let yaml = serde_yaml::to_string(&action).unwrap();
        assert_eq!(yaml.trim(), "FIX");
    }

    #[test]
    fn test_confidence_level() {
        let level = ConfidenceLevel::High;
        let yaml = serde_yaml::to_string(&level).unwrap();
        assert_eq!(yaml.trim(), "high");
    }
}
