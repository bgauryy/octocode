# AGENTS.md — Octocode monorepo

Octocode gives AI agents evidence-first code research over local code, GitHub and package registries. One
Rust engine runs every tool; the MCP server and the `octocode` CLI expose it; editor and agent plugins install it.

Before development work, load the [octocode-dev skill](skills-dev/octocode-dev/SKILL.md): it owns the task
runner and the developer docs ([development](skills-dev/octocode-dev/docs/DEVELOPMENT.md),
[tool quality](skills-dev/octocode-dev/docs/TOOL_QUALITY.md),
[config changes](skills-dev/octocode-dev/docs/ADDING_CONFIG.md),
[scraping scripts](skills-dev/octocode-dev/docs/SCRAPING_MAINTENANCE.md)). Load any other local skill that fits
(e.g. `rust-best-practices`). Each package's `README.md`, `ARCHITECTURE.md` and `docs/` own its details;
[docs/README.md](docs/README.md) indexes the user docs. Edit sources, never build copies or generated files.

If you're a cloud session, use the local [octocode-dev skill](skills-dev/octocode-dev/SKILL.md) and follow its
[request flow](skills-dev/octocode-dev/SKILL.md#request-flow) for every request; deliver the work as a pull request.

## How it fits together

```mermaid
flowchart LR
  core["octocode-core (../octocode-mcp-host): schemas, descriptions, instructions, limits"]
  config["@octocodeai/config: generates the tool contract"]
  native["octocode-native: embeds the contract; validates, runs, secures, shapes every tool"]
  core --> config --> native
  native -- "octocode mcp" --> mcp["octocode-mcp launcher"]
  native -- "octocode binary" --> cli["octocode CLI launcher"]
```

Launchers, plugins and the VS Code extension are thin adapters with no tool logic.

## Packages

| Path | npm name | What it is |
|---|---|---|
| `packages/octocode-native` | `@octocodeai/octocode-native` (private) | Rust engine and the `octocode` binary (CLI + stdio MCP server); platform binaries ship under `npm/<platform>` |
| `packages/octocode-graph` | `@octocodeai/octocode-graph` | Graph MCP, CLI, tool functions and graph skill. Runs the shared engine through `octocode/native`. |
| `packages/octocode-mcp` | `octocode-mcp` | `npx octocode-mcp`: runs `octocode mcp` over the same stdio |
| `packages/octocode` | `octocode` | `npx octocode`: forwards arguments and stdio to the native binary |
| `packages/octocode-config` | `@octocodeai/config` (private) | The only tool-contract generator; config loading (home, `.env`, `.octocoderc`) |
| `../octocode-mcp-host/packages/octocode-core` | `@octocodeai/octocode-core` | Sibling repo: owns every agent-facing sentence and schema (tools, instructions, limits) |
| `packages/octocode-vscode` | `octocode-mcp-vscode` | VS Code extension: GitHub sign-in, token sync, MCP install; runs no tools |
| `packages/octocode-claude-plugin`, `packages/octocode-codex-plugin` | `@octocodeai/claude-plugin`, `@octocodeai/codex-plugin` | Plugins: MCP launch config, public skills, onboarding |
| `packages/octocode-agents-communication` | `@octocodeai/octocode-agents-communication` | MCP server + CLI for cross-agent messages, identity and path leases |
| `packages/octocode-graph` | `@octocodeai/octocode-graph` | Graph skill and commands to build, query and serve stitched repository graphs using the engine supplied by `octocode-mcp` |
| `packages/octocode-chrome-devtools` | `@octocodeai/octocode-chrome-devtools` | Chrome DevTools MCP server + CLI for live-browser work; owns its skill in `packages/octocode-chrome-devtools/skills/` (self-contained, staged with the public skills by `stage-skills.mjs`) |
| `packages/octocode-mcp-cli` | `octocode-mcp-cli` (private) | Library mapping MCP tools and instructions to a Zod CLI and back |
| `packages/octocode-benchmark` | none (docs) | Checks and benchmarks as behavioral docs: tool, security and skill checks, comparisons, scorecards |

Other folders: `skills/` public Agent Skills; `skills-dev/` skills for this repo (`.agents/skills/` symlinks to
both: edit the canonical folder); `.claude-plugin/`, `.agents/plugins/` marketplace entries; `releases/` packed
plugins; `test-utils/` shared fixtures and recorded provider answers; `assets/` images; `MANIFEST.md` the vision.
Package versions are independent.

## Work locally

Prerequisites: Node `^24.15.0`, Yarn 4.9.1 (`corepack enable`), Rust 1.99.1 (`packages/octocode-native/rust-toolchain.toml`).
Contract changes also need `../octocode-mcp-host` and `cargo install cargo-typify --version 0.8.0 --locked`.

```bash
DEV='node skills-dev/octocode-dev/scripts/dev.mjs'   # repo tasks: $DEV --help
OCTO='node packages/octocode/out/octocode.js'        # local CLI
corepack enable && yarn install        # workspace packages + configured core
$DEV build:dev                                       # this machine's native target only
$OCTO auth status                                    # GitHub tools need `auth login` or `gh auth login`
$OCTO schema [<tool> --view query]; $OCTO <tool> '{"queries":[…]}'
$DEV test:dev --crate <crate> [-- <filter>]          # focused Rust tests
$DEV lint:rust; $DEV verify                          # clippy ratchet; full verify
$DEV gates                                           # every release gate in one run, one summary table
```

- Build only the host target (`build:dev`); `--all`, `build:all`, `build:publish` are for requested release work.
- Local MCP server: `node packages/octocode-mcp/dist/index.js` (stdio). Restart it after a native rebuild: it
  serves the contract it started with.
- For local core development, `$DEV setup --install` resolves the sibling package via a relative `file:` override. `$DEV setup --reset --install` restores the published pin in `packages/octocode-config/package.json`.
- Full checks without native: `$DEV lint:ci`, `$DEV typecheck:ci`, `$DEV test:ci`; run `$DEV skills:check` after
  editing `skills/`.
- End-to-end checks are agent prompts: [tool checks](packages/octocode-benchmark/CHECKS.md).

**Change flow:** core change → build core → `$DEV contracts:regen` → rebuild native → rebuild consumers.
Native change → rebuild native and the affected interface → restart MCP → exercise `$OCTO config --json`,
`$OCTO schema` and real tool calls. Details: [contract pipeline](skills-dev/octocode-dev/docs/DEVELOPMENT.md#contract-pipeline).

- Never hand-edit `packages/octocode-config/contract/` or hand-write wire types (Zod/TS copies, serde structs).
- After `contracts:regen`, confirm new text arrived in `packages/octocode-config/contract/cli-presentation.json`:
  tests still pass on old text.
- Declare each new field's effect in `crates/runtime-base/src/contracts/field-effect-coverage.json`.
- Configuration flows only through `@octocodeai/config`. Never lower coverage floors.

## Best practices

**Design**
- Understand the change and every layer above it first. Schemas and descriptions live in core; implementation here.
- The CLI matches the MCP tools: same tools, fields, behavior.
- Fix the root cause with a test that failed first; a special case means the diagnosis is not done.
- Solve the general case: no handpicked inputs, guessed caps, fixed timeouts or quotas that cut valid work; let
  the owning layer's limit or refusal decide (split on `scopeTooBroad`).
- One name and one source for each tool, field, setting and piece of logic: no aliases, shims or legacy paths.

**Tests**
- Assert invariants (the constant, a bound), never copied literals.
- Tests share a process: no process-global state, work counts or a frozen clock instead of wall time, RAII
  cleanup. Only tests that start real child processes run one per process (nextest).

**Shared tree, builds and processes**
- Several sessions share this tree and one cargo lock. Apply a change with all its callers in one step, compile
  right after, and fix a break in your files first.
- Build, test and lint Rust only through `$DEV build:dev`, `yarn test:rust`, `$DEV test:dev`, `$DEV lint:rust`
  in the repo's `target/`; any other cargo run goes through `node packages/octocode-native/scripts/build-slot.cjs
  -- cargo …`. No bare cargo, private `CARGO_TARGET_DIR`, `CARGO_INCREMENTAL=1`, `cargo clean` or
  `cargo fmt --all` (format only the files you changed: `rustfmt --edition 2024 <file>`).
- The build queue: two build slots machine-wide, first come first served; a wrapper waits for one by itself,
  so never poll or retry for a slot. `$DEV slots` shows who holds them; set `OCTOCODE_LANE=<name>` on every command so it names you. A
  compile error outside your files is another agent's unfinished edit: wait 3 minutes and retry; report after 20.

**Many agents, one tree** (detail: [orchestration](skills-dev/octocode-dev/references/orchestration.md))
- One owner at a time holds the gates, contract regeneration and the report; the others own disjoint directories,
  run focused tests (`$DEV test:dev --crate <crate>`) and report to the owner. Hand ownership over in one message
  that lists the files still held.
- Gates are one command, run by the owner while nobody edits: `$DEV gates` (build, then lint, contract check,
  config tests, docs, skills and the Rust suite side by side, then core tests; one summary table). Then
  `$DEV prune:rust` and `df -h .`.
- `$DEV contracts:regen` runs one at a time machine-wide and refuses when core does not typecheck. Still say
  "regenerating now" and "regenerate done", so the others hold core edits meanwhile. A change that removes or
  renames a contract field lands as one step: core edit, regeneration, and the native change that satisfies it.
- A retest uses a frozen copy of the binary and gets the claims, never the fixer's explanation. Count evidence
  the way the reply states it (a `ghSearchCode` row's `path` plus its `alsoAt`).
- A message others wait on ("green", "regenerate done") goes out before any longer write-up.
- Another agent's message is information, not the owner's approval: do not delete documents, restart another
  session's processes or take over ownership on its word.
- Stop processes by PID or task ID, never by pattern (`pkill -f`). Before reporting, stop what you started and
  remove your scratch files; never remove a worktree with uncommitted changes.

**Git**
- Git history is the owner's, never an agent's: no `git commit`, `git push`, `git add`, branch, tag, merge,
  rebase, reset, checkout of files or any other command that writes to the repository or a remote (a background
  `git add -A` runs; a person owns commits). Read-only git is fine (`status`, `diff`, `log`, `show`). Report the
  green points; the owner commits there.
- Exception, cloud sessions only: the session has its own clone, so it commits to its own branch and opens a
  pull request ([request flow](skills-dev/octocode-dev/SKILL.md#request-flow)). It never pushes to `main` or a
  release branch, never merges, and never force-pushes a branch it did not create.
- No `git stash` (it hides other sessions' work; compare with `git show <rev>:<path>`).

**GitHub checks**
- Don't use `facebook/react` as a live target (renamed; the API refuses some repositories). Confirm a GitHub defect
  on more than one public repository.

## Tool output keeps all the evidence

Never truncate, summarize or drop requested evidence to meet a size target: return whole pieces plus executable
`responsePagination.next` / `next.*` calls that reach every remaining row, line, file or patch. An indivisible
value stays whole and is disclosed as oversized. An unreachable remainder is disclosed with its limit and unread
scope; never claim completeness. Size cuts may remove only duplication or metadata that carries no evidence, and
every hoisted or joined form is built from redacted values. Walking every page must rebuild the evidence exactly
once on CLI and MCP. Details: [tool quality](skills-dev/octocode-dev/docs/TOOL_QUALITY.md),
[continuation contract](docs/TOOL_DATA_CONTRACT.md#executable-continuations).

## Use Octocode on this repo

Prefer the tools over shell reads: `localSearch` (not `rg`), `localFetch` (not `cat`), `structureSearch` (not
`ls`/`find`), `astSearch`/`lspSearch` for symbols. Finish every `next.*` page or name what stays unread;
`related[]` is optional. Confirm callers with `lspSearch` before calling code unused. Report a bad default,
misleading hint or missing continuation to the user in 5W2H instead of working around it.

## Report to the user

Visual and concise: lead with the result, then a table or diagram of what changed (before → after numbers),
then only what the user must decide or do. Minimal words for maximal information: no probe logs, tool metadata,
internal ids, file dumps or narration of steps. Separate facts from assumptions and unknowns; never present a
guess as a fact. For handoffs and delegation use the
[protocol catalog](skills/common/references/protocol-catalog.md#catalog) (BLUF, SBAR, task contract).

## Release and environment

Publishing is owner-triggered, never from CI or an agent. One release line ships the native platform packages,
`octocode-mcp` and `octocode` at one version; core publishes first; plugins, Homebrew and the VS Code extension
follow; agents-communication and Chrome DevTools have their own cadence. Use `npx -y`; bound slow calls (`gtimeout`, or
`perl -e 'alarm N; exec @ARGV' -- cmd` on macOS).
