# hupasiya (hp)

**Multi-agent session orchestrator for parallelized local development**

> Named after Hupasiya - the Hittite hero who slayed the dragon Illuyanka

## Overview

hupasiya (`hp`) is a multi-agent session orchestrator built on top of [hannahanna](https://github.com/rajatscode/hannahanna). It provides coordinated AI agent workflows with isolated workspaces, PR integration, and multi-agent coordination.

### What is hupasiya?

- **Session Management**: Create isolated AI agent sessions with dedicated workspaces
- **Multi-Agent Coordination**: Orchestrate multiple AI agents working on different aspects of a project
- **PR Integration**: Integrate with GitHub PRs and automate comment resolution
- **Context Management**: Maintain rich context for AI agents across sessions
- **hannahanna Integration**: Built on top of hannahanna for workbox management

### Relationship to hannahanna

- **hannahanna (hn)**: Intelligent workbox manager - handles git/hg/jj workspaces, resource sharing, Docker environments, state management
- **hupasiya (hp)**: Multi-agent orchestrator - handles AI sessions, context management, PR workflows, agent coordination

hupasiya calls `hn` commands as an external tool. Both must be installed.

For the verified local Git workflow and the required JSON-capable companion build,
see [local workflow setup](docs/local-workflow.md).

## Status

**Version**: 1.0.0
**Status**: Production Ready ✅

### ✅ v1.0 Complete Feature Set

**Core Features (v0.1)**:
- ✅ Complete session management (new, list, info, close, switch)
- ✅ hannahanna integration for workbox management
- ✅ Context management with snapshots and templates
- ✅ 4-level configuration hierarchy
- ✅ Doctor command for system health checks

**Multi-Agent Orchestration (v0.2)**:
- ✅ `cascade` - Sync parent changes to all children
- ✅ `gather` - Collect children back to parent
- ✅ `tree` - Visualize session hierarchies
- ✅ AI tool integration (launch, shell, exec)
- ✅ Multiple launch methods (exec, tmux, screen, shell-function)

**PR & Shepherd Workflows (v0.3)**:
- ✅ Full GitHub API integration
- ✅ `pr create` - Create PRs with reviewers/labels/draft
- ✅ `pr sync` - Fetch unresolved comments
- ✅ `pr status` - Display PR state and metrics
- ✅ Interactive shepherd workflow for comment resolution
- ✅ Activity logging and metrics tracking

**Collaboration & Marketplace (v0.4)**:
- ✅ `handoff` - Transfer sessions between developers
- ✅ `clone` - Duplicate sessions for parallel work
- ✅ `merge-sessions` - Consolidate work
- ✅ Template marketplace (list, install, search)
- ✅ Configuration profiles

**Utilities & Polish (v1.0)**:
- ✅ `monitor` - Live dashboard of all sessions
- ✅ `clean` - Remove old/archived sessions
- ✅ `leave` - Gracefully exit sessions
- ✅ Activity and metrics commands
- ✅ Shell completion scripts (bash, zsh, fish)
- ✅ Template marketplace with HTTP backend and caching
- ✅ Progress indicators for long operations
- ✅ Enhanced error messages with troubleshooting guidance
- ✅ Interactive tutorial covering all features
- ✅ 82 passing tests

### 📋 Known Limitations (v1.0)
- Template marketplace registry server not yet deployed (fallback to local templates)
- Real-time monitoring dashboard is basic (shows snapshot, not live updates)

See [CHANGELOG.md](CHANGELOG.md) for detailed release notes.

## Installation

### Prerequisites

- Rust 1.70 or later
- hannahanna 0.2.0 or later (`cargo install hannahanna`)
- Git (and optionally Mercurial or Jujutsu)

### Install hupasiya

```bash
# Clone repository
git clone https://github.com/yourorg/hupasiya.git
cd hupasiya

# Build and install
cargo install --path .

# Verify installation
hp version
# hupasiya (hp) v0.1.0

# Verify hannahanna is installed
hn --version
# hannahanna 0.2.0
```

### Install Git Hooks (for developers)

```bash
# Install pre-commit and pre-push hooks
./scripts/install-hooks.sh
```

## Quick Start

```bash
# Create a new session
hp new auth-feature --type=feature

# Create session on current branch (no new branch)
hp new my-work --no-branch

# Edit context (opens in your $EDITOR)
hp context edit auth-feature

# Launch AI agent with context
hp launch

# Create child session for tests
hp new auth-tests --parent=auth-feature --type=test

# Sync parent changes to children
hp cascade auth-feature

# Collect children back to parent
hp gather auth-feature

# Create PR
hp pr create auth-feature

# Address PR comments with AI
hp shepherd
```

## Key Features

### Session Management
- Create isolated sessions for different tasks
- Session types: feature, bugfix, test, docs, shepherd, refactor, research, review
- Parent/child session relationships
- Session lifecycle management

### Context Management
- Rich markdown context for AI agents
- Context templates for different session types
- Context snapshots for versioning
- Global (repo-wide) and shared contexts

### Multi-Agent Coordination
- **Cascade**: Sync parent changes to child sessions
- **Gather**: Collect child work back to parent
- Session trees for complex workflows
- Conflict resolution strategies

### PR Integration
- Create PRs from sessions
- Sync PR comments to context
- **Shepherd**: AI-powered PR comment resolution
  - Analyzes comments and suggests actions
  - Auto-applies high-confidence fixes
  - Drafts responses

### AI Tool Integration
- Works with multiple AI tools:
  - Claude Code
  - Cursor
  - Codex (OpenAI)
  - Any CLI-based AI tool
- Custom slash commands and instructions
- Environment variable integration

### Multi-VCS Support
- Git
- Mercurial (via hannahanna)
- Jujutsu (via hannahanna)

## Documentation

Comprehensive documentation is available in the [`spec/`](spec/) directory:

- **[Overview](spec/README.md)** - Start here
- **[Architecture](spec/architecture.md)** - System design and loose coupling
- **[Data Model](spec/data-model.md)** - Core data structures
- **[Commands](spec/commands.md)** - Complete CLI reference
- **[Workflows](spec/workflows.md)** - Common workflows and use cases
- **[Context Structure](spec/context-structure.md)** - `.hp/` directory structure
- **[Integration](spec/integration.md)** - How it integrates with hannahanna
- **[Configuration](spec/configuration.md)** - Config file format
- **[Roadmap](spec/roadmap.md)** - Version goals and milestones
- **[Testing](spec/testing.md)** - Testing strategy
- **[Contributing](spec/contributing.md)** - Development guide

## Example Workflow

### Feature Development with Tests

```bash
# 1. Create feature session
hp new auth-feature --type=feature

# 2. Edit context (describe what you want to build)
hp context edit

# 3. Launch AI agent
hp launch

# AI works on feature...

# 4. Create child session for tests
hp new auth-tests --parent=auth-feature --type=test

# 5. Cascade feature code to test session
hp cascade auth-feature

# 6. Switch to test session
hp switch auth-tests

# 7. Launch AI to write tests
hp launch

# 8. Gather tests back to feature
hp gather auth-feature

# 9. Create PR
hp pr create auth-feature

# 10. Address PR comments
hp shepherd
```

### PR Comment Resolution

```bash
# 1. Sync PR comments
hp pr sync auth-feature

# 2. Run shepherd to analyze comments
hp shepherd

# Shepherd:
# - Fetches all unresolved comments
# - Creates shepherd.md with context
# - Launches AI for analysis
# - AI suggests actions (FIX, CLARIFY, ACKNOWLEDGE, etc.)
# - Applies high-confidence fixes (if --auto-apply)
# - Posts responses to PR

# 3. Verify comments resolved
hp pr status auth-feature
```

## Configuration

Configuration files use YAML and follow a 4-level hierarchy:

1. **System**: `/etc/hapusiyas/config.yml`
2. **User**: `~/.config/hp/config.yml`
3. **Repo**: `.hapusiyas.yml` (committed)
4. **Local**: `.hapusiyas.local.yml` (gitignored)

### Example Configuration

```yaml
# .hapusiyas.yml
hp:
  default_agent: feature

  ai_tool:
    command: claude-code  # or cursor, codex, etc.
    launch_method: exec
    context_strategy: slash_command

  pr:
    github:
      org: myorg
      default_reviewers: [alice, bob]
      labels: [agent-created]

  shepherd:
    auto_apply_safe: false
    confidence_threshold: high
```

See [Configuration](spec/configuration.md) for complete reference.

## Development

### Setup

```bash
# Clone repository
git clone https://github.com/yourorg/hupasiya.git
cd hupasiya

# Install git hooks
./scripts/install-hooks.sh

# Build
cargo build

# Run tests
cargo test

# Run clippy
cargo clippy

# Format code
cargo fmt
```

### Project Structure

```
hupasiya/
├── src/              # Source code
│   ├── main.rs       # CLI entry point
│   ├── cli/          # CLI commands
│   ├── session/      # Session management
│   ├── context/      # Context management
│   ├── hn/           # hannahanna integration
│   ├── orchestration/# Multi-agent coordination
│   ├── pr/           # PR integration
│   └── config/       # Configuration
├── tests/            # Tests
├── spec/             # Specification documents
├── .githooks/        # Git hooks
└── scripts/          # Utility scripts
```

### Contributing

We welcome contributions! Please see [Contributing Guide](spec/contributing.md).

**Quick Contribution Steps**:
1. Fork the repository
2. Create a branch (`git checkout -b feature/my-feature`)
3. Make changes and add tests
4. Run checks (`cargo test`, `cargo clippy`, `cargo fmt`)
5. Commit (`git commit -m "feat: add my feature"`)
6. Push and create PR

### Code Style

- Use `rustfmt` for formatting (enforced by pre-commit hook)
- Use `clippy` for linting (enforced by pre-commit hook)
- Follow [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Document all public APIs

## Terminology

- **Workbox**: An isolated development environment managed by hannahanna (VCS workspace + deps + Docker + state)
- **Session**: An AI agent context managed by hupasiya (objectives + conversation + PR integration + context)
- **Cascade**: Syncing parent session changes to child sessions
- **Gather**: Collecting child session work back to parent
- **Shepherd**: Interactive PR comment resolution workflow

## Architecture

hupasiya is built as a standalone Rust binary that calls `hn` commands via the shell. This loose coupling provides:

- **Independence**: Version and distribute separately
- **Flexibility**: Work with different hn versions
- **Simplicity**: Clear separation of concerns
- **Testing**: Easy to mock hn commands

See [Architecture](spec/architecture.md) for detailed design.

## Roadmap

### v0.1.0 - Foundation (90% Complete)
- ✅ Core session management (create, list, info, close)
- ✅ hannahanna integration (workbox CRUD, exec, info)
- ✅ Context management (templates, snapshots, edit)
- ✅ Configuration system (4-level hierarchy, profiles)
- ✅ Full CLI with colored output
- ✅ Comprehensive test suite (41 tests passing)
- ⏳ Shell wrapper for `hp switch`
- ⏳ Integration tests with real hn
- ⏳ E2E workflow tests

### v0.2.0 - Multi-Agent (Next)
- ⏳ Multi-agent coordination (cascade, gather)
- ⏳ Orchestration engine
- ⏳ AI tool integration (launch commands)
- ⏳ Advanced context syncing

### v0.3.0 - PR Integration (Future)
- ⏳ GitHub API integration
- ⏳ PR creation and syncing
- ⏳ Shepherd workflow
- ⏳ Activity and metrics tracking

### v1.0.0 - Production Ready (Future)
- ⏳ Performance optimization
- ⏳ Monitoring dashboard
- ⏳ Template marketplace
- ⏳ Comprehensive documentation

See [Roadmap](spec/roadmap.md) for complete version goals.

## License

MIT License - see [LICENSE](LICENSE) for details

## Acknowledgments

- Built on top of [hannahanna](https://github.com/rajatscode/hannahanna)
- Inspired by the need for better multi-agent development workflows
- Named after Hupasiya, the Hittite hero

## Links

- **Documentation**: [spec/](spec/)
- **Issues**: [GitHub Issues](https://github.com/yourorg/hupasiya/issues)
- **Discussions**: [GitHub Discussions](https://github.com/yourorg/hupasiya/discussions)
- **Changelog**: [CHANGELOG.md](CHANGELOG.md)

---

**Status**: ✅ Production Ready
**Version**: 1.0.0
**Last Updated**: 2025-01-14
