# Usage matrix — when & how

Load when choosing whether/how to offload a surface (research, articles, code, translate, images, …).

Portable: pick from **installed** models via live `ollama list` + tiers in [model-selection.md](./model-selection.md). Named tags below are **examples from dogfood**, not requirements.

## Split of labor (always)

| Role | Owns |
|---|---|
| **Orchestrator** | Tools, web/MCP fetch, architecture, security, final synthesis, repo writes, verify |
| **Local worker** | Single-shot / map-reduce on **already-provided** text or images — no browse, no tools |

## Surface guide

| Surface | Use local? | How | Prefer tier | Orchestrator keeps |
|---|---|---|---|---|
| **Research / web browse** | **No** for fetch | Orchestrator (or host tools) retrieves pages | — | Discovery, ranking, citation policy |
| **Article / internet summarize** | **Yes** after fetch | Save body → `summarize` with grounded `support_quote` schema; shard if long (map-reduce) | `balanced` (fidelity); warm `small` only for rough skim | Fetch, quote substring check, multi-source merge |
| **Code summarize / extract** | **Yes** | Per-file shards → merge | `balanced` | Correctness, tests, security |
| **Code draft / tests** | **Yes** (first draft) | Tight schema; orchestrator edits + runs tests | `balanced` (+ coder signals) | Final code, test green |
| **Classify / triage** | **Yes** (often small) | Closed label set | `small` if warm | Priority decisions |
| **Translate** | **Yes** (often small) | Schema + fidelity spot-check; cascade on mix/lang fail | `balanced` for user-facing; `small` only if verified | Publish tone, high-stakes langs |
| **Checklist / structured checks** | **Yes** | Pass/fail rows | `balanced` | Acting on fails |
| **Vision caption** | **Yes** | `--job vision --image` (describe only) | `special` (vision) | Spot-check vs pixels |
| **Image generation** | **Never** | — | — | Out of scope |
| **Architecture / security / auth** | **Never** | — | — | Always orchestrator |

## Article / internet summarization (dogfood lesson)

**Good:** privacy, cost, cloud-context savings on short/medium already-fetched articles (~2–8k chars per shard); structured JSON with verbatim quotes.

**Bad / escalate:** worker browsing the web; long unsharded pages; multi-article contested synthesis; citation-exact claims without a substring verify gate.

**Packet pattern**

1. Orchestrator fetches → writes `SOURCE_URL` + plain text file.
2. Worker `--job summarize` + schema requiring `tldr`, `key_points`, `claims[].support_quote`.
3. Verify: every `support_quote` is a contiguous substring of the input (normalize whitespace); drop ungrounded claims; cascade once if grounded_rate < 1.0.
4. Long pages: chunk → map summarize → orchestrator reduce (same pattern as map-reduce corpus).

**Fidelity vs latency (measured on this skill’s kit — illustrative):** warmer ~7B often faster; ~12B multimodal/instruct often better quote grounding. Always verify; never skip cascade after partial grounding.

**Why verify before cascade:** cheap/local draft first, accept only if quality gate passes, else stronger model or orchestrator — same cascade idea as FrugalGPT / cascadeflow, implemented as substring + schema checks (not a trained scorer).

## Small tasks

Same surfaces, smaller packets. Prefer **warm** installed models. User-facing translate/article skim still needs verify — tiny ≤3B models often fail fidelity (see loop-report).

## Anti-patterns

- Asking Ollama to “open this URL”
- Summarizing without saving source text the orchestrator can re-check
- Using embedding models as chat summarizers
- Thinking **on** for bulk article shards (`--think=false` by default)
- Silent-accepting failed JSON / ungrounded quotes without cascade or solo redo
- Omitting `--keepalive` on map-reduce (cold reload each shard)
- Oversized shards vs `num_ctx` (silent truncation — no Ollama error)
- High temperature on extract/classify (prefer `0.1–0.3` via `--temperature`)
- Confusing this skill with Ollama **setup** skills or full local **agent** bridges (tools/browser)
