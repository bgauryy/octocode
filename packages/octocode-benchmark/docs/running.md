# Octocode benchmark

Measures **how an Octocode MCP version researches code** against other tool setups. Every arm
answers the same questions with a fresh headless agent; a blind judge grades correctness; the
transcript measures observable tool exchanges and reported model tokens. The output is a matrix and a
dashboard: who is leaner, by how much, at what correctness.

Paths are relative to `packages/octocode-benchmark/`.

## The flow

```bash
CAMP=campaigns/<what>-$(date +%H%M%S)-$(date +%F)          # gitignored

# 0. Preflight: tools, auth, each MCP arm's tool count and schema size; then the overfitting
#    check (no question term in anything an agent sees; product hits listed for review).
scripts/preflight.sh mcp,gh-lean
python3 scripts/leak_check.py

# 1. Run: one isolated agent per (arm, pass, question). Re-running retries only invalid runs.
scripts/run_campaign.sh $CAMP mcp,gh-lean 1 all 4       # arms · passes · question selection · parallel jobs

# 2. Judge: blind X/Y per question, a different model family than the runners.
python3 scripts/question_catalog.py external/questions |
  xargs -P 4 -I{} scripts/judge_pair.sh $CAMP mcp gh-lean 1 {}

# 3. Summarize the campaign into results/ (local), then rebuild the matrix.
python3 scripts/campaign_summary.py $CAMP --octocode-label "Octocode v20 (local 20.1.1)"
python3 scripts/matrix_summary.py                       # → results/overview/MATRIX.md, results/overview/matrix_summary.json

# 4. Show it: static page in the repo, and the shareable dashboard (`docs/dashboard.md`).
python3 dashboard/build_datasets.py --static results/overview/index.html
```

Then write `results/<campaign>/report.md` as [output.md](outputs.md) describes.

## Arms

| Arm | The agent gets | Brief |
|---|---|---|
| `mcp` | the Octocode MCP server only, clasify off | `prompts/mcp.md` |
| `mcp-clasify` | the same, clasify offered (needs a provider key) | `prompts/mcp.md` |
| `local-mcp` | five local MCP read tools, semantic features off | `prompts/mcp.md` |
| `local-mcp-clasify` | the same, with optional goals and clasify | `prompts/mcp.md` |
| `gh-lean` | read-only `gh` + `--json/--jq/--limit` + `grep/sed -n/head/jq` filters | `prompts/gh-lean.md` |
| `gh` | read-only `gh`, no filters | `prompts/gh.md` |
| `rtk` | `rtk gh …` | `prompts/rtk.md` |
| `headroom` | `ghc …` (gh → Headroom; needs `HR_PY`) | `prompts/headroom.md` |

**Rate an MCP version:** `OCTO_MCP_CMD="npx -y octocode-mcp@20.1.1" run_campaign.sh …` runs a
published version; unset, it runs the local build (`packages/octocode-mcp/dist`) against a
snapshot of the MCP launcher and native binary taken when the campaign starts. Give each version its own
`--octocode-label` and the matrix shows them side by side. **Add an arm:** write
`prompts/<arm>.md` and its tool flags in `run_arm.sh`.

GitHub runners share `prompts/runner.md`; local runners share `prompts/local-runner.md`.
Compare arms on the same `RUNNER_MODEL`, within the same campaign and question set.
The default runner is Claude; `RUNNER=pi` and `RUNNER=codex` select other installed hosts.
For Terra use `RUNNER=codex RUNNER_MODEL=gpt-5.6-terra`; Codex supports MCP arms only.

Start with the [local cookbook](../local/README.md) or [external cookbook](../external/README.md).
For a local comparison, select a catalog and its repository manifest with absolute
`QUESTION_DIR` and `QUESTION_REPO_MAP` paths. The language research set lives in
`local/questions`; its repository assignments live in `local/repositories.json`.
The runner discovers available question IDs; use `all`, a numeric range or comma-separated IDs. Set `ANSWER_DIR` only
for the judge's private rubrics. Codex copies the question's selected tracked repositories
into a fresh lab and denies evaluator paths through the actual MCP policy. Its local arms
offer only the five MCP read tools, plus clasify when available. Check CLI-only tools in a
separate applicability lane; do not claim they were available to the solver.

