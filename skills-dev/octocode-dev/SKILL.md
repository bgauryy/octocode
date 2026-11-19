---
name: octocode-dev
description: "Use when developing or verifying this Octocode monorepo: setup, builds, tests, lint, contract regeneration, tool audits, configuration changes or release preparation. Load it before running builds, tests or audits here: it owns the task runner and the shared-build rules."
---

# Octocode Dev

Run repository tasks from the root. Define these shell functions once.

```bash
dev() { node skills-dev/octocode-dev/scripts/dev.mjs "$@"; }
octo() { node packages/octocode/out/octocode.js "$@"; }
```

For tool calls see [MCP and CLI usage](references/octocode-usage.md).
Delegating more than one task: [orchestration](references/orchestration.md). Checking tools and workflows: [tool checks](../../packages/octocode-benchmark/CHECKS.md) (direct calls judged by reading; no harness scripts).

## Task runner

| When | Run |
|---|---|
| First checkout | `dev setup` then `yarn install` |
| Building locally | `dev build:dev` for the host (native `build` is the release-mode host build). Rebuild affected launchers. |
| Testing one package or tool | `yarn workspace <pkg> <script>`; after a local build, check the tool on CLI and MCP per [tool checks](../../packages/octocode-benchmark/CHECKS.md) |
| A skill or documentation changed | `dev skills:check` or `dev docs:verify`, respectively. Run both when both surfaces changed. |
| Static checks before handing over a change | `dev check`, with a focused test. Rust only: `dev lint:rust` |
| Finding where a tool flow's CPU goes | `dev profile <tool> '<query>' [--phase repeat]`: see [Development](docs/DEVELOPMENT.md#profiling-a-tool-flow) |
| Cross-package handoff | `dev verify` and `dev docs:verify` |
| Any other repository task | `dev --help` lists tasks. Extra arguments pass through to the selected task. For flags, owning script and regression test, use the [script guide](scripts/README.md). |

## Hard rules

Follow [repository working rules](../../AGENTS.md). Schemas and descriptions live in sibling core (`../octocode-mcp-host/packages/octocode-core`). After a contract change: build core, `dev contracts:regen`, rebuild native and affected consumers, restart any running MCP server. For a native change, rebuild the host and affected interface. Exercise `octo config --json`, `octo schema` and a real changed call ([contract pipeline](docs/DEVELOPMENT.md#contract-pipeline)).

Use `--all`, `build:all` and `build:publish` only for requested release or cross-platform work. `verify` and `check-outputs` check the host binary; `check-outputs --all` checks staged platforms.

## Audit a tool end to end

Run [tool-inventory.mjs](scripts/tool-inventory.mjs) with the tool name. Follow the [surface map](references/surface-map.md) from core to each interface, then [contract](references/contract-audit.md), [implementation](references/implementation-audit.md) and [output](references/output-audit.md) in order, covering all three lanes. Fix a confirmed defect at its owning layer per [fix and verify](references/fix-and-verify.md).

Use the [audit template](assets/audit-report.md), one report per tool; each 5W2H finding has source location, severity, evidence, fix and verification result, with unverified slots marked unknown. For independent audits give each actor its tool, skill, template and definition of done.

## Resources

| When | Read |
|---|---|
| Package ownership, environment, builds or opt-in Rust tools | [Development](docs/DEVELOPMENT.md) |
| A setting or credential changes | [Configuration changes](docs/ADDING_CONFIG.md) |
| Maintaining a scraper or adding a vendor | [Scraping maintenance](docs/SCRAPING_MAINTENANCE.md) |
| Checking tool evidence, pagination or continuations | [Tool quality](docs/TOOL_QUALITY.md) · [data contract](../../docs/TOOL_DATA_CONTRACT.md) |
| Reporting results | [Output](output.md) |

## Definition of Done

Generated outputs come from their owner, new fields have coverage, focused tests pass, and runtime changes are verified through the affected interface. Plain task runs report command and exit status; audits give result and evidence first, then unchecked scope.

## Related skills

`octocode-research` (code evidence), `rust-best-practices` (Rust design), `octocode-graph-dev` (graph engine, language battery), `octocode-documentation` (prose rewrites).
