# 06 — Token budget: measure and cut the per-request baseline

**Status:** Proposed · **Priority:** P3 · **Lane:** Scale & cost
**Related:** [05 — Fan-out batches](05-fan-out-batches.md) multiplies this baseline by every child. [01 — Permissions](01-permissions.md) adds the `permissions:` frontmatter beside the new `mcpTools:` and `skills:` fields.

## Problem and evidence

Every model request resends the system prompt and all declared tool schemas. Prompt caching lowers the price of repeats, but every new session and every subagent pays a cache write, and the full prefix fills the context window.

### Method

Pi records each session's prompt sections and declared tools in the first `system` entry of the session JSONL (`sections`, `toolsAdded`). The first assistant entry carries the provider-reported `usage`. Recorded characters divided by reported tokens give the real ratio for `claude-opus-5-5`: **2.54 chars/token overall** and **2.42 chars/token for MCP JSON schemas**. The second figure comes from two e2e runs that differ only in MCP: 18,458 tokens for 44,602 characters. A chars/4 estimate undercounts schemas by about 40%.

The one-off read-only scripts are in `.octocode/tmp/agents/implementer-824bf1/` (`session-breakdown.mjs`, `mcp-list.mjs`, `dup2.mjs`, `flatten.mjs`, `ext-tools.mjs`, `prompt.mjs`). Phase 1 moves this measurement into `tests/token-budget.test.ts`.

### Measured first-request input (cacheWrite + cacheRead + input)

| Session | Tokens | Notes |
|---|---|---|
| Repo session, 2026-10-06 07:55 (this package) | 50,525 | All 9 Octocode tools direct, 27 skills (incl. 9 synced), AGENTS.md. Started before the synced-skill fix and the gh*/npm deferral. |
| e2e, current defaults (`--private-tmp-pi-e2e--/…09-31-27…`) | **30,649** | 4 direct MCP tools + `tool_search`, 17 skills, no AGENTS.md |
| e2e, MCP off (`…09-21-08…`) | **12,191** | Same, without the `octocode` server |

Repo session with current defaults (deferred gh*/npm, AGENTS.md, synced skills excluded): about 35K − 2.3K = **≈ 32.7K**. A fresh `pi -p` session in this repo no longer lists synced skills (asked for `docx`: absent), and `dist/skills.js` `extraSkillDirs()` excludes `synced/` (`src/skills.ts:44-57`).

### Breakdown of the 30.6K default (chars → tokens at the measured ratios)

| Part | Chars | ≈ Tokens | Share |
|---|---|---|---|
| `localSearch` | 15,662 | 6,160 | 20% |
| `localAnalyzeGraph` | 12,245 | 4,820 | 16% |
| `lspGetSemantics` | 9,587 | 3,770 | 12% |
| `localGetFileContent` | 6,515 | 2,560 | 8% |
| **4 direct MCP tools** | **44,009** | **≈ 17,300** | **56%** |
| Extension tools (browser 2.9K, backlog 2.0K, askUser 1.8K, coordinate 1.7K, memory 1.7K, file 1.6K, agent 1.6K, sendMessage 1.3K, bash 0.9K, web 0.9K) | 16,434 | 6,470 | 21% |
| Skills section (17 skills) | 7,409 | 2,910 | 10% |
| `octocode` prompt section | 3,670 | 1,440 | 5% |
| Pi rules, docs, tools list, preamble, `read`, `tool_search`, `mcp_servers` | 6,394 | 2,500 | 8% |

`AGENTS.md` (`project_context`) adds 5,168 chars (≈ 2,000 tokens) in this repo.

### Octocode MCP schema duplication (octocode-mcp 19.1.0, `tools/list`)

Each tool's `queries.items` is an `anyOf` of 2–6 strict variants, one per `operation`. Every variant repeats the shared fields in full.

| Tool | Schema chars | Variants | Duplicated property chars | Flattened estimate |
|---|---|---|---|---|
| localSearch | 15,163 | 5 | 6,101 (40%) | 6,873 |
| localAnalyzeGraph | 11,905 | 6 | 7,943 (67%) | 2,998 |
| localGetFileContent | 5,954 | 4 | 3,251 (55%) | 2,665 |
| lspGetSemantics | 9,058 | 3 | 5,300 (59%) | 3,786 |
| **4 direct tools (desc + schema)** | **43,705** | | **22,595 (52%)** | **16,322 (−63%)** |
| All 9 tools | 80,670 | | 32,284 (40%) | 36,776 (−54%) |

