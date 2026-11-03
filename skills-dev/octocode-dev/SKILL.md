---
name: octocode-dev
description: "Develop and verify the Octocode monorepo. Run setup, builds, tests, lint and release preparation, regenerate contracts, or audit a tool or configuration across its owning layers."
---

# Octocode Dev

Run repository tasks from the root. Define these shell functions once.

```bash
dev() { node skills-dev/octocode-dev/scripts/dev.mjs "$@"; }
octo() { node packages/octocode/out/octocode.js "$@"; }
```

For tool calls see [MCP and CLI usage](references/octocode-usage.md).

## Task runner

| When | Run |
|---|---|
| First checkout | `dev setup` then `yarn install` |
| Building locally | `dev build:dev` for the host. Native `build` also targets the host for release-mode testing. Rebuild affected launchers. |
| Testing one package or tool | `yarn workspace <pkg> <script>` or, after a local build, `dev test:tools [tool,...]` for CLI and MCP |
| A skill or documentation changed | `dev skills:check` or `dev docs:verify`, respectively. Run both when both surfaces changed. |
| Cross-package handoff | `dev verify` and `dev docs:verify` |
| Any other repository task | `dev --help` lists tasks. Extra arguments pass through to the selected task. For flags, owning script and regression test, use the [script guide](scripts/README.md). |

## Hard rules

Follow [repository working rules](../../AGENTS.md). Schemas and descriptions live in sibling core (`../octocode-mcp-host/packages/octocode-core`). After a contract change, build core, run `dev contracts:regen`, rebuild native and affected consumers, then restart any running MCP server. For a native change, rebuild the host and affected interface. Exercise `octo config --json`, `octo schema` and a real changed call. See the [contract pipeline](docs/DEVELOPMENT.md#contract-pipeline) for field coverage.

Use `--all`, `build:all` and `build:publish` only for requested release or cross-platform work. `verify` and `check-outputs` check the host binary. Before release, `check-outputs --all` checks staged platforms.

## Audit a tool end to end

Run [tool-inventory.mjs](scripts/tool-inventory.mjs) with the tool name. Follow the [surface map](references/surface-map.md) from core to each interface, then work through [contract](references/contract-audit.md), [implementation](references/implementation-audit.md) and [output](references/output-audit.md) in order. On a confirmed defect, [fix and verify](references/fix-and-verify.md) at the owning layer. Keep coverage across all three lanes.

Use the [audit template](assets/audit-report.md) for one report per tool. Each 5W2H finding includes source location, severity, evidence, fix and verification result. Mark unverified slots unknown. For independent tool audits give each actor its tool, skill, template and definition of done.

## Resources

| When | Read | Why |
|---|---|---|
| Package ownership, environment, builds or opt-in Rust tools | [Development](docs/DEVELOPMENT.md) | Commands and contract pipeline |
| A setting or credential changes | [Configuration changes](docs/ADDING_CONFIG.md) | Key lifecycle |
| Preparing a release | [Release](docs/RELEASE.md) | Order and gates |
| Maintaining a scraper or adding a vendor | [Scraping maintenance](docs/SCRAPING_MAINTENANCE.md) | Script and fixture requirements |
| Checking tool evidence, pagination or continuations | [Tool quality](docs/TOOL_QUALITY.md) · [data contract](../../docs/TOOL_DATA_CONTRACT.md) | Acceptance checks |
| Reporting results | [Output](output.md) | Chat response or saved audit report |

## Definition of Done

Generated outputs come from their owner, new fields have coverage, and focused tests pass. Verify runtime changes through the affected interface. For plain task runs report the command and exit status. For audits give the result and its evidence first, then any unchecked scope.

## Related skills

Use `octocode-research` to establish code evidence, `rust-best-practices` for Rust design choices, `octocode-graph-dev` for graph engine changes and the language battery, or `octocode-documentation` for prose rewrites.
