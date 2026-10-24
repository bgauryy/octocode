# Harness components

Start with the [local cookbook](../local/README.md) or [external cookbook](../external/README.md)
for commands, worker and judge instructions, output locations and measurement limits.
The language research questions live in [local/questions](../local/questions); repository
assignment stays outside the question text.

- `prompts/` — `runner.md` (every GitHub runner's brief), `local-runner.md` (the local runners'
  brief) and one brief per arm: `mcp.md` (also used by `local-mcp*`), `gh-lean.md`, `gh.md`,
  `rtk.md`, `headroom.md`, `local-shell.md`. The brief is the arm's only primer.
- `scripts/` — the harness. `run_campaign.sh` runs arms over questions in parallel, `run_arm.sh`
  runs one isolated agent, `transcript_to_log.py` measures it, `judge_pair.sh` judges two
  arms blind, `campaign_summary.py` and `matrix_summary.py` aggregate, `preflight.sh` checks
  setup, `guard/gh` keeps every gh call read-only, `guard-local/` (`git`, `find`) keeps the
  `local-shell` arm read-only, `ghc` + `hr_compress.py` are the Headroom arm's compressor.
- `external/questions/` — the shared GitHub questions. A question is a title, an `id` and a
  self-contained `## Question` naming repos and refs; no hints, scope or answers.
- `local/datasets/core-v1/` — 30 questions against the pinned clones in
  `packages/octocode-benchmark/local/repos/`, run by arms named `local-…`; `answers/` holds the ground
  truth the judge reads (runners never see it).
- `local/datasets/goals-v2/` — 40 source-backed questions across nine pinned clones, including
  eight Terraform questions. `catalog.json` selects repositories per question;
  `repositories.json` records revisions and tracked-file hashes. Rubrics and tool coverage
  checks live outside the solver corpus. CLI-only topology and rewrite checks are a
  separate applicability lane.

`RUNNER=codex RUNNER_MODEL=gpt-5.6-terra` selects fresh Codex execution for MCP arms.
`QUESTION_DIR` selects the question catalog, `QUESTION_REPO_MAP` its repository manifest,
and `ANSWER_DIR` the private judge rubrics. `render_question.py` substitutes repository
tokens; `codex_runner.py` materializes only the selected tracked files into each local lab.
`mcp_probe.py` checks the live `classification` schema gates and denies evaluator paths
before research. Its local and classifier-source probes use valid current input shapes so
a malformed request cannot stand in for a scope denial; disabled source tools must report
the explicit disabled-tool reason.

Use `with_credentials.mjs -- <command> ...` to resolve canonical Octocode credentials into
the child environment without saving them. `BENCH_CLASSIFICATION_TRACE=1` records JEV HTTP
attempt metadata through `classification_proxy.mjs`; failed or unreported usage stays unknown.
Run summaries preserve invalid attempts, requested classification, selection coverage and
provider usage. Opaque cursor replay does not reveal whether classification ran. Historical
goal/ranking records retain their original interpretation. Pass
`--question-set` to the summary and matrix commands to keep corpora separate.

The flow and its rules: [`../docs/running.md`](running.md).
