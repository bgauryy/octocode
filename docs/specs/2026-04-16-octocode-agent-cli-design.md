# Octocode Agent CLI Architecture

Date: 2026-04-16 (revised 2026-04-17)
Repo: `vltansky/octocode-mcp`
Scope: Ship an agent-facing surface on the existing `octocode-cli` binary so coding agents get non-interactive parity with Octocode MCP for remote/provider-backed tools. Excludes local filesystem and LSP tools in v1.

## Decision Summary

Ship **per-tool subcommands** on the existing `octocode-cli` binary — one subcommand per tool with flag-style inputs — modeled on `sawyerhood/dev-browser`.

Specifically:

1. Add 6 per-tool subcommands that mirror the Octocode remote tools 1:1. Each takes the tool's required inputs as named flags and optional inputs as additional flags. All commands support `--json` for machine output.
2. Every subcommand supports bulk mode via stdin: `echo '{"queries":[...]}' | octocode-cli <subcommand>` — the single JSON object path is a thin wrapper around the same executor used by the MCP server.
3. Move the current interactive installer menu behind an explicit `octocode-cli install` (alias: `setup`).
4. `octocode-cli` with no args prints top-level help (dev-browser style). No hidden default behavior.
5. Keep the hidden `--tool <name>` entrypoint for two releases with a stderr deprecation notice.
6. Defer consolidation work (package split, provider extraction, OAuth dedup, `executeTool` seam) until after v1 ships and real agent usage is measurable.

**Why not generic `list | info | grep | call` verbs (mcp-s-style):** `@mcp-s/cli` exists to discover and drive *hundreds* of unknown tools exposed by third-party MCP servers. Octocode has 6 first-party remote tools we own and ship. A discovery/dispatch layer adds round-trips (`list → info → call`) and schema-shaped JSON payloads for no gain. Per-tool subcommands give agents direct `--help` pages and shell-native flag syntax — same model as `gh`, `kubectl`, `dev-browser`. Skills wrap the CLI with one-page cheat sheets, so discovery is solved at the skill layer, not at runtime.

This supersedes the earlier Approach B / mcp-s-style draft.

## Current State (verified)

The repo is closer to the target than it looks:

- **Execution is already decoupled from MCP transport.** Every tool splits `<tool>/execution.ts` (pure logic) from `<tool>/<tool>.ts` (register wrapper). E.g. `packages/octocode-mcp/src/tools/github_search_code/execution.ts` vs `github_search_code.ts:8-15`.
- **Direct executors are already exported.** `packages/octocode-mcp/src/public/tools.ts:109-176` exports 13+ execution functions. v1 uses: `searchMultipleGitHubCode`, `fetchMultipleGitHubFileContents`, `exploreMultipleRepositoryStructures`, `searchMultipleGitHubRepos`, `searchMultipleGitHubPullRequests`, `searchPackages`.
- **The canonical catalog already lives outside the repo.** `@octocodeai/octocode-core` (external npm dep, `^1.0.2`) publishes `OCTOCODE_TOOL_CATALOG`, `OCTOCODE_TOOL_NAMES`, `OctocodeToolCatalogEntry`, and Zod query schemas. v1 does not depend on the catalog at runtime — the CLI imports schemas directly from `octocode-mcp/public` as it already does.
- **The CLI tool runner already exists.** `packages/octocode-cli/src/cli/tool-command.ts` (819 LOC) runs a hidden `--tool <name>` entrypoint with autofill (`id`, `mainResearchGoal`, `researchGoal`, `reasoning`) and the full validation + result envelope handling. The 6 new subcommands reuse this code path — they are thin flag-to-JSON wrappers.
- **Auth is already transport-neutral.** `packages/octocode-shared/src/credentials/*` owns token resolution. CLI has a parallel device-flow OAuth (`features/github-oauth.ts`, 402 LOC). Left intact in v1.

### What v1 does not touch

- Provider extraction (`src/providers/{github,gitlab,bitbucket}/` stays in `octocode-mcp`).
- Package rename or split (`octocode-cli` stays one package).
- Auth consolidation (CLI OAuth stays as-is).
- Cross-repo changes to `@octocodeai/octocode-core`.
- Dynamic discovery/dispatch (`list | info | grep | call`).

## Product Recommendation

### Agent surface (6 per-tool subcommands)

```bash
# GitHub code search — required: --query
octocode-cli search-code --query "useReducer dispatch" --owner facebook --repo react --language js --limit 10

# File content — required: --owner --repo --path
octocode-cli get-file --owner facebook --repo react \
  --path packages/react-reconciler/src/ReactFiberHooks.js \
  --match-string "dispatchReducerAction" --match-context-lines 3

# Repo structure — required: --owner --repo
octocode-cli view-structure --owner facebook --repo react --path packages --depth 2

# Repo search — required: --query
octocode-cli search-repos --query "react hooks" --limit 5

# PR search — required: --owner --repo (or --query)
octocode-cli search-prs --owner facebook --repo react --state closed --limit 10

# Package search — required: --name --ecosystem
octocode-cli package-search --name lodash --ecosystem npm
```

