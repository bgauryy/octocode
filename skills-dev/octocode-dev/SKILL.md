---
name: octocode-dev
description: "Load this skill first for any request in this Octocode monorepo (building, testing, linting, regenerating contracts, auditing tools, preparing a release) and in every cloud session: it owns the request flow, the task runner and shared-build rules that bare cargo or yarn would break."
---

# Octocode Dev

Run repository tasks from the root. Define these shell functions once.

```bash
dev() { node skills-dev/octocode-dev/scripts/dev.mjs "$@"; }
octo() { node packages/octocode/out/octocode.js "$@"; }
```

For tool calls see [MCP and CLI usage](references/octocode-usage.md). Delegating more than one task: [orchestration](references/orchestration.md). Checking tools and workflows: [tool checks](../../packages/octocode-benchmark/CHECKS.md) (direct calls judged by reading; no harness scripts).

## Request flow

Treat every request as a feature, whatever its size: a question, a fix, a doc edit and a new tool all run the same flow.

| Step | Do |
|---|---|
| 1. Research | Load the skills that fit (`octocode-research`, `octocode-engineer`, `rust-best-practices`, …) and use the Octocode tools, not shell reads, to find the owning layer, every caller and the existing tests before editing. |
| 2. TDD | Write the test that fails for the right reason first, then the change that makes it pass. Fix the root cause; no special cases. |
| 3. Cleanup | Remove what the change made dead: unused code, duplicate helpers, debug output, scratch files, stale docs and comments (`octocode-clean-agentic-code`). |
| 4. Maintenance | Leave the touched area healthier: update the docs, checks and scripts that describe it, and fix or report a defect found on the way. |
| 5. Validate | Always. Focused tests, `dev check`, then the changed behavior through the affected interface (CLI and MCP). Report what ran with its result, and what did not run. |
| 6. Learn | Name what the request taught: a missing rule, a misleading doc, a tool defect, a step that cost time. |
| 7. Update the skill | Put that lesson where the next session will read it: this skill, its docs or `AGENTS.md`. A lesson only in the report is lost. |

**Pull requests.** A cloud session works on its own branch and delivers through a pull request: one request, one PR, with the result, the validation evidence and the unchecked scope in its description. Push follow-up fixes to the same PR and re-validate after each. Never push to `main` or a release branch and never merge. A local session shares the tree: git stays the owner's ([Git rules](../../AGENTS.md#best-practices)), so it reports the green point instead.

## Task runner

| When | Run |
|---|---|
| First checkout | `yarn install` (use `dev setup --install` for sibling core development, or `dev setup --reset --install` for the npm pin) |
| Building locally | `dev build:dev` for the host (native `build` is the release-mode host build). Rebuild affected launchers. |
| Testing one package or tool | `yarn workspace <pkg> <script>`; after a local build, check the tool on CLI and MCP per [tool checks](../../packages/octocode-benchmark/CHECKS.md) |
| A skill or documentation changed | `dev skills:check` or `dev docs:verify`, respectively. Run both when both surfaces changed. |
| Static checks before handing over a change | `dev check`, with a focused test. Rust only: `dev lint:rust` |
| Finding where a tool flow's CPU goes | `dev profile <tool> '<query>' [--phase repeat]`: see [Development](docs/DEVELOPMENT.md#profiling-a-tool-flow) |
| Cross-package handoff | `dev verify` and `dev docs:verify` |
| Release candidate, or the last check of a multi-agent round | `dev gates`: every gate in one run, one summary table. One owner runs it; nobody edits the tree meanwhile |
| Any other repository task | `dev --help` lists tasks. Extra arguments pass through to the selected task. For flags, owning script and regression test, use the [script guide](scripts/README.md). |

## Hard rules

Follow [repository working rules](../../AGENTS.md). Schemas and descriptions live in sibling core (`../octocode-mcp-host/packages/octocode-core`). After a contract change: build core, `dev contracts:regen`, rebuild native and affected consumers, restart any running MCP server. For a native change, rebuild the host and affected interface. Exercise `octo config --json`, `octo schema` and a real changed call ([contract pipeline](docs/DEVELOPMENT.md#contract-pipeline)).

Use `--all`, `build:all` and `build:publish` only for requested release or cross-platform work. `verify` and `check-outputs` check the host binary; `check-outputs --all` checks staged platforms.

## Resources

| When | Read |
|---|---|
| Package ownership, environment, builds or opt-in Rust tools | [Development](docs/DEVELOPMENT.md) |
| A setting or credential changes | [Configuration changes](docs/ADDING_CONFIG.md) |
| Maintaining a scraper or adding a vendor | [Scraping maintenance](docs/SCRAPING_MAINTENANCE.md) |
| Checking tool evidence, pagination or continuations | [Tool quality](docs/TOOL_QUALITY.md) · [data contract](../../docs/TOOL_DATA_CONTRACT.md) |
| Auditing one tool across core, CLI and MCP | [Tool audit](docs/TOOL_QUALITY.md#audit-a-tool-end-to-end) |
| Reporting results | [Output](output.md) |
| Neighbor skills | `octocode-research` (code evidence), `rust-best-practices` (Rust design), `octocode-graph-dev` (graph engine, language battery), `octocode-documentation` (prose rewrites). |

## Definition of Done

Generated outputs come from their owner, new fields have coverage, focused tests pass, and runtime changes are verified through the affected interface. Plain task runs report command and exit status; audits give result and evidence first, then unchecked scope.
