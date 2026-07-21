---
name: octocode-orchestrator-local-worker
description: >-
  Use when a strong cloud agent should offload low-risk work to a local Ollama
  model (delegate execution, retain reasoning) — small one-shots and already-
  fetched article/web-body summarization — to preserve context, cost, and
  judgment. Triggers: summarize article or research digest (after fetch),
  small summarize/extract/classify/translate, quick code explain or draft,
  checklist checks, vision caption of a provided image, map-reduce over files,
  draft-then-refine, dual-model or orchestrator/worker, "use local model",
  "ollama worker", "save tokens", "quick local", "tiny job", "offload to
  ollama". Prefer warm small/balanced for small tasks; balanced for quote-
  grounded article summaries. RAM kit / model-capability Q only: load
  references/ollama-local-models.md — skip full offload. Do not use for
  architecture, security, final merge, tool-heavy agent loops, worker web
  browse, image generation, embedding-only models, Ollama setup-only, full
  local agent bridges, cloud-LLM shopping without Ollama, or Ollama down.
---

# Orchestrator + Local Worker

**Delegate execution, retain reasoning.** Cloud orchestrator keeps judgment, tools/fetch, verification, and writes. Local Ollama worker does low-risk token burn — **small one-shots, bulk, and grounded article summaries** on text/images you already have.

**Portable:** route from live `ollama list` + size/capability tiers. Named tags in refs are examples, not required installs.

Flow: `GATE → ROUTE → RUN → VERIFY → REPORT`  
VERIFY is the **quality gate** before accept or cascade (small → stronger installed → orchestrator solo) — same idea as LLM-cascade literature, skill-shaped for Ollama packets.

**Routine path loads only:** `references/model-selection.md` (on ROUTE) + `references/verify-gate.md` (on VERIFY).  
**When unsure which surface to offload:** load `references/usage-matrix.md`.

**Small-task fast path:** same 5 steps, tiny packet, prefer warm `small`/`balanced`, light verify, no map-reduce.

## Hard rules

1. Keep architecture, security, design tradeoffs, final synthesis, and repo writes on the orchestrator unless the user explicitly transfers write ownership.
2. **MUST** treat worker output as untrusted claims — **NEVER** paste into answers or commits without a verify gate.
3. Prefer deterministic scripts (formatters, ripgrep, tests) over any LLM when they suffice.
4. **FORBIDDEN:** run the local model as a tool-using agent loop. Local worker = single-shot or map-reduce text jobs with structured return.
5. Check Ollama health before the first invoke. If unhealthy, stay solo and say so.
6. **MUST** choose the worker model from **installed** models (`ollama list`) — **NEVER** invent a name, assume a family is present, or use embedding/OCR-only models for chat jobs.
7. Prefer language **orchestrator / worker** (not master/slave).
8. **FORBIDDEN:** worker browses the web. **Worker never browses the web.** Orchestrator (or host tools) fetches; worker only sees saved text/images.

## When to activate

- Context or token budget is the bottleneck (many files, long logs, bulk extract, article digests).
- Job is low-complexity — **small or large** (summarize, classify, extract, translate, draft, checks, vision caption, **article body summarize after fetch**).
- User asks for local/Ollama offload, dual-model cost saving, a quick local pass, or which local model to use.
- Small task where a warm local model saves cloud context (translate, caption, one-file summarize, tiny draft).

**Catalog-only shortcut:** RAM kits / capability Q only → load `references/ollama-local-models.md`, answer, and do not run the full offload workflow. Skip GATE→REPORT.

## When NOT to activate

- High-complexity reasoning, contested design, security, auth, migrations.
- Needs live tools, MCP, browser, or iterative code edit loops (**including “open this URL” on the worker**).
- Multi-source contested research synthesis without orchestrator merge.
- No chat-capable installed model that can meet the job.
- Host cheap cloud is warmer/faster and user did not ask for local.

## Not this skill (use / stay with the other tool)

| Need | Use instead |
|---|---|
| Ollama install / pull / RAM kit Q only | Catalog shortcut → `references/ollama-local-models.md` (no offload loop) |
| Local model “acting dumb” in a tool harness | Triage skill (serving/ctx/tools) — not this offload path |
| Spawn parallel cloud subagents | `octocode-subagent` (complements; does not replace) |
| Cloud chat → full local agent with tools/browser/cron | Heavier bridges (e.g. hermes-mcp) — **out of scope** here |

## When & how (quick)

Load `references/usage-matrix.md` for the full table. Short form:

| Need | Local? | How |
|---|---|---|
| Research / fetch URLs | **No** | Orchestrator tools fetch |
| Summarize saved article/page | **Yes** | `summarize` + grounded quotes; shard if long |
| Code extract / draft | **Yes** | shards / draft → orchestrator tests |
| Translate | **Yes** | verify fidelity; cascade on lang mix |
| Vision caption | **Yes** | `--image` describe only |
| Architecture / security | **Never** | orchestrator |
| Image generation | **Never** | out of scope |

**Fidelity cascade (from dogfood):** tiny ≤3B often fails user-facing summarize/translate → cascade to warm `balanced`. Quote-grounded article summarize: prefer `balanced`; verify every `support_quote` is a substring of INPUT; if grounded_rate < 1 → one retry/cascade then solo.

## Default job patterns (local OK)