### Bulk mode (stdin)

Agents that want to batch identical-shape queries pipe a `{queries:[...]}` payload:

```bash
echo '{"queries":[
  {"keywordsToSearch":["useReducer"],"owner":"facebook","repo":"react"},
  {"keywordsToSearch":["useState"],"owner":"facebook","repo":"react"}
]}' | octocode-cli search-code
```

When stdin is piped AND flags are present, flags are ignored in favor of the stdin payload (with a stderr warning). When stdin is empty, flags build a single-query payload.

### Administrative commands (unchanged or relocated)

```bash
octocode-cli install                    # interactive installer (was: no-args default)
octocode-cli setup                      # alias for install
octocode-cli login | logout | status    # unchanged auth verbs
octocode-cli token                      # unchanged
octocode-cli --version | --help         # unchanged
```

### Default behavior (no args)

`octocode-cli` with no args prints top-level help. To open the installer menu explicitly: `octocode-cli install`.

### Output modes

- stdout: tool result (text blocks if present, else structured JSON).
- stderr: errors, deprecation notices, diagnostics.
- `--json`: always print the raw result envelope.

### Exit codes

- `0` success
- `1` tool returned `isError` OR execution failed
- `2` usage / validation error (reserved; v1 may still return `1` for these — document as "non-zero on error")

### Autofill

Keep the existing autofill for `id`, `mainResearchGoal`, `researchGoal`, `reasoning` (as implemented in `tool-command.ts:266-299`). Subcommand handlers pass through to the same path.

### Deprecation

`octocode-cli --tool <name>` stays for two releases with a stderr notice: `warning: --tool is deprecated; use 'octocode-cli <subcommand>'`. Remove after the notice window.

### Why flags, not JSON payloads on the command line

Agents are LLMs. Quoting nested JSON in shell is a known failure mode ("smart" quotes, escaped braces). Named flags are copy-paste safe from `--help` output, survive round-trips through markdown, and map 1:1 to the tool's schema. When an agent genuinely needs bulk input, stdin avoids the quoting problem entirely.

## v1 Capability Scope

Expose (6 tools → 6 subcommands):

- `githubSearchCode` → `search-code`
- `githubGetFileContent` → `get-file`
- `githubViewRepoStructure` → `view-structure`
- `githubSearchRepositories` → `search-repos`
- `githubSearchPullRequests` → `search-prs`
- `packageSearch` → `package-search`

GitLab and Bitbucket equivalents: decide at ship time. The providers are already wired behind the shared abstractions, but UX of overloading `--owner/--repo` across three hosts needs a separate pass.

Do not expose: local tools, LSP tools, clone/checkout, interactive installer over subcommands.

## Migration Plan

### P1 — v1 ship (single package, single release)

Scope:

- Add 6 per-tool subcommands. Each is a thin wrapper that builds a query object from flags and delegates to the existing `executeToolCommand` path.
- Add `setup` as an alias for `install`.
- Flip default: no-args → top-level help (was: interactive installer). Installer reached via `install` / `setup` subcommands.
- Keep `--tool <name>` working, add stderr deprecation warning.
- Ensure `isError → exit 1` wiring is correct in `executeToolCommand` (currently: `return !result.isError; if (!success) process.exitCode = 1;` — verify this is hit by the new commands).
- Vitest tests: for each new subcommand — happy path via flags, bulk via stdin, `--json`, exit codes, autofill on/off, deprecation notice on `--tool`.
- Doc updates: `docs/CLI_REFERENCE.md`.
- Ship `skills/octocode-cli-usage/SKILL.md` — one-page agent cheat sheet (dev-browser style) listing the 6 subcommands and their most common flags.

Out of scope for v1:

- Updates to the 3 external skills (`octocode-install`, `octocode-engineer`, `octocode-research`) — these still reference `--tool`, which keeps working for two releases. Update in a follow-up release.
- Package rename or split.
- OAuth consolidation.
- Provider extraction.
- `executeTool` abstraction.
- Cross-repo changes to `@octocodeai/octocode-core`.

### P2 (optional, post-v1) — adoption-driven consolidation

Only with data from real agent usage. Candidates, ranked by value:

