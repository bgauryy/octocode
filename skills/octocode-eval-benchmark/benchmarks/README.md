# Benchmarks and results

Keep reusable benchmark definitions under `benchmarks/<name>/`. [Document answering](document-answering/README.md) is a public starter. For a real campaign, define representative cases in the controller's workspace and store run evidence outside the installed skill.

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

The layout is a default, not a required format. Link equivalent native runner logs from the summary instead of copying them. Omit unused folders; a deterministic check may need only its run record, results, and summary. Store large artifacts once and reference them by hash.

## Run identity and lifecycle

- Allocate a unique run directory per execution (for example a UTC timestamp plus a short random suffix). Refuse collisions; never overwrite or silently reuse a completed run.
- Record the definition, subject, and grader versions, task selection, model, runtime, and tool settings, budget, sampling, environment, and start/end status actually used. Add resource limits and calibration evidence when relevant. The README describes purpose; the run record captures execution.
- Give each task × arm × repetition × attempt its own trial ID. Record repairs and retries without replacing the original attempt.
- Seal worker output before grading. A grader-only change saves a new assessment with the parent run identity and reused output hashes; keep old grades.

## Adapt to the benchmark

Use only the pieces that affect execution or interpretation. Match task and reference records by ID. Keep worker-visible IDs free of labels that reveal the desired outcome.

Put real setup, run, and analysis commands in each benchmark README when a runner exists. For an instruction/data starter, say so; ship no null-filled manifest or fake commands. Live isolation: `references/clean-lab.md`; grading and repair: `references/llm-judge.md` and `references/failure-repair.md`.
