Load when planning an authorized package benchmark run.

# Run a benchmark (orchestrated checklist)

Follow the [matchup procedure](../SKILL.md#run-a-matchup). Read [preflight and instrumentation](run-preflight.md) for setup and [runner, judge and aggregation phases](run-phases.md) for execution. Methodology: [design](BENCHMARK.md).

## Checklist

Complete the entry’s preflight → answer → judge → summarize gates, then apply the fairness rules below. Preserve the complete campaign per [output.md](../output.md); update published results only when requested.

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
