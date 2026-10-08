# 05 — Fan-out batches: 5–30 isolated subagents with a merge queue

**Status:** Proposed · **Priority:** P2 · **Lane:** Scale & cost
**Depends on:** [01 — Permissions](01-permissions.md) (per-profile policy, tighten-only merge), [03 — Agent wait and context](03-agent-wait-and-context.md) (`coordinate wait`, `agent` `context`), [04 — Agents view](04-agents-view.md) (`/agents` overlay). Token figures come from [06 — Token budget](06-token-budget.md).

## Problem and evidence

Octocode can isolate one subagent in a git worktree, but it cannot run a large change as a batch.

| Today | Where | Limit for batches |
|---|---|---|
| At most `OCTOCODE_MAX_SUBAGENTS` (default 3) children run at once. An extra call is **refused**, not queued. | `src/subagents/tool.ts:55-69`, refusal at `tool.ts:112` | The model must poll and retry: about seven rounds for 20 units. |
| `isolate: true` starts a detached worktree at HEAD under `<home>/pi-worktrees/<repo-hash>/<id>`. On finish it commits with hooks off and saves `refs/octocode/pi/<id>`. | `src/subagents/worktree.ts:73-104`, `128-155`, `171-178` | Good base. One commit per run, no verify gate. |
| `/agents merge <id>` runs `git merge --no-ff` into the **checked-out tree**; on conflict it aborts and keeps the ref. | `worktree.ts:194-210`, `command.ts:63-73` | One merge at a time into the user's tree; no rebase, retry or test run. |
| `pruneWorktrees` saves orphaned worktrees to refs at session start. | `worktree.ts:225-262`, called at `tool.ts:291` | Work survives a crash, but nothing maps it to a unit. |
| Child usage is counted per run (`details.input = input + cacheRead + cacheWrite`). | `tool.ts:184-185`, `209` | No batch total and no budget. |

Cost baseline (06): a default-profile child's first request costs about **30.6K input tokens** (≈ 31K; Pi session `--private-tmp-pi-e2e--/2026-10-06T09-31-27…jsonl`). A 20-unit batch pays about 610K prefix tokens before any work. Caching turns most of that into cache reads only if the prefix is identical and the first write lands before the rest start.

## Competitor research

