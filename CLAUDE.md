# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Detected stack
- Languages: Rust.
- Frameworks: none detected from the supported starter markers.

## Verification
- Run Rust verification from `src/`: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`

## Repository shape
- `src/` contains the Rust workspace and active CLI/runtime implementation.
- `.github/scripts/check_doc_source_of_truth.py` is the remaining Python-based maintenance check in the active tree.

## Working agreement
- Prefer small, reviewable changes and keep generated bootstrap files aligned with actual repo workflows.
- Keep shared defaults in `.claude.json`; reserve `.claude/settings.local.json` for machine-local overrides.
- Do not overwrite existing `CLAUDE.md` content automatically; update it intentionally when repo workflows change.
- **Design/implementation docs**: WIP docs use `_` prefix and are gitignored (`_*.md`, `_docs/`). Drop the `_` to commit. See `AGENTS.md` for the full naming convention.

## Substrate Platform Role

Ninmu-code is the **agentic coding harness** for the Substrate (kurayami kōjō)
platform. Factory Cells run ninmu-code as a static binary inside microVMs to
execute coding tasks autonomously.

- **Task mode**: `ninmu run-task` receives `HarnessTaskRequest` on stdin, returns
  `HarnessTaskResult` on stdout (JSON, v1alpha1 contract)
- **Cell deployment**: The 17MB release binary is baked into Factory Cell images
- **Orchestrator**: ninmu (the mission planner) spawns ninmu-code via the harness
  contract; cosmictron-cell wraps the lifecycle
- **Backlog**: See `~/Projects/substrate/docs/backlog/` for the unified backlog.
  This project's items are in workstream **C1** (cell runtime) — ninmu-code is
  the harness that C1.6 (cell entrypoint) orchestrates.
- **Branch convention**: `substrate/{workstream}-{item}` for substrate work
- **Types**: Use `substrate-types` crate for canonical identity types when
  interfacing with the platform
