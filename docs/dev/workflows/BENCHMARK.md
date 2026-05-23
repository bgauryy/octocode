# CLI + Skill vs MCP — Agent Benchmark

Real-agent harness (via `claude -p`) that measures end-to-end performance when an autonomous agent answers research tasks using **either** `octocode-cli` + skill, **or** the Octocode MCP server. Used to validate CLI+skill as a viable alternative access path (terminal, pipelines, CI) and track regressions.

- **Variant A** — MCP: agent has `mcp__octocode__*` tools, no shell access.
- **Variant B** — CLI+skill: agent has `Bash(bench-cli:*)` only, MCP disabled, with `skills/octocode-cli/SKILL.md` injected via `--append-system-prompt`.

Symmetric, n=10 per task per variant, 5 tasks, trials interleaved.

## Task design principle

Tasks mirror the **real usage archetypes** observed in Claude Code chat history across hundreds of sessions: deep repo exploration, npm package investigation, library usage pattern search, PR archaeology, comparative multi-repo research. Earlier synthetic tasks (symbol lookup via LSP, call-chain trace) were moved to an appendix — they exercised tools (`lspGotoDefinition`, `lspCallHierarchy`) that have zero real-world invocations in observed sessions.

Tool coverage across the primary catalog:

| Tool | R1 | R2 | R3 | R4 | R5 |
|---|---|---|---|---|---|
| `packageSearch` | ✓ | | | | |
| `githubSearchCode` | | ✓ | | | ✓ |
| `githubGetFileContent` | ✓ | ✓ | ✓ | ✓ | ✓ |
| `githubViewRepoStructure` | ✓ | | ✓ | | ✓ |
| `githubSearchPullRequests` | | | | ✓ | |
| `githubSearchRepositories` | | | | | ✓ |

## Prerequisites

- `claude` CLI (Claude Code) with MCP server `octocode` configured (for variant A).
- Node 22+, yarn, working copy of `octocode-mcp` monorepo.
- `jq`, `gtime`/`/usr/bin/time -p`, `gh` CLI (for ground-truth pinning).
- A CLI build of `octocode-cli` you want to benchmark.

## Setup

### 1. Build the CLI you want to measure

```bash
YARN_ENABLE_SCRIPTS=0 yarn --cwd packages/octocode-cli build:dev
```

This emits `packages/octocode-cli/out/octocode-cli.js`. If you're benchmarking a branch in a separate worktree, build there and symlink `node_modules` if needed:

```bash
ln -sf ../octocode-mcp/node_modules node_modules
YARN_ENABLE_SCRIPTS=0 yarn --cwd packages/octocode-cli build:dev
```

### 2. Create the CLI wrapper the agent will invoke

```bash
cat > /tmp/bench-cli <<'EOF'
#!/usr/bin/env bash
exec node /ABSOLUTE/PATH/TO/packages/octocode-cli/out/octocode-cli.js "$@"
EOF
chmod +x /tmp/bench-cli
/tmp/bench-cli --version   # confirm it runs
```

Use the wrapper name `bench-cli` (not `octocode-cli`). That keeps `--allowedTools "Bash(bench-cli:*)"` scoped tight and lets you benchmark arbitrary builds without touching `$PATH` globally.

### 3. Verify MCP variant still works

```bash
claude -p --allowedTools "mcp__octocode__githubSearchCode" \
  --output-format stream-json --verbose \
  "ping" >/dev/null && echo OK
```

### 4. Pin ground truth for this batch

Task answers drift (repos change, PRs merge, packages release). Pin fresh ground truth **immediately before** starting a batch, and score every trial in the batch against that pinned snapshot:

```bash
# R1 — zod npm metadata + repo
npm view zod version repository.url
gh api repos/colinhacks/zod --jq '.default_branch'

# R2 — test files using z.discriminatedUnion in zod
gh search code --repo=colinhacks/zod 'discriminatedUnion path:packages/zod/tests' --limit 20

# R3 — python-sdk top-level layout
gh api repos/modelcontextprotocol/python-sdk/contents --jq '.[] | select(.type=="dir") | .name'

# R4 — most recent merged PR touching src/compiler/ in TypeScript
gh pr list --repo microsoft/TypeScript --state merged --search 'path:src/compiler' --limit 5 \
  --json number,title,mergedAt,files

# R5 — vite + webpack CLI entry files
gh api repos/vitejs/vite/contents/packages/vite/bin --jq '.[].name'
gh api repos/webpack/webpack/contents/bin --jq '.[].name'
```

Save the output into `/tmp/bench-ground-truth-YYYYMMDD.txt` and reference it during scoring.

