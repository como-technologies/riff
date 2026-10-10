# About this branch

Claude Code created this branch on 2026-10-10, at the owner's request. Thelio's disk is about to be wiped for a fresh Ubuntu 26.04 install (n8behavior/dotfiles#24 and #19). This branch keeps work that existed only on Thelio.

- Source: `~/src/como-technologies/riff/.claude/worktrees/issue-273`, branch `worktree-issue-273`, at `ca39e65`
- What it holds: its unpushed commit: ca39e65 A wake takes back the ask to stop an idle worker.
- Not saved: the worktree also had `.cargo/audit.toml`, `.gitignore` and `Cargo.lock` deleted, plus untracked build output (`target/`, 13 GB; `docs/book/`; `docs/gruvbox/`). The output shows up only because `.gitignore` was gone. These look like leftovers rather than work, so they were left out.

To pick the work up: `git fetch origin thelio-safekeeping/worktree-issue-273`, then cherry-pick or merge the commits below this file's commit. This file is not part of the work, so leave its commit out. Delete the branch when the work is merged or no longer needed.
