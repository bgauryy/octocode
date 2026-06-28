---
name: octocode-loop
description: Use when a clear research or code goal needs grounded Act→Observe→Learn→Repeat loops over Octocode (MCP/CLI) until evidence converges — research, local code-check, or multi-source loops.
---

# Octocode Loop

Run **grounded research loops** — Act → Observe → Learn → Repeat — where every step is proven by a real Octocode tool result, never guessed. Use this when the goal and the research path are already clear and the work is iterative: issue a call, read the result, adjust, repeat until evidence converges or a budget is hit. This file is the router; mechanics live in `references/`.

Octocode transport reference: read `references/octocode.md` when choosing, installing, or explaining Octocode MCP vs CLI usage.

Core principle: an agent steered by concrete tool feedback beats one running open-loop. So every iteration MUST end in a **grounded observation** — an actual result with a `status` — and the next action is chosen from that observation, not from assumption. Treat snippets as leads; conclusions are earned by proof.

## Transport: MCP or CLI

Use the Octocode **MCP server** tools when the host has them (`oqlSearch` / `ghSearchCode` / `ghViewRepoStructure` / `ghGetFileContent` / `ghSearchRepos` / `npmSearch` / `ghHistoryResearch`, plus local + LSP tools when present). Otherwise use the **CLI** (`npx octocode <cmd>`). If MCP configuration is needed, read `references/octocode.md`. Same capabilities — pick by what the user has. Load `references/tools.md` when you need the exact MCP↔CLI mapping, `status` semantics, schema-first calling, or how to carry anchors across iterations.

## The loop (all modes)

1. **Frame** — one line: goal + the single question this iteration closes + what observation would end the loop.
2. **Act** — issue ONE cheap call: the smallest query that could move the answer. Start broad/concise, then narrow.
3. **Observe** — read `status` first. `empty` = ran but matched nothing → check scope, spelling, branch, extraction mode, then broaden or switch path before concluding absence. `error` = correct the call (auth, validation, rate limit, scope) and retry. Results → capture exact anchors (path:line, match range, repo/PR/package id, branch).
4. **Learn** — update a short hypothesis map (likely answer, alternate, what would disconfirm each). Pick the next cheapest step that could confirm or kill a branch.
5. **Repeat** until a stop test passes.

Keep ≥2 plausible explanations alive while any is unsettled. Never let a single unverified snippet collapse the map.

## Pick the mode

- **General research loop** — when goal and path are clear on a single surface, read `references/research-loop.md`.
- **Local code checks & findings loop** — when looping over workspace code where search / AST / LSP / tests are the ground-truth "compiler", read `references/code-check-loop.md`.
- **Full multi-source research loop** — when the question spans local + GitHub + npm + history + web as chained sub-loops, read `references/full-research-loop.md`.
- **Loop mechanics, principles, stopping, best-of-K, budgets** — read `references/loop-protocol.md` before running any mode.

## Stop tests (don't over- or under-run)

Set multiple exits — don't trust one. Stop when ANY fires: the framed question is answered with grounded evidence and the alternate is killed; the cheapest next step can no longer change the conclusion; the iteration or token budget is hit; or no-progress is detected (the last N steps changed no state). Otherwise keep going — agents tend to stop early after one good hit. Before stopping, run one reflection pass: weakest claim, strongest counter-evidence, whether one more cheap call would flip it. If a loop stalls (repeats, same `empty`/`error`), change the surface or query shape rather than retrying verbatim — see `references/loop-protocol.md`.

## Output contract

End with a compact result, not a transcript: **Answer**, **Evidence** (anchors like file:line/repo/package/commit), **Loop trace** (only decisive iterations), **Verification** (the deterministic check actually run, or say none), and **Open gaps** (what remains unproven). If the trace cannot fit comfortably, summarize the ledger and keep exact anchors.

## Gates

- Read the relevant mode reference before running that loop; read `references/tools.md` before the first raw-tool/schema call.
- Verify before concluding: prove a finding with a deterministic check first (test, build, AST/structural match, LSP, exact read, history) — an LLM judge only for the unquantifiable, never your own say-so alone. Do not record a conclusion from a lead.
- Keep the loop on a leash: gate expensive or irreversible actions (clone, test run, any write) on a surviving lead, and checkpoint with the human before the irreversible ones.
- Surface a brief loop trace (iterations, key observations, final evidence) so the path is auditable.
