# Question Bank — local-code / ast-grep-react-v2

Canonical 10-question bank for local structural/AST research on a frozen
Flow-typed React checkout. Subject: **local-code** (no GitHub access needed at
solve time beyond the initial pinned clone).

- `questions.md` — solver-facing prompts (Q1–Q10): AST parity, relational
  rules, cross-file identity, reachability, outlines, bounded reads.
- `ground-truth.json` — judge-only oracle (`questionBankId: ast-grep-react-v2`).
  Counts recompute at run time against the pinned checkout.

Used by: [`../../../compare/octocode-vs-ast-grep/`](../../../compare/octocode-vs-ast-grep/)
(suite keeps a frozen copy; verify the copy against this bank before a run).

Contracts: [methodology](../../../README.md) ·
[instructions](../../../INSTRUCTIONS.md) · [judging](../../../JUDGING.md) ·
[scoring](../../../SCORING.md).
