//! Context management for sessions

use crate::config::Config;
use crate::error::{Error, Result};
use crate::models::{AgentType, Session, SnapshotInfo, SnapshotTrigger};
use chrono::Utc;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Context manager
#[allow(dead_code)]
pub struct ContextManager {
    config: Config,
    context_base_dir: PathBuf,
}

impl ContextManager {
    /// Create a new context manager
    pub fn new(config: Config) -> Result<Self> {
        let context_base_dir = config.hp.sessions.context_dir.clone();
        fs::create_dir_all(&context_base_dir)?;

        Ok(Self {
            config,
            context_base_dir,
        })
    }

    /// Initialize context directory for a session
    pub fn init_context(&self, session: &Session) -> Result<()> {
        let context_dir = &session.context_dir;

        // Create context directory structure
        fs::create_dir_all(context_dir)?;
        fs::create_dir_all(context_dir.join("snapshots"))?;

        // Create initial context file from template
        let template = self.get_template_content(&session.agent_type)?;
        let context_content = self.substitute_variables(&template, session)?;

        fs::write(context_dir.join("context.md"), context_content)?;

        // Create empty files
        fs::write(context_dir.join("activity.json"), "[]")?;
        fs::write(context_dir.join("metrics.json"), "{}")?;

        Ok(())
    }

    /// Get context file path for a session
    pub fn get_context_path(&self, session: &Session) -> PathBuf {
        session.context_dir.join("context.md")
    }

    /// Read context for a session
    pub fn read_context(&self, session: &Session) -> Result<String> {
        let context_path = self.get_context_path(session);

        if !context_path.exists() {
            return Err(Error::ContextNotFound(context_path));
        }

        let content = fs::read_to_string(&context_path)?;
        Ok(content)
    }

    /// Write context for a session
    pub fn write_context(&self, session: &Session, content: &str) -> Result<()> {
        let context_path = self.get_context_path(session);
        fs::write(&context_path, content)?;
        Ok(())
    }

    /// Edit context in user's editor
    pub fn edit_context(&self, session: &Session) -> Result<()> {
        let context_path = self.get_context_path(session);

        // Get editor from environment or use default
        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());

        // Launch editor
        let status = Command::new(&editor).arg(&context_path).status()?;

        if !status.success() {
            return Err(Error::Other(format!(
                "Editor '{}' exited with error",
                editor
            )));
        }