## Tasks

5 tasks grounded in real usage archetypes. Prompts are exact — the model must respond with compact JSON only, no other text.

### R1 — NPM package investigation (archetype: package → source)

> Research the npm package `zod`. Identify its source repository and the primary public entry point file in the repo. Respond with compact JSON only: `{"package":"zod","repo":"<owner/repo>","currentMajor":N,"entryFile":"<path-from-repo-root>"}`. Do not include any other text.

**Expected tool shape:** `packageSearch` → `githubViewRepoStructure` and/or `githubGetFileContent` to confirm entry file.

**Scoring:**
- `package` must equal `"zod"` exactly.
- `repo` must equal `"colinhacks/zod"` (accept with or without `https://github.com/` prefix — strip before compare).
- `currentMajor` must match the integer major version from `npm view zod version` at batch time (±0).
- `entryFile` must resolve on `colinhacks/zod` default branch AND contain either an `export` statement or be referenced as `main`/`module` in the repo's published `package.json`. Accept any of: `packages/zod/src/index.ts`, `packages/zod/src/v4/classic/external.ts`, `src/index.ts` (path depends on zod version at batch time).

### R2 — Library usage examples (archetype: search-and-read loop)

> In `colinhacks/zod` on its default branch, find three distinct test files that exercise `z.discriminatedUnion()`. Respond with compact JSON only: `{"examples":[{"file":"<path>","line":N},{"file":"<path>","line":N},{"file":"<path>","line":N}]}`. Each `file` must be a test file (path contains `test` or ends in `.test.ts`/`.spec.ts`). Each `line` is the line where `discriminatedUnion` is called. Do not include any other text.

**Expected tool shape:** `githubSearchCode` with query `discriminatedUnion` + path filter → `githubGetFileContent` on 2–3 hits to confirm line.

**Scoring:**
- Exactly 3 entries.
- Each `file` path must resolve on default branch AND match `/test|spec/`.
- Each `line` must land within ±2 of a real `discriminatedUnion` call site in that file (verify with `gh api repos/colinhacks/zod/contents/<path>` or `git show`).
- The 3 files must be distinct.

### R3 — Deep repo orientation (archetype: "tell me about repo X")

> Research the GitHub repo `modelcontextprotocol/python-sdk` on its default branch. Respond with compact JSON only: `{"purpose":"<one-sentence description>","topDirs":[{"name":"<dir>","role":"<one-line>"},{"name":"<dir>","role":"<one-line>"},{"name":"<dir>","role":"<one-line>"}],"entryPoint":{"file":"<path-from-repo-root>","symbol":"<exported-function-or-class>"}}`. `topDirs` must contain exactly three entries chosen from the repo's top-level directories. Do not include any other text.

**Expected tool shape:** `githubViewRepoStructure --depth 1` → `githubGetFileContent` on README → `githubGetFileContent` on the source entry file(s).

**Scoring:**
- `purpose` must be non-empty and substantively correct — reject if it says "a tool for X" when the repo is for Y. Human-scored rubric; accept plausible paraphrases.
- `topDirs[].name` must each be a real top-level directory of the repo on default branch. Verify with `gh api repos/modelcontextprotocol/python-sdk/contents --jq '.[] | select(.type=="dir") | .name'`.
- `topDirs[].role` must be non-empty, non-hallucinated (directory must plausibly match the role). Human-scored.
- `entryPoint.file` must resolve on default branch AND contain an export of `entryPoint.symbol`. Accept `src/mcp/server/fastmcp.py` + `FastMCP`, `src/mcp/__init__.py` + any publicly exported class, or similar.

### R4 — PR archaeology (archetype: change investigation)

> In `microsoft/TypeScript`, find the most recent merged pull request that modifies at least one file under the `src/compiler/` directory. Respond with compact JSON only: `{"prNumber":N,"title":"<exact title>","mergedAt":"<ISO-8601 date>","changedFile":"<one file path under src/compiler/ changed by the PR>"}`. Do not include any other text.

**Expected tool shape:** `githubSearchPullRequests` with path filter → optional `githubGetFileContent` to confirm one changed file.

**Scoring:**
- `prNumber` must match the top result of `gh pr list --repo microsoft/TypeScript --state merged --search 'path:src/compiler' --limit 1` at batch pin time. Accept the top 3 results to tolerate race with merges during the batch.
- `title` must exactly match the PR's title on GitHub.
- `mergedAt` must match the PR's `merged_at` date (accept date-only or full ISO, UTC).
- `changedFile` must be a path under `src/compiler/` present in the PR's file list (verify with `gh api repos/microsoft/TypeScript/pulls/N/files`).

