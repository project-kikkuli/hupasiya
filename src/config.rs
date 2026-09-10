//! Configuration management for hupasiya

use crate::error::Result;
use crate::models::AgentType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    pub hp: HpConfig,
    #[serde(skip)]
    pub repository_root: PathBuf,
}

impl Config {
    /// Load configuration with 4-level hierarchy
    pub fn load() -> Result<Self> {
        let mut config = Self::default();
        let root = repository_root()?;

        // 1. System config
        if let Ok(system_config) = Self::load_from("/etc/hapusiyas/config.yml") {
            config = Self::merge(config, system_config);
        }

        // 2. User config
        if let Some(user_home) = dirs::home_dir() {
            let user_config_path = user_home.join(".config/hp/config.yml");
            if let Ok(user_config) = Self::load_from(&user_config_path) {
                config = Self::merge(config, user_config);
            }
        }

        // 3. Repo config
        if let Ok(repo_config) = Self::load_from(root.join(".hapusiyas.yml")) {
            config = Self::merge(config, repo_config);
        }

        // 4. Local config (gitignored)
        if let Ok(local_config) = Self::load_from(root.join(".hapusiyas.local.yml")) {
            config = Self::merge(config, local_config);
        }

        for path in [
            &mut config.hp.sessions.metadata_dir,
            &mut config.hp.sessions.context_dir,
        ] {
            if path.is_relative() {
                *path = root.join(&*path);
            }
        }
        if config.hp.hn.command.contains(std::path::MAIN_SEPARATOR)
            && Path::new(&config.hp.hn.command).is_relative()
        {
            config.hp.hn.command = root
                .join(&config.hp.hn.command)
                .to_string_lossy()
                .into_owned();
        }
        config.repository_root = root;
        Ok(config)
    }

    /// Load configuration from a specific file
    pub fn load_from<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&content)?;
        Ok(config)
    }

    /// Merge two configs, with `other` taking precedence
    fn merge(mut base: Self, other: Self) -> Self {
        // For now, simple replacement merge
        // In production, this would do deep merging
        base.hp = other.hp;
        base
    }

    /// Get active profile configuration
    #[allow(dead_code)]
    pub fn get_active_profile(&self) -> Option<&ProfileConfig> {
        self.hp.profiles.get(&self.hp.active_profile)
    }
}

/// Git's first worktree is the primary checkout. Use its root for shared
/// metadata/config so entering a linked worktree or subdirectory does not
/// create a second, unrelated session store.
fn repository_root() -> Result<PathBuf> {
    let current = std::env::current_dir()?;
    let output = std::process::Command::new("git")
        .args(["worktree", "list", "--porcelain", "-z"])
        .current_dir(&current)
        .output();
    if let Ok(output) = output {
        if output.status.success() {
            if let Some(first) = output.stdout.split(|byte| *byte == 0).next() {
                if let Some(path) = String::from_utf8_lossy(first).strip_prefix("worktree ") {
                    return Ok(PathBuf::from(path));
                }
            }
        }
    }
    for directory in current.ancestors() {
        if directory.join(".hg").exists() || directory.join(".jj").exists() {
            return Ok(directory.to_path_buf());
        }
    }
    Ok(current)
}

/// hupasiya configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HpConfig {
    /// Default agent type
    #[serde(default = "default_agent_type")]
    pub default_agent: AgentType,

    /// hannahanna CLI settings
    #[serde(default)]
    pub hn: HnConfig,

    /// Session management
    #[serde(default)]
    pub sessions: SessionConfig,

    /// AI tool integration
    #[serde(default)]
    pub ai_tool: AiToolConfig,

    /// Multi-agent orchestration
    #[serde(default)]
    pub orchestration: OrchestrationConfig,

    /// PR integration
    #[serde(default)]
    pub pr: Option<PrConfig>,

    /// Templates
    #[serde(default)]
    pub templates: TemplateConfig,

    /// Configuration profiles
    #[serde(default)]
    pub profiles: HashMap<String, ProfileConfig>,

    /// Active profile
    #[serde(default = "default_profile")]
    pub active_profile: String,
}

impl Default for HpConfig {
    fn default() -> Self {
        Self {
            default_agent: default_agent_type(),
            hn: HnConfig::default(),
            sessions: SessionConfig::default(),
            ai_tool: AiToolConfig::default(),
            orchestration: OrchestrationConfig::default(),
            pr: None,
            templates: TemplateConfig::default(),
            profiles: HashMap::new(),
            active_profile: default_profile(),
        }
    }
}

fn default_agent_type() -> AgentType {
    AgentType::Feature
}

fn default_profile() -> String {
    "default".to_string()
}

/// hannahanna CLI configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HnConfig {
    /// Path to hn executable
    #[serde(default = "default_hn_command")]
    pub command: String,

    /// Default options to pass to hn commands
    #[serde(default)]
    pub default_options: HashMap<String, String>,

    /// Output format to request from hn
    #[serde(default = "default_output_format")]
    pub output_format: String,
}

impl Default for HnConfig {
    fn default() -> Self {
        Self {
            command: default_hn_command(),
            default_options: HashMap::new(),
            output_format: default_output_format(),
        }
    }
}

fn default_hn_command() -> String {
    "hn".to_string()
}

fn default_output_format() -> String {
    "json".to_string()
}