        Ok(())
    }

    /// Create a snapshot of the current context
    pub fn create_snapshot(
        &self,
        session: &Session,
        name: &str,
        trigger: SnapshotTrigger,
        description: Option<String>,
    ) -> Result<SnapshotInfo> {
        let snapshots_dir = session.context_dir.join("snapshots");
        fs::create_dir_all(&snapshots_dir)?;

        // Generate snapshot filename
        let timestamp = Utc::now();
        let filename = format!("{}_{}.md", timestamp.format("%Y%m%d_%H%M%S"), name);
        let snapshot_path = snapshots_dir.join(&filename);

        // Copy current context to snapshot
        let context_content = self.read_context(session)?;
        fs::write(&snapshot_path, context_content)?;

        let snapshot = SnapshotInfo {
            name: name.to_string(),
            timestamp,
            path: snapshot_path,
            description,
            trigger,
        };

        Ok(snapshot)
    }

    /// List all snapshots for a session
    pub fn list_snapshots(&self, session: &Session) -> Result<Vec<SnapshotInfo>> {
        let snapshots_dir = session.context_dir.join("snapshots");

        if !snapshots_dir.exists() {
            return Ok(Vec::new());
        }

        let mut snapshots = Vec::new();

        for entry in fs::read_dir(&snapshots_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                // Parse filename to extract info
                if let Some(filename) = path.file_stem().and_then(|s| s.to_str()) {
                    // Expected format: YYYYMMDD_HHMMSS_name
                    let parts: Vec<&str> = filename.splitn(3, '_').collect();
                    if parts.len() >= 3 {
                        let name = parts[2].to_string();
                        let timestamp = Utc::now(); // In production, parse from filename

                        snapshots.push(SnapshotInfo {
                            name,
                            timestamp,
                            path: path.clone(),
                            description: None,
                            trigger: SnapshotTrigger::Manual,
                        });
                    }
                }
            }
        }

        // Sort by timestamp (newest first)
        snapshots.sort_by_key(|snapshot| std::cmp::Reverse(snapshot.timestamp));

        Ok(snapshots)
    }

    /// Restore a snapshot
    pub fn restore_snapshot(&self, session: &Session, snapshot_name: &str) -> Result<()> {
        let snapshots = self.list_snapshots(session)?;

        let snapshot = snapshots
            .iter()
            .find(|s| s.name == snapshot_name)
            .ok_or_else(|| Error::Other(format!("Snapshot '{}' not found", snapshot_name)))?;

        // Read snapshot content
        let snapshot_content = fs::read_to_string(&snapshot.path)?;

        // Write to current context
        self.write_context(session, &snapshot_content)?;

        Ok(())
    }

    /// Sync context from one session to another
    pub fn sync_context(&self, from_session: &Session, to_session: &Session) -> Result<()> {
        let from_content = self.read_context(from_session)?;
        self.write_context(to_session, &from_content)?;
        Ok(())
    }

    /// Get default template content for an agent type
    fn get_template_content(&self, agent_type: &AgentType) -> Result<String> {
        // For now, use built-in templates
        // In production, would load from .hp/templates/
        Ok(get_builtin_template(agent_type))
    }

    /// Substitute variables in template
    fn substitute_variables(&self, template: &str, session: &Session) -> Result<String> {
        let vars: HashMap<&str, String> = [
            ("session_name", session.name.clone()),
            ("session_type", session.agent_type.as_str().to_string()),
            ("repo_name", session.repo_name.clone()),
            ("branch", session.branch.clone()),
            ("base_branch", session.base_branch.clone()),
            ("vcs_type", session.vcs_type.clone()),
            ("created_at", session.created.to_rfc3339()),
        ]
        .iter()
        .map(|(k, v)| (*k, v.clone()))
        .collect();

        let mut result = template.to_string();
        for (key, value) in vars {
            let placeholder = format!("{{{{{}}}}}", key);
            result = result.replace(&placeholder, &value);
        }

        Ok(result)
    }
}

/// Get built-in template for an agent type
fn get_builtin_template(agent_type: &AgentType) -> String {
    match agent_type {
        AgentType::Feature => TEMPLATE_FEATURE.to_string(),
        AgentType::Bugfix => TEMPLATE_BUGFIX.to_string(),
        AgentType::Test => TEMPLATE_TEST.to_string(),
        AgentType::Docs => TEMPLATE_DOCS.to_string(),
        AgentType::Review => TEMPLATE_REVIEW.to_string(),
        AgentType::Research => TEMPLATE_RESEARCH.to_string(),
        AgentType::Refactor => TEMPLATE_REFACTOR.to_string(),
        AgentType::Shepherd => TEMPLATE_SHEPHERD.to_string(),
        AgentType::Custom(_) => TEMPLATE_FEATURE.to_string(), // Default to feature
    }
}

// Built-in templates
const TEMPLATE_FEATURE: &str = r#"# Feature Session: {{session_name}}

**Type:** Feature Development
**Repository:** {{repo_name}}
**Branch:** {{branch}} (from {{base_branch}})
**VCS:** {{vcs_type}}
**Created:** {{created_at}}

## Objective

[Describe what feature you want to build]

## Requirements

- [ ] Requirement 1
- [ ] Requirement 2

## Implementation Plan

1. Step 1
2. Step 2

## Testing Strategy

- Unit tests for...
- Integration tests for...

## Notes

"#;

const TEMPLATE_BUGFIX: &str = r#"# Bugfix Session: {{session_name}}

**Type:** Bug Fix
**Repository:** {{repo_name}}
**Branch:** {{branch}} (from {{base_branch}})
**VCS:** {{vcs_type}}
**Created:** {{created_at}}

## Bug Description

[Describe the bug]

## Reproduction Steps

1. Step 1
2. Step 2

## Expected Behavior

[What should happen]

## Actual Behavior

[What actually happens]

## Root Cause Analysis

[Your analysis]

## Fix Plan

1. Change 1
2. Change 2

## Testing

- Test case 1
- Test case 2

## Notes

"#;

const TEMPLATE_TEST: &str = r#"# Test Session: {{session_name}}

**Type:** Test Development
**Repository:** {{repo_name}}
**Branch:** {{branch}} (from {{base_branch}})
**VCS:** {{vcs_type}}
**Created:** {{created_at}}

