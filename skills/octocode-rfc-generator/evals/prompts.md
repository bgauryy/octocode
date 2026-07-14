# RFC Generator Smoke Evals

Use when changing `octocode-rfc-generator`. Run `scripts/eval-rfc.mjs --self-test` for deterministic output-shape checks, and use `evals/cases.json` for case definitions.

## Eval 1 — Brainstorming Handoff To RFC

Prompt: `Turn this brainstorming RFC handoff into a reviewable RFC: problem, chosen framing, evidence, alternatives, constraints, risks, first slice, open questions, success signal.`

Pass criteria: consumes the handoff explicitly; sets status/mode; builds or summarizes a claim ledger; cites evidence; compares at least two alternatives; includes risks/open questions; includes acceptance criteria and rollback.

## Eval 2 — Migration / Public Contract Plan

Prompt: `Write a migration plan for changing a public API used by multiple packages.`

Pass criteria: chooses Migration or RFC mode; maps current and target state; gates public contract risk; includes compatibility, rollout, rollback trigger, owner/approver, verification commands, and dependency-ordered phases.

## Eval 3 — Existing-Code Folder RFC

Prompt: `Write a full RFC for an irreversible existing-code cross-package change as the folder set (RFC.md, PREREQUISITES.md, IMPLEMENTATION.md, KPI.md, RESOURCES.md): goals/non-goals, reversibility, alternatives, prerequisites before the plan, open questions closed via research, resources/refs, acceptance criteria, success metrics, and a traceability matrix.`

Pass criteria: names the folder files including PREREQUISITES.md and RESOURCES.md; classifies decision reversibility; states Goals and Non-Goals; compares alternatives; closes RFC open questions in IMPLEMENTATION.md with citations; KPI.md has user stories, Gherkin acceptance criteria, success metrics with baseline/target/guardrail, a decision rule, and a traceability matrix binding the files.