Use `scripts/with_credentials.mjs -- <command> ...` to load canonical credentials into
the child environment. Enable `BENCH_CLASSIFICATION_TRACE=1` for JEV ablations: the proxy
records every provider attempt, including setup and errors, without saving bodies or keys.
Freeze the product, questions and harness before scored runs. Record budgets and run order;
see [method](method.md) for Codex limits, isolation and accounting.

## Rules that keep it honest

- **Isolation.** One fresh agent process per run: no inherited settings, project instructions, skills, memory, or
  other tools. Never let a runner see another arm, a verdict, or an answer key.
- **Measured, never reported.** Characters come from the runner's own transcript
  (`transcript_to_log.py`): tool calls written + tool results read + the final answer.
  Failed and refused calls count. `sumlog.py --strict <log>` re-hashes every artifact.
- **Retain invalid runs.** Crashes, budget stops and missing MCP execution are invalid.
  Preserve every attempt and its cost; retry only within the campaign's declared budget.
  Never rerun an unfavorable valid answer.
- **Validate returned evidence and answers separately.** Verify raw artifact hashes, source-line
  fidelity, citation support, relevance, coverage and unread continuations per question. Existing
  paths alone do not prove correctness. The local cookbook includes saved-run audits.
- **Correctness first, as whole points.** The judge moves about a point between runs, so
  report correctness rounded to whole points and treat arms within one point as tied. Only
  then do characters decide.
- **Ratios are per question.** Headline the geometric mean of per-question ratios with a 95%
  bootstrap CI and the leaner count; never a pooled total alone.
- **Same campaign beats cross-campaign.** Arms from different campaigns ran with different
  agents or days: show them, label them, but draw conclusions from same-campaign pairs.
- **No tuning to the questions.** `leak_check.py` must find no question term in the briefs or
  the MCP's instructions and tool list. A product change made because a benchmark question
  failed turns that question into development data: note it, and confirm on new questions.
- **Missing is unknown.** Hosts expose different token and timing fields. Do not turn missing
  API time, usage or provider cost into zero. Codex observable characters exclude internal
  framing; reported input tokens already include cached input. Compare time within a campaign.
- **Keep corpora separate.** Pass `--question-set` to summaries and matrices. Never pool
  equally numbered questions from local and GitHub catalogs.
- **Concurrency ≤4.** More parallel runners exhaust GitHub code search and every arm pays.

Details and the reasoning behind each rule: `docs/method.md`. The judge's rubric:
`docs/judging.md` with `docs/example-verdict.md` (both are sent to the judge).
Calibrate against checked supported and contradicted source claims; record uncertainty and
judge independence. Keep cost hidden until quality is sealed. Exclude the initial prompt from
normalized exchange accounting; provider usage that cannot subtract it stays separately labeled.
Workers and judges can organize prose naturally: no mandatory headings, word counts or
fixed tool sequences. Preserve only fields consumed by the harness.

## Outputs

- `results/<campaign>/run.json` — every run and judgment of a campaign (local, gitignored).
- `results/overview/MATRIX.md`, `results/overview/matrix_summary.json` — every arm against the anchor and each other.
- `results/overview/index.html` — the dashboard as one static file.
- The shareable dashboard artifact — `docs/dashboard.md`.
- `results/<campaign>/report.md` — your write-up; add a row to `results/README.md`.

## Related guidance

- `octocode-eval-benchmark`: designing an evaluation from scratch, judge calibration, held-out
  sets. This package supplies the code-research harness and catalogs built on those rules.
- `octocode-research`: reading transcripts and verifying a surprising answer by hand.

## Questions

`external/questions/Q<n>.md`: a title, an `id`, and a self-contained `## Question`
naming repos and refs. No hints, scope or answers. Add its row to that folder's README.
Local catalogs use pinned `repositories.json` manifests; repository scope may be assigned
outside the question in `questionRepositories`, or through existing `{{REPO:id}}` tokens. Keep source
excerpts, expected findings and coverage labels out of solver-visible files.