## Objective

Write tests for...

## Test Categories

### Unit Tests
- Test 1
- Test 2

### Integration Tests
- Test 1
- Test 2

### E2E Tests
- Test 1
- Test 2

## Coverage Goals

- Target: XX%
- Current: XX%

## Notes

"#;

const TEMPLATE_DOCS: &str = r#"# Documentation Session: {{session_name}}

**Type:** Documentation
**Repository:** {{repo_name}}
**Branch:** {{branch}} (from {{base_branch}})
**VCS:** {{vcs_type}}
**Created:** {{created_at}}

## Documentation Goals

[What needs to be documented]

## Audience

[Who is this documentation for]

## Outline

1. Section 1
2. Section 2

## Notes

"#;

const TEMPLATE_REVIEW: &str = r#"# Review Session: {{session_name}}

**Type:** Code Review
**Repository:** {{repo_name}}
**Branch:** {{branch}} (from {{base_branch}})
**VCS:** {{vcs_type}}
**Created:** {{created_at}}

## Review Focus

[What to review]

## Checklist

- [ ] Code quality
- [ ] Test coverage
- [ ] Documentation
- [ ] Performance
- [ ] Security

## Findings

### Issues
-

### Suggestions
-

## Notes

"#;

const TEMPLATE_RESEARCH: &str = r#"# Research Session: {{session_name}}

**Type:** Research/Spike
**Repository:** {{repo_name}}
**Branch:** {{branch}} (from {{base_branch}})
**VCS:** {{vcs_type}}
**Created:** {{created_at}}

## Research Question

[What are we investigating]

## Approach

1. Step 1
2. Step 2

## Findings

### Discovery 1
-

### Discovery 2
-

## Recommendations

## Notes

"#;

const TEMPLATE_REFACTOR: &str = r#"# Refactor Session: {{session_name}}

**Type:** Refactoring
**Repository:** {{repo_name}}
**Branch:** {{branch}} (from {{base_branch}})
**VCS:** {{vcs_type}}
**Created:** {{created_at}}

## Refactoring Goals

[What needs to be refactored]

## Current State

[Describe current implementation]

## Target State

[Describe desired implementation]

## Refactoring Plan

1. Step 1
2. Step 2

## Safety Checks

- [ ] All tests passing before refactor
- [ ] Tests still passing after refactor
- [ ] No behavior changes

## Notes

"#;

const TEMPLATE_SHEPHERD: &str = r#"# Shepherd Session: {{session_name}}

**Type:** PR Comment Resolution
**Repository:** {{repo_name}}
**Branch:** {{branch}} (from {{base_branch}})
**VCS:** {{vcs_type}}
**Created:** {{created_at}}

## PR Details

**PR Number:**
**PR Title:**

## Unresolved Comments

### Comment 1
**File:**
**Line:**
**Author:**
**Comment:**

**Analysis:**
- Action: [FIX/CLARIFY/ACKNOWLEDGE/DEFER/DISAGREE]
- Confidence: [HIGH/MEDIUM/LOW]

**Response:**

---

## Notes