Also: `goal` and `reasoning` appear in every variant (42 copies, 5,530 chars). The `path` description (*"Target path. GitHub tools: repo-relative… Local tools: absolute path."*, 99 chars) appears 14 times, also in local-only tools. There are 60 `additionalProperties: false` and 9 `$schema` keys. Server instructions (1,654 chars) are not in tool descriptions; Pi serves them through `describeNamespace`.

Flattening the 4 direct tools to about 16.3K chars saves about **11.3K tokens per request**.

## Competitor research

| Product | Lever | Source |
|---|---|---|
| Claude Code | Subagent frontmatter scopes `tools`, `disallowedTools`, `mcpServers`, `skills`, `omitClaudeMd` | [sub-agents](https://code.claude.com/docs/en/sub-agents) |
| Claude Code | Cost guidance; fan-out starts are staggered so later agents read the first agent's cached prefix | [costs](https://code.claude.com/docs/en/costs), [workflows](https://code.claude.com/docs/en/workflows) |
| Codex | Per-server `enabled_tools` / `disabled_tools` | [`codex-rs/config/src/mcp_types.rs:284`](https://github.com/openai/codex/blob/822e58cc3d666166c7446c5b1ea2e52f5d09594c/codex-rs/config/src/mcp_types.rs#L284), [`types.rs:1071-1075`](https://github.com/openai/codex/blob/822e58cc3d666166c7446c5b1ea2e52f5d09594c/codex-rs/config/src/types.rs#L1071) |
| Codex | Deferred tools (`defer_loading: true`) behind a `tool_search` handler | [`codex-rs/core/src/tools/handlers/tool_search.rs:74,411`](https://github.com/openai/codex/blob/822e58cc3d666166c7446c5b1ea2e52f5d09594c/codex-rs/core/src/tools/handlers/tool_search.rs#L74) |
| Codex | Custom agent TOML sets `mcp_servers` and `skills.config` | [subagents](https://learn.chatgpt.com/docs/agent-configuration/subagents) |
| OpenCode | Per-agent `tools` map with wildcards (`"mymcp_*": false`) | [agents](https://opencode.ai/docs/agents/) |

All three drop MCP tools per agent. Octocode profiles drop only whole servers (`mcp: false`). No competitor ships an 18K-token direct research toolset, so the upstream schema size is the main fix.

## Pi API constraints

- MCP exposure is per server with per-tool overrides: `toolExposure` keys are exact names or `*` patterns; exact names win, then the first matching pattern. Values: `direct`, `deferred`, `codemode`, `hidden` (Pi `docs/mcp.md`, "Control tool exposure"). Octocode sets `exposure: 'direct'` plus `toolExposure: { 'gh*': 'deferred', npmSearch: 'deferred' }` (`src/mcp/octocode.ts:31,55-56`).
- A user's `octocode` entry in `mcp.json` overrides the built-in registration (`octocode.ts:11-15`).
- `registerTool` accepts `exposure: 'deferred'`; `tool_search` finds and activates such tools (Pi `docs/extensions.md`, "Tool exposure").
- Pi lists name, description and path of every discovered skill (`docs/skills.md`). Extensions can only **add** paths through `resources_discover` (`src/skills.ts:66-71`). `BeforeAgentStartEventResult` offers only `message` or a full `systemPrompt` replacement (`dist/core/extensions/types.d.ts:1082-1086`). The CLI has `--no-skills` plus `--skill <path>` (`docs/cli.md:198-201`), which works for children because Octocode launches them.
- Codemode removes tool declarations; the model writes QuickJS scripts that call `tools.<name>()` and results are filtered (`docs/cli.md:145-168`). Cost: one `codemode` declaration plus a script round-trip.
- Declarations must not change mid-session. `tool_search` loads append (`docs/mcp.md`). Profile narrowing is fixed at spawn time, so it is cache-safe.

## Design

### 1. MCP exposure: `OCTOCODE_MCP_EXPOSURE` and per-profile `mcpTools:`

**`OCTOCODE_MCP_EXPOSURE` replaces the shipped `OCTOCODE_MCP_DIRECT`.** No alias (AGENTS.md: no compatibility shims). `src/shared/env.ts` removes `MCP_DIRECT_ENV`; README, `docs/FEATURES.md` and `docs/CONFIGURATION.md` change in the same commit.

| Value | Effect |
|---|---|
| `default` (or unset) | The built-in map: local + LSP direct, `gh*` and `npmSearch` deferred. Phase 2 also defers `localAnalyzeGraph` in the main session. |
| `direct` | All nine tools direct (old `OCTOCODE_MCP_DIRECT=1`). |
| `codemode` | The non-direct group uses `codemode` instead of `deferred`. Opt-in. |

`OCTOCODE_MCP_TOOLS` overrides single tools on top of any `OCTOCODE_MCP_EXPOSURE` value.

New optional profile frontmatter, parsed in `src/subagents/profiles.ts`:

```yaml
mcpTools: localSearch, localGetFileContent, lspGetSemantics   # these direct; every other octocode tool deferred
# or
mcpTools: "localSearch=direct, localAnalyzeGraph=deferred, gh*=hidden"   # explicit map
# or
mcpTools: none     # same as mcp: false
```

- `buildAgentEnv` (`src/subagents/process.ts:40-50`) passes it as `OCTOCODE_MCP_TOOLS=<normalized map>`.
- `octocodeServerConfig` (`src/mcp/octocode.ts:45-58`) builds `toolExposure` from `OCTOCODE_MCP_EXPOSURE`, then applies `OCTOCODE_MCP_TOOLS`. With a list, listed names become `direct` and a trailing `'*': 'deferred'` catches the rest.
- `hidden` lets read-only or web profiles block tools. Deferred tools stay reachable through `tool_search`, so narrowing moves capability behind search and does not remove it.
- When the map is narrowed, the prompt's research guidance (`prompt.ts:32`) names the deferred tools.

Bundled defaults:

| Profile | Direct | Deferred | Δ tokens |
|---|---|---|---|
| main session | localSearch, localGetFileContent, lspGetSemantics | localAnalyzeGraph, gh*, npmSearch | −4.8K |
| `implementer`, `reviewer` | localSearch, localGetFileContent, lspGetSemantics | rest | −4.8K |
| `researcher` | all local + LSP | gh*, npmSearch | 0 |
| `plan` (from 01) | localSearch, localGetFileContent, localAnalyzeGraph | rest | −3.8K |
| `webHeadless` / `webLive` | — (`mcp: false`) | — | 0 |

### 2. Extension tool narrowing

- Bundled `researcher` and `reviewer` add `backlog` to `excludeTools` (`process.ts:86-87`); a child proposes backlog items in its report.
- In subagents, register `memory` (read-only there) and `backlog` with `exposure: 'deferred'`: about −1.4K tokens per child.
- In the main session, defer `browser` (≈ 1.1K tokens) only if the Phase 3 eval shows no drop in browser task success.

### 3. Skill-list trimming

- New profile field `skills:` (comma list of names, or `none`). The child launches with `--no-skills` plus `--skill <path>` per name, resolved from the parent's discovered skills. Defaults: `researcher: octocode-research`; `implementer`, `reviewer`, `web*`: `none`. About −2.9K tokens per child.
- `/octocode skills` prints each skill's listing cost (chars, ≈ tokens, source dir) and the total, and points at `OCTOCODE_EXTRA_SKILLS=0` and Pi's `skills` settings.
- Upstream skill descriptions (`skills/` here and the `octocode` skills): 300 chars or less. Longest today: `octocode-clean-agentic-code` (567), `octocode-research` (471), `octocode-orchestrator` (462).

### 4. Upstream schema proposals (bgauryy/octocode)

Where the text lives:

- Published 19.1.0 (what Pi receives): `@octocodeai/octocode-tools-core/dist/schema.js:8` defines `{ goal, reasoning }` metadata merged into every strict variant. `dist/chunks/direct/chunk-BICVXQ5F.js` builds localSearch as `union([V,W,k,C,j])` with `operation: literal("text")` etc.
- Public source (main @ `c265e3f`): [`packages/octocode-tools-core/src/scheme/fields.ts:35-48`](https://github.com/bgauryy/octocode/blob/c265e3f9413161c900cbf4aa70d451b8e6b3920a/packages/octocode-tools-core/src/scheme/fields.ts#L35) (`createRelaxedBulkQuerySchema`), [`src/scheme/coreSchemas.ts:63-89`](https://github.com/bgauryy/octocode/blob/c265e3f9413161c900cbf4aa70d451b8e6b3920a/packages/octocode-tools-core/src/scheme/coreSchemas.ts#L63) (`describeQuerySchema`, `createQueryShapeSchema`), [`src/tools/local_ripgrep/scheme.ts:142-155,295`](https://github.com/bgauryy/octocode/blob/c265e3f9413161c900cbf4aa70d451b8e6b3920a/packages/octocode-tools-core/src/tools/local_ripgrep/scheme.ts#L142), and `src/tools/*/scheme.ts`. Field prose comes from `@octocodeai/octocode-core` (`src/tools/toolMetadata/descriptions.ts`). **The 19.1.0 variant builder is not on public `main`, `updates` or `octocode-20.0.0`.** The proposal targets the branch that owns it.

Proposals, by yield:

1. **Flatten operation variants on the wire.** One object per query: `operation` as an enum plus the union of fields, each described once. Keep the zod discriminated union for server-side validation; its errors already return hints. Add a short "fields per operation" line to the description. Yield: 4 direct tools 43.7K → 16.3K chars (≈ −11.3K tokens); all 9 80.7K → 36.8K. Prefer this to `$defs`/`$ref`, because provider support for references varies.
2. **Hoist `goal` and `reasoning`** to one top-level pair beside `queries`, or drop them from the wire: −5,530 chars.
3. **Split the `path` description** into local ("Absolute path.") and GitHub versions (14 copies).
4. **Strip `$schema` and redundant `additionalProperties: false`** (60 copies ≈ 1.7K chars). Strictness stays server-side.
5. **Upstream budget test:** CI fails if a tool's `tools/list` JSON grows more than 5% over its recorded budget.

Octocode does not rewrite schemas client-side; a local rewrite would drift from server validation.

### 5. Codemode

Codemode gives deferred gh*/npm tools zero declarations and lets scripts fan out with `Promise.allSettled` and filter output above 20 KB (`docs/mcp.md`). Decision: **`deferred` stays the default**, because direct calls keep Octocode's renderers, permission gates (01) and schema-guided arguments. `OCTOCODE_MCP_EXPOSURE=codemode` is the opt-in; Phase 3 measures its declaration cost and script failure rate.

## Measurement: `tests/token-budget.test.ts`

No `scripts/` change. One test file holds all measurement:

- **CI mode (default, no network, no model).** Asserts fixed character budgets: (a) bundled `octocode-mcp` `tools/list` schemas per tool (spawned over stdio: `initialize` + `tools/list`; skipped when the bundled server is absent); (b) extension tool declarations, recorded through a mock `pi` that captures `registerTool` (name, description, parameters, promptSnippet, promptGuidelines), with `OCTOCODE_SUBAGENT*` unset so `agent` and `askUser` register; (c) the `octocodePrompt()` section, main and subagent variants (≤ 3,800 chars); (d) `octocodeServerConfig` exposure maps per profile and per `OCTOCODE_MCP_EXPOSURE` value. A budget fails on more than 5% growth.
- **Report mode (opt-in).** `OCTOCODE_TOKEN_REPORT=<session.jsonl> yarn test tests/token-budget.test.ts` parses that session's first `system` and assistant entries and prints the per-section and per-tool breakdown, scaled to the reported usage. It asserts nothing.

## Targets

First-request tokens, from session JSONL usage:

| Context | Today | After phases 1–2 (local) | After upstream flatten |
|---|---|---|---|
| Main session, e2e defaults | 30.6K | **≤ 26K** | **≤ 19K** |
| Main session, this repo (18 skills + AGENTS.md) | ≈ 32.7K | ≤ 28K | ≤ 21K |
| `implementer` child | 30.6K | **≤ 22K** | **≤ 15K** |
| `reviewer` child | 30.6K | ≤ 22K | ≤ 15K |
| `researcher` child | 30.6K | ≤ 26K | ≤ 18K |
| `webHeadless` child | ≈ 12K | ≤ 10K | ≤ 10K |

For a 20-unit batch (05), the implementer prefix drops from about 610K to about 440K tokens locally and about 300K after upstream.

## Files to change

| File | Change |
|---|---|
| `src/shared/env.ts` | Remove `MCP_DIRECT_ENV` (`OCTOCODE_MCP_DIRECT`); add `OCTOCODE_MCP_EXPOSURE`, `OCTOCODE_MCP_TOOLS`, `OCTOCODE_TOKEN_REPORT`. |
| `src/mcp/octocode.ts` | `toolExposure` from `OCTOCODE_MCP_EXPOSURE` + `OCTOCODE_MCP_TOOLS`; main-session map defers `localAnalyzeGraph`; update the doc comment at line 28. |
| `src/subagents/profiles.ts` | Parse `mcpTools` and `skills`. |
| `src/subagents/process.ts` | `OCTOCODE_MCP_TOOLS` env; `--no-skills` + `--skill` args. |
| `src/prompt.ts` | Name deferred Octocode tools when narrowed. |
| `src/memory/*`, `src/backlog/*` | `exposure: 'deferred'` when `OCTOCODE_SUBAGENT=1`. |
| `src/skills.ts` | `/octocode skills` cost listing. |
| `subagents/*.md` | `mcpTools:` and `skills:` defaults; `backlog` excluded for researcher and reviewer. |
| `tests/token-budget.test.ts` (new) | CI budgets and `OCTOCODE_TOKEN_REPORT` mode. |
| `tests/mcp.test.ts`, `tests/turn.test.ts` | Replace `OCTOCODE_MCP_DIRECT` cases with `OCTOCODE_MCP_EXPOSURE=direct`. |
| `README.md`, `docs/CONFIGURATION.md`, `docs/FEATURES.md` | Replace `OCTOCODE_MCP_DIRECT`; document `OCTOCODE_MCP_EXPOSURE`, `OCTOCODE_MCP_TOOLS`, `mcpTools:`, `skills:`. Same change as the env rename. |
| Upstream `bgauryy/octocode`: `packages/octocode-tools-core/src/scheme/{fields,coreSchemas}.ts`, `src/tools/*/scheme.ts`, the variant builder behind `dist/schema.js`; prose in `@octocodeai/octocode-core` | Proposals 1–5. Then bump `OCTOCODE_MCP_VERSION` (`octocode.ts:21`) and `package.json` (manifest change: needs approval). |

## Phased plan

1. **Measure.** Add `tests/token-budget.test.ts` with today's budgets and the report mode.
2. **Local narrowing.** `OCTOCODE_MCP_EXPOSURE` (replaces `OCTOCODE_MCP_DIRECT`), `mcpTools`, `skills`, deferred `memory`/`backlog` in children, `localAnalyzeGraph` deferred in main. Lower the budgets.
3. **Evals.** A 20-task research/implement set with and without narrowing. Pass: `tool_search` calls for deferred tools in ≤ 15% of tasks, and no drop in task success. Also evaluate deferring `browser` in main and `OCTOCODE_MCP_EXPOSURE=codemode`.
4. **Upstream.** File the issue and PR in `bgauryy/octocode` with the duplication table, ship flattening, bump the pinned version, lower the budgets.

## Test plan

- **Unit:** profile parsing (`mcpTools` list, map and `none`; invalid names rejected with a notice). `octocodeServerConfig` maps per profile, exact-name precedence, each `OCTOCODE_MCP_EXPOSURE` value (`default`, `direct` declares all nine, `codemode`), and `OCTOCODE_MCP_TOOLS` overriding `OCTOCODE_MCP_EXPOSURE=direct`. `OCTOCODE_MCP_DIRECT=1` has no effect. `buildAgentArgs` emits `--no-skills --skill …` only when `skills:` is set. `memory` and `backlog` register deferred in subagent env.
- **Budget test:** fails on more than 5% growth; report mode prints a breakdown for a fixture session JSONL.
- **E2E (scripted model, stub MCP):** the child's `toolsAdded` contains exactly the expected direct tools. `tool_search` for `localAnalyzeGraph` loads it, and the next call succeeds.
- **Real Pi flow:** `pi --no-extensions -e dist/index.js -e builtin:mcp -e builtin:tool-search -p "say ok"` in the e2e folder; first-request usage meets the targets table. Repeat with an `implementer` child through `agent`, reading its usage from the run details (`tool.ts:184`). Then run `OCTOCODE_TOKEN_REPORT=<that session>` and record the breakdown.

## Open questions

1. Should `localAnalyzeGraph` stay direct in the main session for repos with frequent architecture questions (a per-project setting)?
2. Will upstream accept a flat wire schema, or prefer `$defs` with a Pi-side provider-support check?
3. Should the `octocode` prompt section (1.4K tokens) shrink further for children without `agent`? It already varies with `canDelegate`.

## Out of scope

- Pi's core system prompt, rules and docs section (about 1.8K tokens): upstream in Pi.
- Conversation-level savings: see [COMPACTION.md](../COMPACTION.md).
- Price-based budgeting. Tokens are the unit.
