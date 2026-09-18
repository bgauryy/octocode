# Bug-Triage Benchmark — Results
**Suite:** bug-triage-v1  
**Status:** NOT YET RUN

Run `node inspect.mjs --aggregate` after all agents complete and grades are filed to populate this file.

## How to run

```sh
# 1. Freeze the suite (run once before spawning any agents)
node benchmark/harness.mjs --freeze

# 2. Preflight check
node benchmark/harness.mjs --preflight

# 3. Print all 20 agent prompts and spawn via Pi workflow / Codex / equivalent
node benchmark/harness.mjs --prompts

#    Or print one at a time:
node benchmark/harness.mjs --prompt BUG-01 baseline
node benchmark/harness.mjs --prompt BUG-01 treatment

# 4. After all 20 agents complete, check completeness
node benchmark/inspect.mjs --summary
node benchmark/inspect.mjs --save

# 5. Spawn 10 judge agents (one per case)
node benchmark/judge.mjs --prompt BUG-01   # paste to judge agent
# ... repeat for BUG-02 through BUG-10

# 6. Validate all grades
node benchmark/judge.mjs --validate-all

# 7. Aggregate and write RESULTS.md
node benchmark/inspect.mjs --aggregate
```

## Expected output files

```
benchmark/
├── frozen.json                       ← written by harness --freeze
├── metrics.json                      ← written by inspect --save
├── RESULTS.md                        ← written by inspect --aggregate
├── runs/
│   ├── baseline/
│   │   ├── BUG-01/answer.md
│   │   ├── BUG-01/result.json
│   │   └── ... (BUG-02 through BUG-10)
│   └── treatment/
│       ├── BUG-01/answer.md
│       ├── BUG-01/result.json
│       ├── BUG-01/jev-calls.json
│       ├── BUG-01/decision-before.json
│       ├── BUG-01/decision-after.json
│       └── ... (BUG-02 through BUG-10)
└── grades/
    ├── BUG-01/grade.json
    └── ... (BUG-02 through BUG-10)
```
