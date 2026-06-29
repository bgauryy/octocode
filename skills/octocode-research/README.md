# Octocode Research

`octocode-research` is the main evidence-first skill for technical and code work. It covers investigation, implementation planning, code changes, PR or local diff review, refactors, architecture analysis, dead-code proof, artifact inspection, prior-art mapping, and repeated Act -> Observe -> Learn loops.

Use it whenever an answer, review, decision, or patch should be grounded in code and verifiable evidence.

## When to use

- Find a root cause and explain it with proof.
- Review a PR, local diff, module, or architecture slice.
- Implement a scoped code change and verify it.
- Plan or de-risk a refactor before editing.
- Map prior art across local code, GitHub, npm, docs, specs, or papers.
- Inspect generated artifacts, archives, binaries, or `.wasm`/`.node` files.
- Continue searching until a clear question converges or the evidence budget is exhausted.

Use `octocode-brainstorming` when the idea is still fuzzy. Use `octocode-rfc-generator` when the result should be a formal proposal. Use `octocode-roast` when the user wants critique as entertainment.

## Features

- Mode selection for Map, Validate, Investigate, Plan, Review, Change, and Loop tasks.
- One-line scope that states question, corpus, mode, and active evidence surfaces.
- Cheap orientation before exact reads.
- Evidence grouped by surface instead of raw search dumps.
- Exact anchors such as `file:line`, repo path, package id, PR number, commit, artifact fact, or fetched URL.
- AST, structural search, LSP, history, package, GitHub, and local-file workflows through Octocode when available.
- Confidence labels for confirmed, likely, uncertain, and weak claims.
- Verification reporting for code changes and reviews.
- Long-form brief support when the task needs a durable artifact.

## How it works

The skill follows this flow:

```text
SCOPE -> SEARCH -> READ EXACT -> VALIDATE -> DECIDE/PATCH -> VERIFY
```

It chooses a mode from the user's intent, starts with low-cost discovery, reads exact evidence once anchors appear, checks findings before presenting them, and either recommends a next action or applies the smallest requested patch with verification.

## Internal flow

| Mode | Best for | Output |
|---|---|---|
| Map | Prior art, ecosystems, "what exists?" | Landscape clusters and strongest evidence. |
| Validate | "Should we add or build this?" | Verdict with supporting and weakening signals. |
| Investigate | "Why does this happen?" | Root cause or behavior explanation with proof. |
| Plan | "What path should we take?" | Current-state evidence, options, and safe next step. |
| Review | PR, local diff, or code-quality review | Severity-ranked findings with citations. |
| Change | User asked for edits now | Minimal patch plus actual verification. |
| Loop | Clear question needs convergence | Answer, decisive observations, gaps, and verification. |

Supporting references split the workflow: `research-flow.md` for research modes, `code-research.md` for implementation/review/refactor work, `loop-research.md` for repeated loops, `finding-checks.md` for validation, `long-research.md` for durable reports, and `github-landscape.md` for ecosystem comparisons.

## Installation

Install the published skill:

```bash
npx octocode skill --name octocode-research
```

Install from a GitHub path or fork:

```bash
npx octocode skill --add bgauryy/octocode/skills/octocode-research
```

## Benefits

- Replaces search-and-guess with proof-carrying technical work.
- Gives users traceable evidence and honest uncertainty.
- Keeps implementation changes scoped to what the evidence supports.
- Handles research and code work in one skill so agents do not split context across similar workflows.

## For developers

Keep `SKILL.md` as the compact router for modes, evidence rules, references, scripts, and output shape. Keep workflow depth in `references/research-flow.md`, code-change specifics in `references/code-research.md`, loop mechanics in `references/loop-research.md`, validation rules in `references/finding-checks.md`, and long-report artifacts in `references/long-research.md`. Run `scripts/eval-research.mjs --self-test` after prompt or reference changes.
