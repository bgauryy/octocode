# Reflection and Learning in Pi

Reflection uses the bundled Awareness CLI. It is not a Pi tool and it never
self-approves repository, prompt, skill, or harness changes.

## When to reflect

Reflect after a meaningful outcome when at least one item is reusable:

- a verified root cause or workaround;
- an architectural decision and its reason;
- a recurring failure signature;
- a concrete repository fix still required;
- an evidence-backed skill/harness improvement proposal.

Skip routine status, raw test output, obvious changes, secrets, and material
already authoritative in source/docs.

## Record an outcome

```bash
node "$OCTOCODE_AWARENESS_CLI" reflect record \
  --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" \
  --task "fix parser regression" --outcome worked \
  --lesson "Reject malformed escapes before tokenization" \
  --failure-signature "mechanism:tokenization|cause:late-validation" \
  --compact
```

Outcomes are `worked`, `partial`, or `failed`. Unknown values hard-error.

Reflection can route one observation to separate destinations:

- `--lesson` → durable learning;
- `--fix-repo` → repository follow-up/refinement;
- `--fix-harness` → supervised skill/tooling proposal;
- `--fix-instructions` → feedback to the human instruction author;
- `--failure-signature` → weakness clustering.

Do not use reflection as a second task queue. Selectable, dependency-aware work
belongs in a Plan Task; a live handoff belongs in a signal.

## Recall before risky work

```bash
node "$OCTOCODE_AWARENESS_CLI" memory recall \
  --query "tokenization" --workspace "$PWD" --smart --limit 5 --compact
```

Treat every result as a lead. Re-read cited source and rerun current checks when
the fact can affect a change.

## Supersede instead of stacking

When better evidence replaces a memory, create the corrected memory with
`--supersedes <memory-id>`. Prefer one current abstraction with provenance over
many near-duplicates.

```bash
node "$OCTOCODE_AWARENESS_CLI" memory record \
  --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" \
  --task-context "parser validation" \
  --observation "Validation now occurs before tokenization" \
  --label DECISION --importance 7 --supersedes mem_old \
  --reference file:src/parser.ts --compact
```

## Mine recurrent weakness

Failure signatures make repeated mechanisms queryable:

```bash
node "$OCTOCODE_AWARENESS_CLI" reflect mine-weakness \
  --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" \
  --min-count 2 --limit 10 --compact
```

Clusters are prompts for investigation, not proof. Confirm exact failures and
affected files before opening a task or proposing guidance.

## Harness proposals are human-gated

```bash
node "$OCTOCODE_AWARENESS_CLI" reflect export-harness \
  --workspace "$PWD" --limit 20 --compact
```

Export is a preview. A human reviews evidence, scope, regressions, held-out
behavior, and rollback before changing protected instructions or skills. The
agent that proposes a rule does not certify it.

## Developer review

Use `--fix-instructions` when the issue is the instruction itself, then inspect:

```bash
node "$OCTOCODE_AWARENESS_CLI" reflect developer-review \
  --workspace "$PWD" --format markdown
```

Keep feedback specific: quote the conflicting requirement, show the observed
effect, and propose the smallest correction.

## Cleanup and projection

Awareness CLI maintenance is report-first:

```bash
node "$OCTOCODE_AWARENESS_CLI" maintenance digest \
  --workspace "$PWD" --dry-run --compact
node "$OCTOCODE_AWARENESS_CLI" memory forget \
  --tag EXPERIENCE --before 2026-01-01 --dry-run --compact
```

Review the dry-run output before any mutation. After approved cleanup,
refresh `.octocode/` only if file-based readers need current projections:

```bash
node "$OCTOCODE_AWARENESS_CLI" wiki sync \
  --workspace "$PWD" --mode local --compact
```