1. **OAuth dedup.** Audit CLI device flow vs `octocode-shared/credentials`. Backfill missing flows into shared. Delete the CLI copy (~500 LOC).
2. **Fast-startup bin.** If cold-start times hurt measured usage, add a second `bin` entry that imports only runtime + credentials (no installer/UI). One extra file in the same package.
3. **External skill refresh.** Rewrite `octocode-engineer` / `octocode-research` skills to use the new subcommands.
4. **Provider extraction.** Move `src/providers/*` and clients into a transport-neutral location. MCP becomes a thin adapter. Only do this if a second consumer (VSCode extension, another harness) needs it.
5. **Catalog-sourced descriptions.** Read tool descriptions and schemas from `OCTOCODE_TOOL_CATALOG` rather than hardcoding in CLI. Only makes sense once the CLI is held to the same "canonical catalog" standard as MCP.

Every P2 item is independently shippable.

### P3 (optional) — quality infrastructure

- Drift CI: assert `OCTOCODE_TOOL_CATALOG` names == CLI-registered subcommands == MCP-registered tools. One test, high leverage.
- Shell completions.
- Generated skill/context blocks from the catalog.

## Testing Strategy

v1 only:

1. **Vitest subcommand tests** per tool — flags → query object, stdin → queries array, `--json`, exit codes, autofill.
2. **Existing `tool-command.test.ts`** must continue to pass with the deprecation notice added.
3. **Manual check**: `npx octocode-cli` prints help; `npx octocode-cli install` still opens installer unchanged.

Post-v1 (P3):

- Drift test between catalog / CLI / MCP registrations.

## Risks

- **Behavior change: no-args no longer opens the installer.** Users typing `npx octocode-cli` expecting the menu now see help. `npm` weekly downloads are near zero — behavior-change risk is cosmetic. Mitigation: help output prominently shows `octocode-cli install`.
- **Flag coverage vs schema coverage.** Each tool schema has more fields than we want as flags. Rule: required fields are flags; top-10 most-used optional fields are flags; everything else is reachable via stdin `{queries:[...]}`. Document this in `--help`.
- **Quoting / escaping on Windows cmd.exe.** `--query "text with spaces"` works in PowerShell / bash / zsh / fish. Document the stdin escape hatch for cmd.exe users.
- **Schema drift.** If a tool's schema changes upstream, the subcommand's flag list can go stale. Mitigation (post-v1): drift CI (P3).
- **External skill references.** 3 public skills reference `--tool`. These break *only* if `--tool` is removed. Deprecation window (two releases) gives agents time to migrate; skills update in P2.
- **OAuth drift persists.** Not addressed in v1 — flagged for P2.

## Boundaries

- **Always:** one package, one binary, per-tool subcommands, `--help` as the agent contract.
- **Always:** `octocode-cli install` / `setup` is an explicit verb; no-args prints help.
- **Always:** required fields are named flags; bulk payloads via stdin.
- **Never:** require agents to construct JSON payloads on the command line as a positional arg.
- **Never:** couple subcommand handlers to installer/UI/marketplace imports.
- **Never:** invent new abstractions (`ToolSpec`, `executeTool`, dispatcher) before measuring a second consumer.
- **Ask first:** whether v1 should include GitLab/Bitbucket or ship GitHub+package only and follow up.

## Recommendation

Ship P1 as one release:

- 6 per-tool subcommands (`search-code | get-file | view-structure | search-repos | search-prs | package-search`), flag-driven with stdin bulk fallback.
- Flip no-args from installer menu to help. Installer behind `install` / `setup`.
- `--tool` still works, now with stderr deprecation notice.
- Minimal one-page skill (`skills/octocode-cli-usage/SKILL.md`) in dev-browser style.

Revisit consolidation (P2) only with data from real agent usage.

## Grill Record

This spec was stress-tested with `/vs-grill-me` on 2026-04-16 / 2026-04-17. Original output: READY_WITH_RISKS, 79/100, recommending mcp-s-style flat verbs (`list | info | grep | call`) on the existing binary.

**Revision (2026-04-17):** The mcp-s pattern was rejected on a second pass as over-engineered for Octocode's small fixed tool set. mcp-s solves "drive hundreds of unknown tools"; Octocode ships 6 known tools we own. Replaced with per-tool subcommands modeled on `sawyerhood/dev-browser`. Discovery moves from runtime (`list`/`info` commands) to install-time (one-page skill). Key tradeoffs:

- Schema drift risk: slightly higher (flags hand-mapped from schemas) vs mcp-s `info` being catalog-sourced. Mitigated via Vitest tests per subcommand + P3 drift CI.
- Agent UX: strictly better — no round-trips, no JSON-quoting on the command line, `--help` is the contract.
- Implementation cost: lower — thin wrappers over the existing `executeToolCommand` path.

Falsifiability for P2: revisit consolidation (OAuth dedup, provider extraction) only if distinct agent sessions per week cross a threshold worth defining at ship time, OR a specific issue pattern emerges from real usage.
