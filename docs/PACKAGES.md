# Octocode package overview

Eleven workspace packages and one external contract package provide the Octocode research and agent-integration stack.

## Runtime flow

```text
CLI launcher ───────────────┐
MCP stdio registration ────┼──▶ octocode-native (Rust ToolRuntime)
                           │          ├──▶ internal engine crate
                           │          ├──▶ octocode-core contracts (external)
                           │          └──▶ octocode-config
Pi / VS Code integrations ─┘
```

Public tool validation, providers, security, bulk execution, pagination, response shaping, and cancellation have one owner: `octocode-native`. Node interfaces delegate or fail closed.

## Core runtime

### [`packages/octocode-native`](../packages/octocode-native) — `@octocodeai/octocode-native`

Rust implementation of the public tool catalog and consolidated npm distribution. Six platform packages each ship the native CLI, regex worker, runtime addon, and engine primitive addon. Separate `crates/runtime` and `crates/engine` preserve policy/algorithm boundaries; `.` and `./runtime` expose the runtime while `./engine` exposes primitives.

### [`packages/octocode-engine`](../packages/octocode-engine) — `@octocodeai/octocode-engine`

Deprecated JavaScript-only compatibility wrapper. It re-exports `@octocodeai/octocode-native/engine` and owns no Rust source, platform packages, or native build pipeline.

### [`packages/octocode-config`](../packages/octocode-config) — `@octocodeai/config`

Zero-dependency owner of Octocode home resolution, environment propagation, `.env`, `.octocoderc`, and protected configuration keys.

### `@octocodeai/octocode-core` *(external sibling repository)*

Canonical public schemas, descriptions, examples, relations, and shared MCP/CLI instructions. It defines contracts but does not execute tools.

## Interfaces

### [`packages/octocode`](../packages/octocode) — `octocode`

Public Node launcher. It delegates public tools and flag-only management commands to the native CLI, retains the shared Agent Skill command, and provides a TTY picker that discovers install targets from native before delegating installation.

### [`packages/octocode-mcp`](../packages/octocode-mcp) — `octocode-mcp`

Thin stdio MCP server. It registers Standard Schema definitions and forwards execution to the native N-API runtime. There is no TypeScript tool fallback.

### [`packages/octocode-vscode`](../packages/octocode-vscode) — `octocode-mcp-vscode`

VS Code extension for GitHub OAuth, token synchronization, and MCP installation across supported editors. It does not execute research tools.

### [`packages/octocode-pi-extension`](../packages/octocode-pi-extension) — `@octocodeai/pi-extension`

Pi integration, native workspace tools, Awareness assets, prompts, capability contracts, dynamic tools, and harness hooks.

## Support packages

### [`packages/octocode-skill-installer`](../packages/octocode-skill-installer) — `@octocodeai/octocode-skill-installer`

Private shared implementation for durable canonical skill copies, platform links or copies, upgrades, conflict policy, and atomic replacement. Calling CLIs own argument parsing and presentation.

### [`packages/octocode-awareness`](../packages/octocode-awareness) — `@octocodeai/octocode-awareness`

SQLite-backed coordination runtime for plans, work state, locks, messages, memory, reflection, verification, and host hooks.

### [`packages/octocode-extension-rust`](../packages/octocode-extension-rust) — `@octocodeai/octocode-extension-rust`

Separate Rust/N-API package for filesystem snapshots, guarded mutations, durability, verified loose Git objects, and line diffs used by agent hosts.

### [`packages/octocode-benchmark`](../packages/octocode-benchmark) — `@octocodeai/octocode-benchmark`

Private evaluation workspace for controlled comparisons, VRPT scoring, routing regressions, graders, and reproducible reports.

## Ownership rules

- Public tool behavior belongs only in `octocode-native` Rust.
- Interfaces may register, delegate, render, or provide interactive selection; they may not implement tools.
- The native `crates/engine` crate and public `./engine` subpath expose primitives, not public tool policy.
- `@octocodeai/octocode-engine` remains only as a migration wrapper.
- Public contracts come from `@octocodeai/octocode-core`.
- Configuration comes from `@octocodeai/config`.
- Skill filesystem behavior comes from `@octocodeai/octocode-skill-installer`.
- Awareness host API changes require Awareness to be built before the Pi extension.
