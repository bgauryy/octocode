# Octocode package overview

Workspace packages and one external contract package provide the Octocode research and agent-integration stack.

## Runtime flow

```text
CLI launcher ───────────────┐
MCP stdio registration ────┼──▶ octocode-native (Rust ToolRuntime)
                           │          ├──▶ internal engine crate
                           │          ├──▶ octocode-core contracts (external)
                           │          └──▶ octocode-config
VS Code integration ───────┘
```

Public tool validation, providers, security, bulk execution, pagination, response shaping, and cancellation have one owner: `octocode-native`. Node interfaces delegate or fail closed.

## Core runtime

### [`packages/octocode-native`](../packages/octocode-native) — `@octocodeai/octocode-native`

Rust implementation of the public tool catalog and consolidated npm distribution. Six platform packages each ship the native CLI, regex worker, runtime addon, and engine primitive addon. Separate `crates/runtime` and `crates/engine` preserve policy/algorithm boundaries; `.` and `./runtime` expose the runtime while `./engine` exposes primitives.

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

## Support packages

### [`packages/octocode-skill-installer`](../packages/octocode-skill-installer) — `@octocodeai/octocode-skill-installer`

Private shared implementation for durable canonical skill copies, platform links or copies, upgrades, conflict policy, and atomic replacement. Calling CLIs own argument parsing and presentation.

### [`skills/octocode-agents-communication`](../skills/octocode-agents-communication) — `@octocodeai/octocode-agents-communication`

Private Python CLI and communication skill for shared session identity, path leases, messages, delivery, and handoff documents.

### [`packages/octocode-benchmark`](../packages/octocode-benchmark) — `@octocodeai/octocode-benchmark`

Private evaluation workspace for controlled comparisons, VRPT scoring, routing regressions, graders, and reproducible reports.

## Development and internal environment variables

These are not user configuration settings and are not part of `.octocoderc`.

| Variable | Read by | Effect |
|---|---|---|
| `OCTOCODE_NATIVE_BIN` | `octocode` CLI launcher | Absolute path to a native `octocode` binary used instead of the packaged platform binary; a missing path disables native delegation. |
| `OCTOCODE_NATIVE_BINDING` | `octocode-mcp` | Path to a candidate `.node` addon loaded instead of the packaged one. Ignored when `NODE_ENV=production`; the bundled `dist` also ignores it unless `NODE_ENV` is `development` or `test`. |
| `OCTOCODE_ALLOW_CONTRACT_DRIFT` | `octocode-mcp` | `1` downgrades the core/native contract-fingerprint mismatch from a startup failure to a stderr warning. Ignored when `NODE_ENV=production`; the bundled `dist` also ignores it unless `NODE_ENV` is `development` or `test`. |
| `OCTOCODE_SKILL_DELEGATED` | native `octocode skill` | Internal recursion guard set when the native binary delegates `skill` to the npm CLI; do not set it. |
| `XDG_CONFIG_HOME` | native `octocode install` (Linux) | Locates IDE/client config directories (default `~/.config`). It never moves the Octocode home, which only `OCTOCODE_HOME` overrides. |

## Ownership rules

- Public tool behavior belongs only in `octocode-native` Rust.
- Interfaces may register, delegate, render, or provide interactive selection; they may not implement tools.
- The native `crates/engine` crate and public `./engine` subpath expose primitives, not public tool policy.
- Public contracts come from `@octocodeai/octocode-core`.
- Configuration comes from `@octocodeai/config`.
- Skill filesystem behavior comes from `@octocodeai/octocode-skill-installer`.
