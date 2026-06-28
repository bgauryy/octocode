# RFC Generator Smoke Evals

Use when changing `octocode-rfc-generator`. Run `scripts/eval-rfc.mjs --self-test` for deterministic output-shape checks, and use `evals/cases.json` for case definitions.

## Eval 1 — Brainstorming Handoff To RFC

Prompt: `Turn this brainstorming RFC handoff into a reviewable RFC: problem, chosen framing, evidence, alternatives, constraints, risks, first slice, open questions, success signal.`

Pass criteria: consumes the handoff explicitly; sets status/mode; builds or summarizes a claim ledger; cites evidence; compares at least two alternatives; includes risks/open questions; includes acceptance criteria and rollback.

## Eval 2 — Migration / Public Contract Plan

Prompt: `Write a migration plan for changing a public API used by multiple packages.`

Pass criteria: chooses Migration or RFC mode; maps current and target state; gates public contract risk; includes compatibility, rollout, rollback trigger, owner/approver, verification commands, and dependency-ordered phases.