| Product | Fan-out and concurrency | Isolation, verify, resume | Cost guardrails |
|---|---|---|---|
| Claude Code `/batch` ([commands](https://code.claude.com/docs/en/commands), [sub-agents](https://code.claude.com/docs/en/sub-agents)) | Researches, splits into **5–30 units**, shows a plan, starts one background subagent per unit after approval. Session limit **20 running** (`CLAUDE_CODE_MAX_CONCURRENT_SUBAGENTS`). | One worktree per unit; each unit runs tests and publishes its change. | Plan approval. |
| Claude Code workflows ([workflows](https://code.claude.com/docs/en/workflows)) | Rerunnable script, "dozens to hundreds of agents". **16 concurrent** by default (1–256, `CLAUDE_CODE_WORKFLOW_MAX_CONCURRENT_AGENTS`), 1,000 per run, 4,096 items per `parallel()`. | Isolated copies. Independent agents review each other's findings; `/deep-research` drops claims that fail cross-checks. Resumable: finished agents return from cache. | Warning above **25 agents** or **1.5M** projected tokens. Fan-out agents start **up to 5 s after the first** to read its cached prefix. |
| Codex | `spawn_agent`, `wait_agent`, `send_input`, `close_agent`, `resume_agent` (`codex-rs/core/src/tools/handlers/multi_agents/*.rs`). `DEFAULT_AGENT_MAX_THREADS = Some(6)`, `DEFAULT_AGENT_MAX_DEPTH = 1` ([`config/mod.rs:256,266`](https://github.com/openai/codex/blob/822e58cc3d666166c7446c5b1ea2e52f5d09594c/codex-rs/core/src/config/mod.rs#L256)); override `agents.max_concurrent_threads_per_session` ([subagents](https://learn.chatgpt.com/docs/agent-configuration/subagents)). | `.codex/agents/*.toml` with `sandbox_mode`, `mcp_servers`, `skills.config`. No merge queue. `resume_agent`. | Per-agent model and effort. |
| OpenCode ([agents](https://opencode.ai/docs/agents/)) | `general` subagent runs units in parallel; `explore` is read-only. Concurrency not documented. | Navigable child sessions; no merge. | `steps` caps iterations; `permission.task` limits spawnable subagents. |

Takeaways:

1. Plan-then-approve with 5–30 units is the norm. Octocode matches it and adds a merge queue that integrates and re-tests.
2. Batch concurrency elsewhere is 6–20. Our interactive cap of 3 stays; a batch gets its own pool.
3. Cross-check review and resume from recorded results fit a journal-driven engine.
4. Warn above 25 units or 1.5M tokens; stagger starts so the prefix is cached once.

## Pi API constraints

- Children are separate `pi` processes (`src/subagents/process.ts:72-90`) launched with `--no-extensions -e <ext> -e builtin:mcp -e builtin:tool-search`. A batch reuses `runSubagent`, not the in-process SDK.
- `pi.registerCommand` handlers can call `ctx.ui.confirm` and `ctx.ui.select`. Print and JSON modes have no UI (`ctx.hasUI === false`), so approval there needs an explicit opt-in.
- Background reports reach the model through `pi.sendMessage(..., { deliverAs: 'followUp' })` (`ReportQueue`, `src/subagents/handoff.ts:78-100`). The batch summary uses this path, so 03's `coordinate wait` can claim it.
- Every declared tool costs tokens on every request (06), so the entry point is one optional parameter on `agent`, not a new tool.
- `tests/architecture.test.ts`: `src/batch/` may depend only on `shared`, `agentdb`, `team` and `subagents`.

## Design

### Decision: engine in code, `agent items[]`, `/batch` command, no skill

| Option | Verdict |
|---|---|
| Skill only | Rejected: a skill cannot queue past the cap, keep a journal, re-run tests or resume. |
| New `batch` tool | Rejected: about 1K schema tokens on every request for a rare feature. |
| **`agent` gains `items[]`; engine in `src/batch/`; `/batch` for users** | **Chosen.** About 600 schema chars on an existing tool. The engine owns queueing, journal, verify and merge. |

### Entry points

**Model:** when `items` is present, `agent` starts a batch and returns a batch id at once:

```ts
items?: Array<{
  name: string;            // unique unit label, [a-z0-9-]{1,40}
  task: string;            // unit scope; the shared `task` is prepended as common context
  paths?: string[];        // files or dirs the unit expects to touch (overlap check + reservations)
}>;                        // 2..30 items (hard max 50 with OCTOCODE_BATCH_MAX_ITEMS)
verify?: string;           // run in each worktree and on the integration branch, e.g. "yarn test"
review?: 'none' | 'sample' | 'all';   // reviewer cross-check (default 'sample')
```

With `items`, `isolate` and `background` are forced true. `context` (03) is `'summary'`; `'fork'` is refused because it copies the parent context N times. `profile` applies to every item (default `implementer`).

**User:** `/batch <instruction>` sends a planning turn: research, split into 5–30 independent units with disjoint `paths`, then call `agent` with `items`. Subcommands: `status [id]`, `resume <id>`, `cancel <id>`, `retry <id> <item>`, `land <id>`. `/agents merge` stays for single runs.

### Flow

```
plan ──► approve ──► dispatch queue ──► per-item: run → verify → (review) ──► merge queue ──► summary ──► /batch land
           │            │ (concurrency N, staggered)                         │ cherry-pick onto integration ref
           │            └─ journal every transition                          │ verify every K merges + at end
           └─ UI confirm; headless needs OCTOCODE_BATCH_AUTO=1               └─ conflict → 1 fixer retry → else park
```

1. **Plan check.** Reject duplicate names. Warn on overlapping `paths`. Estimate cost as `items × (prefix ≈ 31K + 120K working budget)`; with 06 local narrowing the implementer prefix is ≤ 22K.
2. **Approve.** With a UI, `ctx.ui.confirm` lists units, profile, verify command and estimate, with a "Large batch" warning above 25 items or 1.5M tokens. Without a UI the call is refused unless `OCTOCODE_BATCH_AUTO=1`.
3. **Dispatch.** FIFO queue with its own pool, `OCTOCODE_BATCH_CONCURRENCY` (default 4, range 1–8), outside `OCTOCODE_MAX_SUBAGENTS`, so interactive `agent` calls still work. The first child starts alone; the rest start after its first model response or 5 s, whichever is first, to read its cached prefix. Each child gets the merged policy from 01 as `OCTOCODE_PERMISSIONS_POLICY` (tighten-only). A headless `ask` becomes `deny`.
4. **Verify each.** After the child exits and **before** its worktree is removed, the engine runs `verify` there (timeout `OCTOCODE_BATCH_VERIFY_MINUTES`, default 15). The child's own test claims are hints only. Exit 0 → `verified`; non-zero → `failed-verify`, ref kept.
5. **Cross-check.** `review: 'all'` starts a `reviewer` child on each verified diff (`git diff base..ref`); `'sample'` reviews every third unit plus every diff over 400 lines. The report must end with `VERDICT: pass` or `VERDICT: fail <reason>`; `fail` parks the unit as `rejected`. Reviewers share the pool.
6. **Merge queue (serial, plan order).** Create `refs/octocode/batch/<batch-id>` at the batch base. In a dedicated integration worktree, per verified unit:
   - `git cherry-pick --no-verify base..ref` (hooks off, as `NO_HOOKS`).
   - On conflict: `cherry-pick --abort`, then one fixer `implementer` child in a fresh worktree at the integration head, given the unit's task, conflicting paths and failed diff. Its ref replaces the unit's ref. A second conflict parks the unit as `conflict`, ref kept.
   - Every `K` merges (default 5) and after the last, run `verify` on the integration worktree. On failure, reset to the last green commit and bisect the last K picks. Park the culprit as `breaks-integration` and continue.
7. **Summary.** One follow-up message under 8 KB (full report in the batch folder): per-unit state, ref, diffstat, tokens, verdict; total tokens; next steps. `coordinate wait { ids: ['batch-<id>'] }` (03) returns it inline.
8. **Land.** `/batch land <id>` runs `git merge --no-ff` of the integration ref into the checked-out branch and aborts on conflict. The user's tree does not change before this step. Parked units keep `refs/octocode/pi/<id>` for `/agents merge`.

### Journal and resume

File: `<Octocode home>/pi-batches/<repo-hash>/<batch-id>.json`, written with `atomicWriteFileSync` (`src/shared/atomic.ts`) on every transition. Owner records follow the `worktree.ts` `.json` sidecars.

```json
{ "id": "b-7f3a", "repo": "/abs/repo", "base": "<sha>", "integration": "refs/octocode/batch/b-7f3a",
  "profile": "implementer", "verify": "yarn test", "review": "sample", "ownerPid": 4242,
  "budget": { "maxTokens": 3000000, "usedTokens": 812345 },
  "items": [{ "name": "auth-routes", "state": "merged", "agentId": "implementer-1a2b", "ref": "refs/octocode/pi/implementer-1a2b",
              "attempts": 1, "tokens": 140211, "verify": { "exit": 0, "log": "…/auth-routes.verify.log" }, "review": "pass" }] }
```

States: `queued → running → finished → verified → (reviewed) → merged`. Terminal parks: `failed`, `failed-verify`, `rejected`, `conflict`, `breaks-integration`, `cancelled`.

At session start, `pruneWorktrees` runs first. The batch module then finds journals with a dead `ownerPid` and notifies "Batch b-7f3a stopped at 9/20; `/batch resume b-7f3a`". On resume:

- `running` with a ref saved by prune → `finished`, then verified. Not re-run.
- `running` with no ref → `queued`.
- `merged` items are checked with `git merge-base --is-ancestor` against the integration ref; the queue continues from the first unmerged item.

### Cost guardrails

| Guard | Default | Behavior |
|---|---|---|
| `OCTOCODE_BATCH_MAX_ITEMS` | 30 (hard max 50) | Refuse above the max; warn above 25. |
| `OCTOCODE_BATCH_MAX_TOKENS` | 3,000,000 | Sum of `input + cacheRead + cacheWrite + output`. Notify at 80%. At 100%, stop dispatching, let running items finish, leave the rest `queued` (resumable with a higher budget). |
| Per-item token cap | 400,000 | Stop the child (as `coordinate stop`); park `failed`, reason "token cap". |
| `OCTOCODE_SUBAGENT_MAX_MINUTES` / idle timeout | existing (`process.ts:107-129`) | Per child. |
| Stagger | first response or 5 s | Shares the cached prefix. |
| Lean children (06) | `mcpTools:`, `skills:` | Implementer prefix ≈ 31K → ≤ 22K locally, ≤ 15K after the upstream schema flatten. |
| Model | profile `model:` | Use a cheaper model for `reviewer` in batches. |

### UX

- Status line: `batch b-7f3a 9/20 merged · 4 running · 1 conflict · 0.81M/3M tok`.
- 04's `/agents` overlay groups children under the batch id, labeled by item name.
- `/batch status` prints the journal table; verify logs are files the model reads with `localFetch`.

### Failure modes

| Failure | Handling |
|---|---|
| Not a git repo or no commits | Refused before planning (same check as `createWorktree`). |
| Main HEAD moves | Integration stays on the batch base; conflicts at land abort with no change. |
| Dirty tree at land | Git refuses; report it, keep the integration ref. |
| Verify missing (exit 127) or flaky | Stop after 2 consecutive 127s and ask the user to fix `verify`. One automatic re-run before `failed-verify`. |
| Two units edit one file | Plan-time warning; merge conflict → fixer once, then park. |
| Parent exits | Children run until idle or max timeout; journal plus prune recover state. |
| `refs/octocode/pi/<id>` taken | `saveRef` falls back to `-2…-100` (`worktree.ts:171-178`). |
| Child edits outside `paths` | Allowed; the summary diffstat flags it. |

## Files to change

| File | Change |
|---|---|
| `src/batch/engine.ts` (new) | Queue, pool, stagger, budget, state machine. |
| `src/batch/journal.ts` (new) | Atomic journal, dead-owner discovery. |
| `src/batch/mergeQueue.ts` (new) | Integration worktree, cherry-pick, bisect, fixer retry. |
| `src/batch/command.ts` (new) | `/batch` subcommands and completions. |
| `src/subagents/tool.ts` | `items`, `verify`, `review`; route to the engine; "after exit, before finish" hook in `runSubagent`. |
| `src/subagents/worktree.ts` | Split `finishWorktree` into `saveWorktree` + `removeWorktree`; add `cherryPickRange` and integration-worktree helpers. |
| `src/subagents/handoff.ts` | Batch summary through `ReportQueue`. |
| `src/team/panel.ts` | Batch grouping (with 04). |
| `src/shared/env.ts` | `OCTOCODE_BATCH_*`. |
| `src/index.ts` | Register the command; journal recovery on `session_start`. |
| `tests/architecture.test.ts` | `batch: ['shared', 'agentdb', 'team', 'subagents']`. |
| `docs/FEATURES.md`, `docs/CONFIGURATION.md`, `README.md` | Feature and env vars. |

## Phased plan

1. **Queue and journal.** `agent items[]`, own pool, stagger, journal, `/batch status|cancel`, summary with refs. Merging stays manual (`/agents merge`).
2. **Verify gate.** Split `finishWorktree`, per-worktree `verify`, budgets and per-item caps.
3. **Merge queue.** Integration ref, ordered cherry-picks, periodic verify with bisect, `/batch land`.
4. **Fixer and review.** `review: sample|all`, verdict parsing, one fixer retry.
5. **Resume.** Dead-owner detection, `/batch resume`, `/batch retry <item>`.

## Test plan

**Unit (vitest, temp git repos as in the worktree tests):**
- The queue never runs more than N at once; items past the cap queue, not refuse. Stagger delays starts 2..N until the first response or 5 s (fake clock).
- Journal round-trip; each transition writes atomically (temp file + rename), so a torn write is never read.
- Merge queue: 3 disjoint units → 3 picks + green verify. A conflicting pair → fixer once, then park. A unit that breaks integration → bisect finds it in at most log2(K)+1 verify runs.
- Budget: at 100% dispatch stops and queued items stay `queued`; the per-item cap stops the child.
- Recovery: kill the owner during `running` → prune saves the ref; resume verifies without re-running the child.
- Children get `OCTOCODE_PERMISSIONS_POLICY` no looser than the parent's; `context: 'fork'` with `items` is refused.

**End-to-end (scripted model, `tests/e2e-features.test.ts` style):** 6-item batch on a fixture repo with verify `node check.js`; one item scripted to conflict, one to fail verify. Assert final states, integration ref content, and summary under 8 KB.

**Real Pi flow (AGENTS.md):** `pi --no-extensions -e dist/index.js -e builtin:mcp -e builtin:tool-search`, then `/batch rename the helper fooBar to fooBaz across src/` on a scratch repo with 8+ call sites. Targets:
- 5–10 planned units; every unit `merged` or parked with a reason; `yarn test` green after `/batch land`.
- Peak children equals `OCTOCODE_BATCH_CONCURRENCY`; interactive `agent` calls still work.
- After the first child, `cacheRead` is ≥ 70% of each later child's first-request input (session JSONL usage).
- Kill Pi mid-batch; `/batch resume` completes without re-running merged or finished units.

## Open questions

1. Allow `profile: "plan"` (01/02) for a parallel planning pre-pass? Recommendation: single planning turn first; add the pre-pass only if plans are poor.
2. Scale `K=5` with suite length? Option: time the first verify and pick K so integration verify takes ≤ 20% of wall time.
3. `/batch land`: squash or one commit per unit? Default: per-unit commits (helps bisect and review).
4. Add a global process ceiling, e.g. `OCTOCODE_MAX_SUBAGENTS + BATCH_CONCURRENCY ≤ 12`, for small machines?

## Out of scope

- A workflows-style scripting runtime (`parallel()`, `pipeline()`). Pi's `codemode` already runs tool scripts; a later doc can expose the engine there.
- Non-git VCS, cloud or remote execution, cross-repo batches.
- Automatic push or PR creation. Landing stays local and user-triggered.
