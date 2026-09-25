# Benchmarks and results

Keep reusable benchmark definitions under `benchmarks/<name>/`. Use [document-answering](document-answering/README.md) for the public starter and [skill-smoke](skill-smoke/README.md) for this skill's maintenance fixtures.

For an actual campaign, copy or author the definition in the controller's workspace. Store its run evidence under `<workspace>/.octocode/benchmarks/<name>/results/<run-id>/`; use the corresponding home directory when no workspace applies. Never write results into an installed skill.

```text
benchmarks/<name>/                   # reusable definition
├── README.md                        # purpose, data, use, limitations
├── questions/                       # task records and split provenance
├── fixtures/                        # permitted source inputs
├── instructions/                    # worker, judge; optimizer if needed
└── evaluator/                       # rubric, references or executable graders

.octocode/benchmarks/<name>/results/<run-id>/
├── run.json                         # effective settings and identities
├── summary.md                       # result, evidence and limitations
└── trials/<trial-id>/                # only when trials actually run
    ├── input.json                   # actual exported worker input
    ├── output.json                  # answer/artifact reference and status
    ├── grade.json                   # grading evidence and status
    └── trace.jsonl                  # actions/tool results when available
```

This is a default layout, not a required serialization format. Reuse equivalent native runner logs and link them from the summary instead of duplicating data. Omit unused folders and files. A deterministic check may need only its run record, results and summary. Store large artifacts once and reference them by identity/hash.

## Run identity and lifecycle
- Allocate a unique run directory for each execution (for example UTC timestamp plus a short random suffix). Refuse collisions; never overwrite a completed run or silently reuse its files.
- Record the definition/subject/grader versions, task selection, model/runtime/tool settings, budget, sampling, environment and start/end status actually used. Include resource limits and calibration evidence when relevant. The benchmark README describes purpose; the run record captures execution settings.
- Keep distinct trial IDs for task × comparison arm × repetition × attempt. Record repairs and retries without replacing the original attempt. Missing/failed/Unknown results remain visible.
- Seal worker output before grading. If only the grader changes, save a new assessment/run with the parent run identity and reused output hashes; preserve old grades. Changing task inputs, context or environment requires new trials.
- Separate worker access from controller storage. The worker gets only its selected question, production-equivalent instruction and permitted fixtures—not the benchmark folder, run results, evaluator material or other trials. Private final tests require protected storage, not just a folder named private.

## Adapt to the benchmark
Use only the pieces that affect execution or interpretation. Questions should contain legitimate requirements without solution hints; rubric questions and reference answers stay evaluator-only. Match task/reference records by ID and split related cases by family. Avoid labels in worker-visible IDs that reveal the desired outcome.

Put actual setup/run/analysis commands in each benchmark README when a runner exists. For an instruction/data starter, say that directly rather than shipping a null-filled manifest or fake commands. Live isolation belongs to `references/clean-lab.md`; grading and repair guidance remain in `references/llm-judge.md` and `references/failure-repair.md`.
