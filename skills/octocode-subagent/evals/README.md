# Orchestration Evaluation

Load when changing the tool-using orchestration contract. Why: the suite separates real delegation value from solo, batch, approval-blocked, and correlated-consensus near misses.

- `cases.json` is the frozen training/regression suite merged from `octocode-orchestrator`.
- `premerge-forward-results.json` is historical provenance: 23/23 regression cases passed against the pre-merge orchestrator subject. Its digest is intentionally stale for this merged skill and must never be reported as post-merge proof.
- `scripts/eval-contract.mjs` validates the suite with no arguments. Pass `--results <fresh-receipt.json>` only after fresh-context evaluators run against the current printed subject digest.

```bash
node scripts/eval-contract.mjs
node scripts/eval-contract.mjs --print-digest
node scripts/eval-contract.mjs --results <fresh-receipt.json>
```

Do not edit cases, graders, or digests to make a subject pass. New held-out prompts stay outside this folder until their verdict.

Next: use `octocode-eval-benchmark` to run the fresh held-out loop; after acceptance return to `references/completion.md`.
