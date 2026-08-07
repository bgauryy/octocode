# Run a benchmark (orchestrated checklist)

One **orchestrator** drives a **pairwise matchup** — Octocode (anchor) vs one baseline CLI —
over the questions, repeated per baseline and ≥ 3 passes. Design: [BENCHMARK.md](BENCHMARK.md).
Concrete recipe: [run-with-agents.md](run-with-agents.md) →
[run-preflight.md](run-preflight.md) + [run-phases.md](run-phases.md).

## Checklist

1. **Phase 0 — preflight.** Run `bash skills/octocode-benchmark/scripts/check-prereqs.sh
   [octocode-version]` (verifies every arm + question set + primers; non-zero exit = fix
   before proceeding). Then set up the run dir + measurement wrappers. Detail:
   [run-preflight.md](run-preflight.md).
2. **Phase 1 — answer.** Per pass, spawn the anchor + baseline as two isolated agents (no
   shared transcript, no answer key). Each appends its `## Q<n>` section to
   `answers/<arm>-p<pass>.md`, chars via its `bin/` wrapper + `bin/record_answer.py`. Detail:
   [run-phases.md](run-phases.md).
3. **Phase 2 — judge.** After both sections exist, build the blind packet
   (`bin/build_blind_packet.py`, X/Y randomized per question) and run one blind judge per
   question per [JUDGING.md](JUDGING.md) — reason first, score, rank correctness-first,
   confirm decisive wins, mark unresolved.
4. **Phase 3 — summarize.** Validate logs (`bin/validate_campaign.py`), recompute paired
   stats per [aggregation-and-stats.md](aggregation-and-stats.md), write the report
   ([REPORT_TEMPLATE.md](REPORT_TEMPLATE.md)), exclude + count unresolved questions, and
   update `results/SUMMARY.md` + `results/README.md`.

## Rules that keep it fair

- Same question, same frozen refs, same budget for both arms in the matchup; only the CLI differs.
- **Leanest legitimate path for every arm** — a whole-file/tree dump where a region or search
  answers is a fairness violation; flag any known suboptimality in the write-up.
- Questions carry **no answer key**; the judge establishes ground truth itself.
- Grade **semantic support**, not wording/length/citation/tool order.
- Fixed primers are setup context, excluded from char totals; any help/schema/failed call during research is counted.

## Add a question

GitHub questions → shared [`compare/github-questions/`](../../../compare/github-questions/)
(all matchups at once); corpus-local → that matchup's `questions/`. Create `Q<n>.md` with
**only** a title, an `id`, and a `## Question`; add its row to that set's `README.md`.