"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Session;
    use std::path::Path;
    use tempfile::TempDir;

    fn create_test_manager() -> (ContextManager, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let mut config = Config::default();
        config.hp.sessions.context_dir = temp_dir.path().join("contexts");

        let manager = ContextManager::new(config).unwrap();
        (manager, temp_dir)
    }

    fn create_test_session() -> Session {
        Session::new(
            "test-session".to_string(),
            AgentType::Feature,
            "test-wb".to_string(),
            PathBuf::from("/tmp/test"),
            "feature/test".to_string(),
            "main".to_string(),
            "myrepo".to_string(),
            "git".to_string(),
        )
    }

    fn create_test_session_with_context_dir(context_base_dir: &Path) -> Session {
        let mut session = Session::new(
            "test-session".to_string(),
            AgentType::Feature,
            "test-wb".to_string(),
            PathBuf::from("/tmp/test"),
            "feature/test".to_string(),
            "main".to_string(),
            "myrepo".to_string(),
            "git".to_string(),
        );
        // Override context_dir to use test directory
        session.context_dir = context_base_dir.join("test-session");
        session
    }

    #[test]
    fn test_init_context() {
        let (manager, temp) = create_test_manager();
        let session = create_test_session_with_context_dir(&temp.path().join("contexts"));

        manager.init_context(&session).unwrap();

        // Check that directories and files were created
        assert!(session.context_dir.exists());
        assert!(session.context_dir.join("context.md").exists());
        assert!(session.context_dir.join("snapshots").exists());
        assert!(session.context_dir.join("activity.json").exists());
        assert!(session.context_dir.join("metrics.json").exists());
    }

    #[test]
    fn test_read_write_context() {
        let (manager, temp) = create_test_manager();
        let session = create_test_session_with_context_dir(&temp.path().join("contexts"));

        manager.init_context(&session).unwrap();

        let content = "# Test Context\n\nThis is a test.";
        manager.write_context(&session, content).unwrap();

        let read_content = manager.read_context(&session).unwrap();
        assert_eq!(read_content, content);
    }

    #[test]
    fn test_create_snapshot() {
        let (manager, temp) = create_test_manager();
        let session = create_test_session_with_context_dir(&temp.path().join("contexts"));

        manager.init_context(&session).unwrap();

        // Write some content
        manager.write_context(&session, "# Test Content").unwrap();

        // Create snapshot
        let snapshot = manager
            .create_snapshot(
                &session,
                "initial",
                SnapshotTrigger::Manual,
                Some("Initial snapshot".to_string()),
            )
            .unwrap();

        assert_eq!(snapshot.name, "initial");
        assert!(snapshot.path.exists());
    }

    #[test]
    fn test_list_snapshots() {
        let (manager, temp) = create_test_manager();
        let session = create_test_session_with_context_dir(&temp.path().join("contexts"));

        manager.init_context(&session).unwrap();

        // Create multiple snapshots
        manager
            .create_snapshot(&session, "snap1", SnapshotTrigger::Manual, None)
            .unwrap();
        manager
            .create_snapshot(&session, "snap2", SnapshotTrigger::Manual, None)
            .unwrap();

        let snapshots = manager.list_snapshots(&session).unwrap();
        assert_eq!(snapshots.len(), 2);
    }

    #[test]
    fn test_restore_snapshot() {
        let (manager, temp) = create_test_manager();
        let session = create_test_session_with_context_dir(&temp.path().join("contexts"));

        manager.init_context(&session).unwrap();

        // Write initial content
        let initial_content = "# Initial Content";
        manager.write_context(&session, initial_content).unwrap();

        // Create snapshot
        manager
            .create_snapshot(&session, "backup", SnapshotTrigger::Manual, None)
            .unwrap();

        // Change content
        manager
            .write_context(&session, "# Modified Content")
            .unwrap();

        // Restore snapshot
        manager.restore_snapshot(&session, "backup").unwrap();

        // Verify content was restored
        let restored_content = manager.read_context(&session).unwrap();
        assert_eq!(restored_content, initial_content);
    }

    #[test]
    fn test_sync_context() {
        let (manager, temp) = create_test_manager();
        let session1 = create_test_session_with_context_dir(&temp.path().join("contexts"));
        let mut session2 = create_test_session_with_context_dir(&temp.path().join("contexts"));
        session2.name = "test-session-2".to_string();
        session2.context_dir = temp.path().join("contexts/test-session-2");

        manager.init_context(&session1).unwrap();
        manager.init_context(&session2).unwrap();

        // Write content to session1
        let content = "# Shared Content";
        manager.write_context(&session1, content).unwrap();

        // Sync to session2
        manager.sync_context(&session1, &session2).unwrap();

        // Verify session2 has the same content
        let session2_content = manager.read_context(&session2).unwrap();
        assert_eq!(session2_content, content);
    }

    #[test]
    fn test_template_substitution() {
        let (manager, _temp) = create_test_manager();
        let session = create_test_session();

        let template = "Session: {{session_name}}, Branch: {{branch}}";
        let result = manager.substitute_variables(template, &session).unwrap();

        assert!(result.contains("test-session"));
        assert!(result.contains("feature/test"));
    }

    #[test]
    fn test_builtin_templates() {
        assert!(get_builtin_template(&AgentType::Feature).contains("Feature"));
        assert!(get_builtin_template(&AgentType::Bugfix).contains("Bug"));
        assert!(get_builtin_template(&AgentType::Test).contains("Test"));
        assert!(get_builtin_template(&AgentType::Docs).contains("Documentation"));
    }
}