/// Session configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    /// Where to store session metadata
    #[serde(default = "default_metadata_dir")]
    pub metadata_dir: PathBuf,

    /// Where to store contexts
    #[serde(default = "default_context_dir")]
    pub context_dir: PathBuf,

    /// Auto-sync with parent on commit
    #[serde(default)]
    pub auto_sync: bool,

    /// Auto-snapshot context before major operations
    #[serde(default)]
    pub auto_snapshot: bool,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            metadata_dir: default_metadata_dir(),
            context_dir: default_context_dir(),
            auto_sync: false,
            auto_snapshot: false,
        }
    }
}

fn default_metadata_dir() -> PathBuf {
    PathBuf::from(".hp/sessions")
}

fn default_context_dir() -> PathBuf {
    PathBuf::from(".hp/contexts")
}

/// AI tool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiToolConfig {
    /// AI tool command
    #[serde(default = "default_ai_command")]
    pub command: String,

    /// Launch method
    #[serde(default)]
    pub launch_method: LaunchMethod,

    /// Context strategy
    #[serde(default)]
    pub context_strategy: ContextStrategy,

    /// Extra args
    #[serde(default)]
    pub extra_args: Vec<String>,

    /// Environment variables
    #[serde(default)]
    pub env: HashMap<String, String>,
}

impl Default for AiToolConfig {
    fn default() -> Self {
        Self {
            command: default_ai_command(),
            launch_method: LaunchMethod::Exec,
            context_strategy: ContextStrategy::SlashCommand,
            extra_args: Vec::new(),
            env: HashMap::new(),
        }
    }
}

fn default_ai_command() -> String {
    "claude-code".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LaunchMethod {
    #[default]
    Exec,
    ShellFunction,
    Tmux,
    Screen,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContextStrategy {
    #[default]
    SlashCommand,
    Flag,
    Env,
    File,
}

/// Orchestration configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(default)]
    pub cascade_on_commit: bool,

    #[serde(default)]
    pub gather_strategy: GatherStrategy,

    #[serde(default)]
    pub conflict_strategy: ConflictStrategy,
}

impl Default for OrchestrationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            cascade_on_commit: false,
            gather_strategy: GatherStrategy::Manual,
            conflict_strategy: ConflictStrategy::Prompt,
        }
    }
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum GatherStrategy {
    #[default]
    Manual,
    Auto,
    PrReady,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConflictStrategy {
    #[default]
    Prompt,
    ParentWins,
    ChildWins,
    Abort,
}

/// PR configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrConfig {
    pub github: GitHubConfig,

    #[serde(default)]
    pub auto_create: bool,

    #[serde(default)]
    pub shepherd: ShepherdConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubConfig {
    pub org: String,

    #[serde(default)]
    pub default_reviewers: Vec<String>,

    #[serde(default)]
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShepherdConfig {
    #[serde(default)]
    pub auto_apply_safe: bool,

    #[serde(default)]
    pub confidence_threshold: ConfidenceLevel,

    #[serde(default)]
    pub auto_post_responses: bool,

    #[serde(default = "default_analysis_model")]
    pub analysis_model: String,
}

impl Default for ShepherdConfig {
    fn default() -> Self {
        Self {
            auto_apply_safe: false,
            confidence_threshold: ConfidenceLevel::High,
            auto_post_responses: false,
            analysis_model: default_analysis_model(),
        }
    }
}

fn default_analysis_model() -> String {
    "gpt-4".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ConfidenceLevel {
    #[default]
    High,
    Medium,
    Low,
}

/// Template configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TemplateConfig {
    #[serde(default)]
    pub local: HashMap<String, PathBuf>,

    #[serde(default)]
    pub marketplace: MarketplaceConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(default = "default_registry_url")]
    pub registry_url: String,

    #[serde(default)]
    pub auto_update: bool,

    #[serde(default = "default_cache_dir")]
    pub cache_dir: PathBuf,
}

impl Default for MarketplaceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            registry_url: default_registry_url(),
            auto_update: false,
            cache_dir: default_cache_dir(),
        }
    }
}

fn default_registry_url() -> String {
    "https://hp-templates.dev".to_string()
}

fn default_cache_dir() -> PathBuf {
    PathBuf::from(".hp/cache/templates")
}

/// Configuration profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileConfig {
    pub hn: Option<HnConfig>,
    pub ai_tool: Option<AiToolConfig>,
    pub pr: Option<PrConfig>,
    pub orchestration: Option<OrchestrationConfig>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.hp.default_agent, AgentType::Feature);
        assert_eq!(config.hp.hn.command, "hn");
        assert_eq!(config.hp.active_profile, "default");
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let yaml = serde_yaml::to_string(&config).unwrap();
        assert!(yaml.contains("default_agent: feature"));

        let deserialized: Config = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(deserialized.hp.default_agent, AgentType::Feature);
    }

    #[test]
    fn test_load_from_string() {
        let yaml = r#"
hp:
  default_agent: bugfix
  hn:
    command: /usr/local/bin/hn
  active_profile: dev
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.hp.default_agent, AgentType::Bugfix);
        assert_eq!(config.hp.hn.command, "/usr/local/bin/hn");
        assert_eq!(config.hp.active_profile, "dev");
    }

    #[test]
    fn test_profile_config() {
        let yaml = r#"
hp:
  profiles:
    dev:
      hn:
        command: hn-dev
    prod:
      hn:
        command: hn-prod
  active_profile: dev
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.hp.profiles.len(), 2);

        let dev_profile = config.get_active_profile().unwrap();
        assert_eq!(dev_profile.hn.as_ref().unwrap().command, "hn-dev");
    }

    #[test]
    fn test_confidence_level() {
        let yaml = "high";
        let level: ConfidenceLevel = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(level, ConfidenceLevel::High);
    }
}
