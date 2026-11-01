Load when executing benchmark phases after preflight.

# Phases 1–3 + outputs

Load when spawning runners, judges, and writing the report. Preflight + measurement first:
[`run-preflight.md`](run-preflight.md).

## Phase 1 — 2 runners per question, per pass

Spawn the **anchor** (`octocode`) and this matchup's **baseline** as separate agents. Give
each only `bash` + **its own inputs**: `RUNNER.md`, its arm's `RUNNER_TOOL_CONTEXT.md`
section, and the question — never another arm's primer, transcript, or an answer key.

Each: freeze mutable refs first; take the **leanest legal path** (no whole-tree/whole-file
dumps where a targeted read/search answers); run **every** command through its `compare/bin/` wrapper
(fallback `scripts/measure.sh`); log the final answer with `compare/bin/record_answer.py`; then
append its **`## Q<n>` section** (Answer + Research steps) to `answers/<arm>-p<pass>.md` —
headings exactly `Q1..QN` in order. Chars are authoritative from the JSONL. A missing
`## Q<n>` section = that arm unscored on that question.

Command forms: `octocode`→`compare/bin/octoc <tool> '<json>'` · `rtk`→`rtk
gh <args>` · `headroom`→`compare/bin/ghc <gh args>` · `gh`→`compare/bin/ghm <gh args>` (bare baseline).

**Scaling:** batch dispatch within an arm, keeping a fresh runner context for every (question, arm, pass). For example, schedule Q1–15 and Q16–30 as separate queues; do not share a runner transcript across those questions. If the host cannot isolate them, report that limitation instead of claiming the isolation gate passed.

## Phase 2 — blind packet, then 1 judge per question

**Barrier:** don't build the packet or judge until both arms' `## Q<n>` sections exist.
Build with `compare/bin/build_blind_packet.py` (redacts tool identity, relabels the two answers
**X / Y** randomized per question, records the map). Then judge per [`JUDGING.md`](JUDGING.md):
ground truth first, reason per answer, score, rank X/Y correctness-first, confirm decisive
wins. One reasoning-first verdict per question.

## Phase 3 — outputs + aggregate

| Artifact | Location | Use |
|---|---|---|
| Per-call measurements | `<campaign>/<arm>-p<pass>-Q<n>.jsonl` | authoritative chars/calls (`sumlog.py`) |
| Headroom diagnostics | `<campaign>/<arm>-p<pass>-Q<n>-diagnostics.log` | classify a 0% ratio |
| Runner answers | `<campaign>/answers/<arm>-p<pass>.md` | `## Q<n>` sections the packet reads |
| Blind packet | `<campaign>/blind-packet.md` | X/Y, tool-redacted; the judge's only input |
| Judge verdicts | `<campaign>/judge/Q<n>.md` | reasoning-first scores + ranking |
| Report / rollup | `<result>/report.md` (published rollup only when requested) | per [`REPORT_TEMPLATE.md`](REPORT_TEMPLATE.md) |

Recompute paired stats from the JSONL via `compare/bin/validate_campaign.py` per
[`aggregation-and-stats.md`](aggregation-and-stats.md): geo-mean + median char ratio, leaner
win-rate + sign test, outlier disclosures. Confirm the report matches the logs row-for-row;
exclude **unresolved** questions and count them. One pass is a snapshot — repeat ≥3.
