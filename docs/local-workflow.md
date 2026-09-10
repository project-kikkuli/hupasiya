# [AI-generated] Reproducible local Git session workflow

Agent-authored by OpenAI Codex (GPT-6 per session instructions; exact model build unverified).
Evaluation: PersonalProjectsBench; run ID unverified (not supplied).

This path creates a session, hands its persisted context to a local command,
branches a child from its parent, exchanges committed changes, and archives clean
worktrees. It requires a JSON-capable hannahanna build; the published 0.6.0 CLI
does not implement the protocol hp needs. Until a compatible release exists:

```sh
cargo install --git https://github.com/project-kikkuli/hannahanna --rev 15f398104638af60ad318a32d4ce021d6284219a --locked hannahanna
cargo install --path . --locked
```

Run from a Git repository with an initial commit and your own Git identity set:

```sh
hp new parent
hp context view parent
hp exec parent -- pwd
hp new child --parent parent
# Commit the child's changes in its worktree before gathering.
hp gather parent
# Commit subsequent parent changes before cascading.
hp cascade parent
hp close child --remove-workbox --archive
```

`hp switch parent --output-shell` emits POSIX shell commands for entering the
worktree. `hp exec` and `hp launch` set `HP_SESSION`, `HP_WORKBOX`, `HP_VCS`, and
`HP_CONTEXT`; the last is the absolute path to persisted `context.md`. The default
launch strategy writes `.claude/commands/hp_context.md` inside the session
worktree. Launching an actual AI tool requires your separately configured tool
and account; verification uses a local Python process to read the context.

Git sessions share configuration and metadata rooted in the primary checkout,
including from linked worktrees and subdirectories. Relative
`hp.sessions.metadata_dir` and `context_dir` resolve there. The existing repository
config filename is `.hapusiyas.yml`, with `.hapusiyas.local.yml` for overrides.
Set `hp.hn.command` to an absolute executable path if hn is not on PATH. Existing
relative context paths resolve against the primary checkout; session stores
previously created elsewhere are not automatically migrated.

A child inherits its parent's branch unless `--from` overrides it. Gather/cascade
return failure when any merge fails; inspect and resolve or abort the Git merge
in the affected worktree before retrying. Closing with `--remove-workbox` refuses
dirty work instead of forcing deletion. Removal requires an exact worktree name,
so a missing worktree cannot redirect deletion to a similarly named one. Archive
retains metadata and context.
Neither operation publishes or merges a GitHub PR.

## Verification

```sh
cargo build --locked
python3 scripts/check-local-workflow.py --hp target/debug/hp --hn "$(command -v hn)"
```

The standard-library script creates and removes disposable repositories, uses
synthetic commits and a fresh HOME, and does not contact GitHub or an AI service.
It exercises custom/default configuration, directory changes, paths containing
spaces/apostrophes, context delivery through exec/launch, parent inheritance,
gather/cascade, real merge conflicts, invalid-parent preflight, failing commands,
dirty-work protection, and archive cleanup. Assertions use real hp/hn binaries.
CI pins the same companion revision and runs this check on Linux.

This establishes the local Git workflow only. Mercurial/Jujutsu, tmux/screen,
GitHub PR automation, remote handoff, and the marketplace were not exercised by
this check. It is not a live multi-agent or model-quality evaluation.