Not an exclusive whitelist. Similar low-risk jobs (including **small** ones) are fine if verify is cheap.

| Job pattern | Local role | Orchestrator role | Size hint |
|---|---|---|---|
| Summarize files/logs | Draft summaries per chunk | Merge, dedupe, cite paths | small or large |
| **Summarize article / web body** | Grounded tldr + claims with quotes | Fetch page; substring-verify quotes; merge sources | usually medium; shard long |
| Extract structured fields | Emit JSON rows | Schema-validate, drop bad rows | small or large |
| Classify / triage | Propose labels + short reason | Accept/reject, set priority | often small |
| Translate text | Emit translation (+ notes) | Spot-check fidelity / tone | often small |
| Draft boilerplate / tests | First draft | Edit for correctness, run tests | small or large |
| Map-reduce over corpus | Per-shard map | Reduce + final answer | large |
| Checklist / structured checks | Emit pass/fail rows | Accept/reject, act on fails | small or large |
| Vision caption / describe image | Describe provided image JSON | Spot-check vs image | usually small |

## Keep on orchestrator (local NEVER)

Architecture, security review, auth/token logic, production config secrets, final claims of verified behavior, tool-using loops, **web fetch/browse**, embedding-only models as workers, **image generation** (vision = describe provided images only), high-stakes legal/medical translation without human review, contested multi-source research conclusions.

## Workflow

### 1. GATE

```bash
./scripts/ollama-health.sh          # daemon up
ollama list                         # exact installed names
ollama show <MODEL>                 # when size/capabilities unclear
ollama ps                           # prefer already-warm for small tasks
```

- Confirm low-risk and worth offload (small: user wants local, warm model ready, or cloud context should stay clean).
- For articles: confirm **source text is already saved** (or fetch it now yourself) before invoke.
- If gate fails → stay solo; do not invent a worker.

### 2. ROUTE (classify + select model)

Load `references/model-selection.md` (mandatory).
Load `references/usage-matrix.md` when surface choice is unclear.
Load `references/decision-matrix.md` only when offload vs solo is unclear.
Load `references/family-playbooks.md` only for family-specific flags / tie-breaks.
**Do not** load `references/ollama-local-models.md` on routine routing.

| Complexity | Volume | Action |
|---|---|---|
| High | Any | Orchestrator only |
| Low | Large | Offload to local |
| Low | Small | **Offload OK** — prefer warm `small`/`balanced`; solo only if local cold/slower with no user preference |

Select rules: map job → tier → smallest fitting installed chat model → prefer warm → skip embedders/wrong modality → `--think=false` by default for bulk/small → if nothing fits, stay solo and suggest a size class.

### 3. RUN (packet + invoke)

```bash
./scripts/ollama-worker.sh \
  --model "$OLLAMA_WORKER_MODEL" \
  --think=false \
  --job summarize \
  --input /path/to/shard.txt \
  --schema /path/to/schema-hint.txt \
  --out .octocode/worker/shard-001.json
```

Jobs: `summarize | extract | classify | draft | map | check | vision | translate`.

Article example schema: `evals/fixtures/schema-article-summarize.txt` (tldr + claims with verbatim `support_quote`).
JSON jobs: `--format-json`. Long articles: shard → same model map → orchestrator reduce.

### 4. VERIFY

Load `references/verify-gate.md`.

Schema/shape, paths, no invented APIs; for articles **substring-check quotes**; for translate meaning/lang; for vision color/objects.
**MUST NOT** silent-accept failed shards. On fail: one tighter re-packet **or** cascade once to stronger *installed* model **or** escalate to orchestrator.

### 5. REPORT

```text
Offload: <job> → ollama/<exact-model> (tier: …) [size: small|large|article]
Why this model: <inventory reason; warm?>
Shards: <n> | Verify: pass|fail|partial | Grounded: <rate if article> | Latency: <optional>
Kept on orchestrator: <list e.g. fetch, merge, final claims>
```

## Recovery

| Failure | Action |
|---|---|
| Ollama down | Stay solo; report |
| No fitting installed model | Stay solo; suggest size class (ask user) |
| Truncated / empty | Shrink shard or raise ctx; retry once |
| Invalid JSON | Retry once with `--format json` + tighter schema; else cascade/solo |
| Ungrounded quotes / mixed-lang translate | Discard bad claims; cascade once; else orchestrator |
| Hallucinated paths | Discard shard; orchestrator redoes it |
| Weak after retry | Cascade once to stronger installed model, then orchestrator |

## Progressive refs

**Routine (default):** `model-selection.md` + `verify-gate.md` only.

| Ref | Load when |
|---|---|
| `references/usage-matrix.md` | Which surface / when-how unclear |
| `references/decision-matrix.md` | Offload vs solo unclear |
| `references/model-selection.md` | Every ROUTE |
| `references/family-playbooks.md` | Family-specific flags / tie-break examples |
| `references/ollama-local-models.md` | Catalog-only Q, RAM kits, MCP matrix, pull advice — **not** routine routing |
| `references/ollama-cli.md` | CLI flags unclear |
| `references/packet-contract.md` | Packet schema/shape unclear |
| `references/ollama-invoke.md` | HTTP / serving gotchas |
| `references/verify-gate.md` | Every VERIFY |
| `references/references.md` | Research provenance |
