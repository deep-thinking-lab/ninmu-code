# AGENTS.md

This file provides guidance to AI coding agents when working in this repository.

## Naming conventions

### Design and implementation docs

Work-in-progress design and implementation documents use an `_` (underscore) prefix and are **gitignored** — they live only on your local machine. When a doc is ready for review, drop the `_` prefix to commit it.

| Pattern | Purpose |
|---------|---------|
| `_*.md` | WIP design/implementation doc (gitignored) |
| `*.md` | Final, committed doc |
| `_docs/` | WIP doc directories (gitignored) |

**Examples:**
```
docs/_provider-fallback-design.md    ← WIP, not tracked
docs/_testing-plan.md                ← WIP, not tracked
docs/provider-fallback-design.md     ← final, committed
```

### Provider naming

Provider labels in code, config, and docs use these canonical names (case-insensitive for user input):

| Canonical | Aliases | Env var prefix |
|-----------|---------|---------------|
| `anthropic` | — | `ANTHROPIC_` |
| `openai` | — | `OPENAI_` |
| `xai` | `grok` | `XAI_` |
| `deepseek` | — | `DEEPSEEK_` |
| `dashscope` | — | `DASHSCOPE_` |
| `ollama` | — | `OLLAMA_` |
| `qwen` | — | `QWEN_` |
| `vllm` | — | `VLLM_` |

## Verification

- Run Rust verification from `src/`: `cargo fmt && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
- API tests: `cargo test -p ninmu-api`
- Runtime tests: `cargo test -p ninmu-runtime`

## Provider architecture

### Adding a new provider

Follow this checklist (see `_provider-implementation-plan.md` for details):

1. `api/src/providers/mod.rs` — add `ProviderKind` variant, `MODEL_REGISTRY` entries, alias resolution, `metadata_for_model` routing, `detect_provider_kind` auth sniffing
2. `api/src/providers/openai_compat.rs` — add `OpenAiCompatConfig::new_provider()` constructor, default base URL, env vars, body size limits
3. `api/src/client.rs` — add `ProviderClient::NewProvider(OpenAiCompatClient)` variant, wire into `from_model_with_anthropic_auth`
4. `api/src/providers/models_file.rs` — add to `VALID_API_VALUES` and `custom_metadata_for_model`
5. `runtime/src/usage.rs` — add pricing entry in `pricing_for_model()` and `per_provider_label()`
6. `ninmu-cli/src/cli_commands.rs` — add to `check_providers_health()`
7. `ninmu-cli/src/format/model.rs` — add to `provider_label()`
8. `ninmu-cli/src/init.rs` — add to `PROVIDER_ENV_TEMPLATE`
9. `README.md` — add to Built-in Providers table
10. Tests — 3-4 per provider (alias, routing, config, credential)

## Substrate Platform — Kurayami Kōjō (暗闘工場)

This project is a component of the **Substrate** governed agentic software
factory platform. **ninmu-code is the agentic coding harness** that runs inside
Factory Cell microVMs. The 17MB static binary is baked into cell images and
invoked via the `run-task` stdin/stdout JSON protocol.

### Role in Substrate

- **Workstream C1** items C1.1-C1.4 are implemented in this project
- `substrate-cell` (a separate wrapper binary) reads the FactoryCell manifest,
  clones the repo, invokes ninmu-code, and translates the result
- ninmu-code doesn't need to know about Substrate directly — it receives a
  `HarnessTaskRequest` and returns a `HarnessTaskResult`
- Substrate-specific extensions: manifest adapter (C1.1), OAMP memory tool
  (C1.2), SubstrateEvent emission (C1.3), Kizuna SCM client (C1.4)
- Design doc: `~/Projects/substrate/docs/design/2026-05-27-ninmu-code-cell-harness-design.md`

### Unified Backlog

The master backlog is at:
`~/Projects/substrate/docs/backlog/2026-05-27-substrate-unified-backlog.md`

The full platform backlog (all projects consolidated) is at:
`~/Projects/substrate/docs/backlog/2026-05-27-unified-platform-backlog.md`

Read these before starting any substrate work.

### Workstream → Project Mapping

| Workstream | Project | Path |
|------------|---------|------|
| S1 | substrate-types | ~/Projects/substrate |
| V1 | voxeltron (cell launcher) | ~/Projects/voxel/voxeltron |
| C1 | cosmictron + ninmu-code (cell runtime) | ~/Projects/cosmictron + ~/Projects/ninmu-code |
| N1, N2 | ninmu (mission control) | ~/Projects/ninmu |
| U1 | ultra/tanuki (governance) | ~/Projects/ultra/ultrasushitron |
| K1 | kizuna-dream/kizuna-mem (memory) | ~/Projects/kizuna-dream |
| F1 | kizuna (source forge) | ~/Projects/kizuna |
| L1 | substrate-llm (shared LLM gateway) | ~/Projects/substrate |
| D1 | operator dashboard | ~/Projects/cosmictron + ~/Projects/ultra/ultrasushitron |
| I1 | integration tests | ~/Projects/substrate |

### Development Protocol

1. **Read the backlog** — find your workstream items
2. **Read or create the TDD plan** — in this project's `docs/plans/`
3. **Write tests first** — then implement
4. **Use substrate-types** — canonical identity types for all platform interfaces
5. **Branch**: `substrate/{workstream}-{item}` (e.g., `substrate/v1.1-cell-manifest-intake`)
6. **Commit**: `substrate/{item-id}: {description}`
7. **Open a PR** when done, referencing the backlog item

### substrate-types Dependency (Rust Projects)

```toml
substrate-types = { git = "https://github.com/deep-thinking-lab/substrate.git" }
```

### Key Design Docs

- Architecture: `~/Projects/substrate/docs/design/2026-05-26-substrate-software-factory-design.md`
- Types: `~/Projects/substrate/docs/design/2026-05-27-substrate-types-design.md`
- GTM: `~/Projects/substrate/docs/design/2026-05-27-go-to-market-strategy.md`
- LLM Gateway: `~/Projects/substrate/docs/design/2026-05-27-llm-gateway-extraction-design.md`
- Dashboard: `~/Projects/substrate/docs/design/2026-05-27-operator-dashboard-design.md`
- Bootstrap: `~/Projects/substrate/docs/plans/2026-05-27-bootstrap-and-coordination.md`
