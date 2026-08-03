# Spawn a Runner (Solver) Agent

Load when briefing any solver arm. One agent per arm per run — never share context across arms.

## What to include in the runner agent's context

**Always include:**
- The suite's `questions/<subject>/<bank>/questions.md` (verbatim, frozen)
- The arm name and its exact tool allowlist (from `references/suites.md`)
- The **model + settings** you froze for this run (same for every arm — see SKILL.md “Pick the model”)
- The run name and output paths it must write to
- The per-question `maxToolCalls`, `maxPages`, and `timeoutMs` from the bank
- Frozen commit SHAs for every mutable ref (orchestrator resolves these at preflight; runner does NOT resolve refs)

**Never include:**
- `ground-truth.json` — judge-only, must never reach any solver
- Another arm's answers, logs, or judge artifacts
- Token targets, quality targets, or scoring rubrics

## System prompt template for a runner agent

```
You are the solver for arm "<ARM_NAME>" in benchmark run "<RUN_NAME>".

Suite: <SUITE>
Bank: <BANK_PATH>/questions.md (attached)
Output dir: output/<RUN_NAME>/answers/<SUITE>/<ARM>/
Log dir:    output/<RUN_NAME>/logs/<SUITE>/<ARM>/

ALLOWED TOOLS: <see tool allowlist below>
FORBIDDEN: all other tools, including ground truth, other arm outputs, and browser.

For every question Q<NN>:
1. Emit [RUNNER] Q<NN> START arm=<ARM_NAME>
2. Research using only allowed tools. Emit [RUNNER] Q<NN> TOOL <name> raw_bytes=<N> read_bytes=<N> per call.
3. If evidence within budget does not support a claim, write Unknown — never infer.
4. Write answer_Q<NN>.md (template below).
5. Write Q<NN>.jsonl — one JSON line per tool call: {"id":N,"tool":"...","exitCode":0,"ms":N,"rawBytes":N,"readBytes":N}
6. Emit [RUNNER] Q<NN> DONE runner_tokens=<N|Unavailable> est_tokens=<N> calls=<N> ms=<N>
   Report per-question runner tokens when the harness exposes them; always also
   report est_tokens = (readBytes_q + answerChars_q)/4. If runner tokens are
   unavailable, write Unavailable — never fabricate. (See references/measurements.md.)

After ALL questions are done:
7. Write solver-output.json (schemas/solver-output.schema.json) — machine-readable twin of all answers and
   logs. Set runnerTokens to the whole-trial provider total; if unavailable set to null and populate
   tokenFallback with the estimation method (e.g. "readBytes/4").
```

## Answer file template (answer_Q<NN>.md)

```markdown
# Q<NN> — <short title>

## Run metadata
- Arm: <ARM_NAME>
- Model: <model-id>
- Started (UTC): <timestamp>
- Finished (UTC): <timestamp>
- Wall time: <ms>
- Runner tokens: input=<N> output=<N> cache_read=<N> cache_write=<N>  [or: Unavailable]
- Tool-output bytes: raw=<N> read=<N>
- Research calls: <N> (including failures, retries, pagination)

## Answer
<answer every subpart of the question directly; cite repo@ref:path:lines for every claim>

## Evidence
- `<repo>@<ref>:<path>:<start-end>` — <what this region proves>

## Gotchas
- <drift, truncation, unsupported premise, budget hit, or Unknown — be explicit>
```

## Tool allowlists by arm

**Octocode MCP (Arm B, GitHub suites):**
`ghSearchCode`, `ghGetFileContent`, `ghViewRepoStructure`, `ghSearchRepos`,
`ghSearchPullRequests`, `ghSearchIssues`, `ghSearchCommits`
— No CLI, no local clone, no AST/LSP, no npm registry, no browser, no raw GitHub API.

**gh CLI (Arm A, octocode-vs-gh):**
`gh search code`, `gh api repos/:owner/:repo/contents`, `gh api repos/:owner/:repo/git/trees`,
`gh pr view`, `gh pr diff`, `gh issue view`, `gh search commits`, and equivalent `gh` read operations.
— No rtk, no external shapers, no non-gh tools.

**gh + rtk (Arm A, octocode-vs-gh-rtk):**
Same `gh` operations as above. `rtk` (third-party stdout shaper, pinned in
`manifest.md` `baselines.rtk`) may only filter or reshape `gh` stdout — it adds
no new research source.
— No gh operations `rtk` cannot reach; no extra tools.
— **HARD:** any `gh` call whose `rawBytes` > 50 KB MUST be piped through `rtk`
  before the payload enters your context. Reading a raw >50 KB `gh` payload
  unfiltered makes the trial invalid (it will be re-run) — it measures your
  discipline, not the tool.
— **HARD:** the question's `maxToolCalls` is a hard cap; exceeding it invalidates
  the trial (both arms are held to this).

**ast-grep (Arm A, octocode-vs-ast-grep):**
`ast-grep run -p '<pattern>' <path>`, `ast-grep scan --inline-rules`, `ast-grep outline`.
Local corpus only at the pinned React checkout. No GitHub access, no file reads beyond ast-grep output.

**Octocode CLI (Arm B, octocode-vs-ast-grep):**
`node packages/octocode/out/octocode.js` with `localSearchCode mode:"structural"` and other local surfaces.
Same pinned corpus. No GitHub access, no ast-grep calls.

## Control arm (no tools)

Use the same system prompt but set ALLOWED TOOLS to none. The control arm answers from memory
only — no tool calls. Its role is contamination detection; its answers are not scored for quality.
