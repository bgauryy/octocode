# Output

One directory per run.

```
output/
└── <UTC-timestamp>-<agent>/
    ├── log.jsonl           ← every tool call: {q, agent, cmd, in_chars, out_chars, elapsed_ms, exit}
    ├── q1/output.md
    ├── …
    ├── q31/output.md
    └── output.md           ← aggregated by finalize.mjs
```

Per-question file:

```markdown
# Q{N} — <title>

## Metadata
| Field      | Value |
|------------|-------|
| Model      | …     |
| Calls      | int   |
| In Chars   | int   |
| Out Chars  | int   |
| Elapsed ms | int   |

## Answer
…full findings…
```

Judge output (cross-run comparison): `output/judge-<run_a>-vs-<run_b>.md`.
