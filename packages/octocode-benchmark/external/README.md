# External GitHub benchmark cookbook

Compare how tools answer the same GitHub research question against a fixed revision. The
[existing question catalog](../external/questions/) is the single owner of these
questions. Keep external results separate from local results even when question IDs match.

Start with the [evaluation method](../README.md#choose-the-benchmark-and-evaluation-method):
use `octocode-agentic` evaluation to freeze metrics, quality guardrails, case exposure, judge
calibration and budgets before running the external harness below.

## Run workers

Run these commands from `packages/octocode-benchmark`. Authenticate the selected runner and
GitHub before starting. The [benchmark guide](../docs/running.md) describes
build snapshots and isolation. `preflight.sh` checks the runner, GitHub authentication, Pi
judge availability and selected tool setup; it does not launch the campaign.

```bash
bash scripts/preflight.sh mcp,gh-lean
bash scripts/run_campaign.sh campaigns/github-example mcp,gh-lean 1 1-3 2
```

This example runs questions Q1–Q3 once per arm, with two concurrent workers. The campaign
runner defaults to Claude. `RUNNER=pi` selects Pi. `RUNNER=codex` supports MCP arms only;
set `RUNNER_MODEL` explicitly when comparing models. Use a new campaign directory for a
changed model, build, prompt or question set. The remaining arms are `gh`, `rtk` and
`headroom`; their dependencies and primers are listed in [compare](../docs/harness.md).

Workers receive the [shared GitHub brief](../prompts/runner.md), the selected
[arm brief](../prompts/), and one question. They must freeze mutable refs, answer each
material part, cite source, explain their conclusions and admit uncertainty. Their prose
has no fixed template. Solvers must not see private rubrics, other answers or judges.

## Check quality and compare

Read the [judge instructions](../docs/judging.md) before
judging. Establish source truth independently, then compare shuffled X/Y answers without
arm identity or cost information. Check factual accuracy, coverage, causal reasoning,
source support and uncertainty for each question. Citation existence alone does not prove
support. Keep unavailable evidence distinct from a proven error. Review known positive and
negative examples before trusting a judge, and audit material disagreements against source.

```bash
bash scripts/judge_pair.sh campaigns/github-example mcp gh-lean 1 Q1
python3 scripts/campaign_summary.py campaigns/github-example --question-set github-example
```

Run `judge_pair.sh` for every completed pair. Its default judge is Pi; use
`JUDGE_RUNNER=codex JUDGE_MODEL=<available-model>` for the isolated Codex judge. Prefer a
judge model different from the workers and record its identity. The judge may write natural
reasoning in any structure; the final JSON score fields are required by the summary parser.

## Outputs

- `campaigns/<name>/runs/<arm>-p<pass>-<Qn>/`: original answer, transcript, result and status;
  retain setup and isolation receipts when the runner emits them.
- `campaigns/<name>/<arm>-p<pass>-<Qn>.jsonl`: measured tool and answer exchanges.
- `campaigns/<name>/attempts/`: previous attempts; do not discard failures when retrying.
- `campaigns/<name>/judge/<armA>~<armB>/`: blind packets, verdicts, judge receipts and private
  `MAP.secret.txt`. Keep the map away from judges until grading is sealed.
- `results/<campaign-name>/run.json`: generated summary; `results/` holds published reports
  and dashboards. Raw campaign directories are ignored by Git.

Report quality before efficiency. Tool characters, normalized tokens, provider usage, tool
rounds, inference rounds and elapsed time are different quantities. Include prompt tokens
only in a separately labelled provider total; exclude the initial prompt from the requested
research-exchange comparison. Preserve unknowns and explain setup, rate limits and retries.