### R5 — Comparative research (archetype: multi-repo comparison)

> Compare how `vitejs/vite` and `webpack/webpack` expose their command-line entry point. For each repo, on its default branch, identify the file that serves as the CLI bin target and the name of the export or function that boots the CLI. Respond with compact JSON only: `{"repos":[{"name":"vitejs/vite","cliFile":"<path>","cliSymbol":"<exported function or identifier>"},{"name":"webpack/webpack","cliFile":"<path>","cliSymbol":"<exported function or identifier>"}]}`. Do not include any other text.

**Expected tool shape:** `githubViewRepoStructure` × 2 → `githubGetFileContent` on each repo's `package.json` (to find the `bin` field) → `githubGetFileContent` on the referenced CLI files.

**Scoring:**
- Both repos present in order given.
- `vitejs/vite` — `cliFile` must resolve on `vitejs/vite` default branch AND be referenced by the `bin` field of some `package.json` in the repo, OR be a path under `packages/vite/bin/` that's executable. Accept paths like `packages/vite/bin/vite.js`, `packages/vite/src/node/cli.ts`.
- `webpack/webpack` — `cliFile` similarly must resolve and be a legitimate CLI entry. Accept paths like `bin/webpack.js`, `lib/webpack.js`.
- `cliSymbol` must be present in the referenced file as an exported name, assignment target, or invoked function. Accept loose identifier match (e.g. `runCLI`, `bootstrap`, `webpack`).

## Invocation

Set these once:

```bash
TASK_PROMPT="<task prompt from above>"
TASK=R1             # R1..R5
TRIAL=1             # 1..10
LOG=/tmp/bench-<variant>-${TASK}-${TRIAL}
```

### Variant A — MCP

```bash
/usr/bin/time -p claude -p \
  --permission-mode acceptEdits \
  --allowedTools \
    "mcp__octocode__githubSearchCode" \
    "mcp__octocode__githubGetFileContent" \
    "mcp__octocode__githubViewRepoStructure" \
    "mcp__octocode__githubSearchRepositories" \
    "mcp__octocode__githubSearchPullRequests" \
    "mcp__octocode__packageSearch" \
  --output-format stream-json --verbose \
  --include-partial-messages \
  "$TASK_PROMPT" \
  > "$LOG.jsonl" 2> "$LOG.time"
```

### Variant B — CLI + skill

```bash
SKILL=$(cat /ABSOLUTE/PATH/TO/skills/octocode-cli/SKILL.md)
PATH="/tmp:$PATH" /usr/bin/time -p claude -p \
  --permission-mode acceptEdits \
  --allowedTools "Bash(bench-cli:*)" \
  --disallowedTools "mcp__octocode__*" \
  --output-format stream-json --verbose \
  --include-partial-messages \
  --append-system-prompt "$SKILL" \
  "$TASK_PROMPT" \
  > "$LOG.jsonl" 2> "$LOG.time"
```

### Safety timeout

Wrap each invocation in a 300s alarm to bound tail latency (some R-tasks can loop on approval prompts):

```bash
perl -e 'alarm 300; exec @ARGV' claude -p ...
```

### Interleaving

Run in order `R1-t1, R2-t1, R3-t1, R4-t1, R5-t1, R1-t2, …` — blunts warmup/cache bias that otherwise favors whichever task runs first. 50 runs per variant = ~45–75 min depending on task complexity (R3 and R5 are the long-tail tasks).

## Metrics extraction

Per run:

```bash
# Wall-clock (seconds)
grep real "$LOG.time" | awk '{print $2}'

# Token accounting + turns
grep '"type":"result"' "$LOG.jsonl" \
  | jq -c '{input: .usage.input_tokens,
            output: .usage.output_tokens,
            cache_read: .usage.cache_read_input_tokens,
            cache_create: .usage.cache_creation_input_tokens,
            turns: .num_turns}'

# Final answer (what you score)
grep '"type":"result"' "$LOG.jsonl" | jq -r '.result'
```

### Effective cost

`cache_read × 0.1 + cache_create × 1.25 + input + output × 5`

Rough Sonnet per-session billing proxy. Use for apples-to-apples token comparisons across variants; absolute $ isn't the point.

### Scoring

Validate each run's final JSON against the pinned ground truth. Record `{pass: bool, reason: string}` per run. See per-task rubrics above. Key principles:

- **Exact match for opaque identifiers** (package names, repo names, PR numbers).
- **Resolve-on-default-branch check** for any path the model claims exists — use `gh api` or octocode's own view-structure.
- **Human sanity check for free-text fields** (R3 `purpose`, `topDirs[].role`). Don't require exact string match; require "plausibly correct" per rubric.
- **Score against the pinned snapshot**, not the live repo, so mid-batch merges don't invalidate earlier trials.

