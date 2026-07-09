# Evaluation Data & Failure Ledger

Load before claiming that a prompt, instruction, description, schema, or tool change improved reliability.

**Measure the behavior, not the prose.** Use realistic tasks with verifiable outcomes; preserve a held-out set so wording is not tuned only to the examples it saw.

## Scenario record

| Field | Capture |
|---|---|
| ID + intent | Realistic user goal and why it matters |
| Setup/data | Minimal fixture, permissions, and relevant ambiguity |
| Expected outcome | Observable result; permit valid alternate paths |
| Verifier | Assertion, invariant, or calibrated judge rubric |
| Risks | Wrong tool, invalid input, unsafe mutation, context bloat, or wrong answer |

## Comparison workflow

1. Freeze baseline scenarios and run the current version.
2. Change one contract or instruction hypothesis at a time.
3. Run the same scenarios plus held-out cases; inspect raw calls/results, not only agent commentary.
4. Track success, tool-selection accuracy, invalid calls, tool errors, calls/task, latency, input/output tokens, and required follow-up pages.
5. Keep a change only when it improves the target metric without an unacceptable regression; otherwise revert or narrow it.

## Failure ledger

```markdown
Case: <ID> | Stage: select/call/read/act | Symptom: <observable failure>
Cause: <evidence, not guess> | Repair: <smallest change> | Recheck: <case IDs>
```

Avoid toy single-call tests for multi-step work, exact-match verifiers that reject valid alternatives, and evaluation cases that expose secret or production data.

## Sources
- Anthropic, [Writing effective tools for AI agents](https://www.anthropic.com/engineering/writing-tools-for-agents) — realistic evaluation tasks, verifiers, held-out tests, and tool-use metrics.
- Anthropic, [Effective context engineering for AI agents](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents) — evaluate context quality as part of agent behavior.