## Output format

Write one summary JSON per variant, `/tmp/bench-<variant>-multi-summary.json`:

```json
{
  "variant": "mcp" | "cli-skill-v2" | ...,
  "cli_binary": "<branch @ short-sha>",
  "skill_sha": "<sha of skills/octocode-cli/SKILL.md>",
  "ground_truth_pinned_at": "<ISO-8601>",
  "runs": [
    {"task":"R1","trial":1,"time":42.1,"input":12,"output":432,
     "cache_read":58000,"cache_create":24000,"turns":5,
     "eff_cost":48000,"pass":true,"reason":""}
  ],
  "per_task": {
    "R1": {"n":10,"pass_rate":"10/10","time_median":40,"time_mean":45,
           "eff_cost_median":52000,"eff_cost_mean":58000},
    "R2": {}, "R3": {}, "R4": {}, "R5": {}
  },
  "overall": {"n":50,"pass_rate":"48/50","time_median":58,"eff_cost_median":89000}
}
```

## Interpreting results

Report per-task and overall medians. **Do not collapse to a single "winner" number** — the multi-task benchmark is designed to surface that task shape matters. Include a comparison table and state which tasks each variant won.

Expect per-task medians in the 40–120s range for R1–R5 (vs 30–100s for the legacy T-tasks) because the real archetypes are read-heavy: the agent typically makes 8–20 tool calls per task, not 3–5.

Known failure modes:
- **Approval loop** (CLI+skill): occasional 300s timeout. Record as fail with `reason="timeout"`; don't retry.
- **Stream truncation** (either variant): last JSON incomplete. Record as fail with `reason="stream truncated"`.
- **Ground-truth drift mid-batch**: a PR merges during R4 trials, shifting the "most recent" answer. Pin at batch start and accept top-3 to tolerate this.
- **R3 role hallucination**: model describes a directory's role based on its name alone, not on contents. Sanity-check by sampling one file from each claimed dir.

## Prior results

Historical comparison tables (old T1–T4 synthetic benchmark) preserved in [PR #387](https://github.com/bgauryy/octocode-mcp/pull/387). R1–R5 results will be published in follow-up PRs as they're run.

## Reproduction checklist

- [ ] CLI built, `/tmp/bench-cli --version` works
- [ ] `claude` CLI can reach MCP server (variant A)
- [ ] `SKILL.md` present at `skills/octocode-cli/SKILL.md`
- [ ] Task prompts copied verbatim (no paraphrasing)
- [ ] Ground truth pinned for this batch against live public repos (zod, python-sdk, TypeScript, vite, webpack)
- [ ] n=10 each, trials interleaved across R1–R5
- [ ] 300s alarm wraps each `claude -p` invocation
- [ ] Per-run JSONL kept (`/tmp/bench-<variant>-R*-*.jsonl`) for audit
- [ ] Summary JSON written for each variant
- [ ] Results compared per-task, not just overall

---

## Appendix: Legacy synthetic micro-benchmark (T1–T4)

The original benchmark ran 4 synthetic tasks against the octocode-mcp repo itself. These are kept as regression-smoke tests because they're cheap and deterministic, but they should not be used for headline claims: two of the four rely on LSP tools (`lspGotoDefinition`, `lspCallHierarchy`) that have zero real-world usage in observed chat history, and all four target a single repo the model may have partial training-data memory of.

<details>
<summary>T1–T4 prompts (reference)</summary>

**T1** — Symbol lookup: find `runCLI` definition + 3 callees in `bgauryy/octocode-mcp`. Compact JSON `{"definition":{"path":"...","line":N},"calls":[...]}`.

**T2** — Workspace mapping: list `packages/` entries in `bgauryy/octocode-mcp` with descriptions. Compact JSON `{"packages":[...]}`.

**T3** — Cross-repo export search: find where `discriminatedUnion` is defined in `colinhacks/zod`. Compact JSON `{"repo":"colinhacks/zod","file":"...","symbol":"..."}`.

**T4** — Call-path trace: starting from `runCLI`, list the chain of calls reaching `executeToolCommand`. Compact JSON `{"chain":[{"from":"...","to":"...","file":"...","line":N},...]}`.

Ground truth: score against `origin/main` of each referenced repo. Line-number tolerance ±2 (T1, T2), ±3 (T4).

</details>

Results from the T1–T4 runs (MCP baseline, CLI+skill-p1, CLI+skill-p2, CLI+skill-v2 on p2) are archived in the PR #387 body.
