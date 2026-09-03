# Octocode Documentation

Create evidence-backed documentation for humans and coding agents, with routed Google developer documentation style guidance and deterministic Markdown checks.

## Use when

- A README, tutorial, how-to, reference, runbook, ADR, or migration guide is missing or stale.
- Agent instructions such as `AGENTS.md` or `CLAUDE.md` need restructuring.
- Existing prose needs a factual or style review.

## Capabilities

- Classifies work as agent docs, human docs, ADR, multi-file pack, or style pass.
- Verifies repository claims before writing.
- Keeps one Diátaxis type per human-facing page.
- Prefers durable links and ownership pointers over copied code or line numbers.
- Routes wording questions to focused style references and the bundled word list.

## Workflow

```text
UNDERSTAND → RESEARCH → CLASSIFY → OUTLINE GATE → WRITE → STYLE → VERIFY
```

The Markdown linter reports ERROR, WARN, and INFO findings. Non-Markdown text still requires a manual style check.

## Install

```bash
npx octocode skill install octocode-documentation --platform codex
```

## Maintainer verification

```bash
node scripts/style-lint.mjs README.md
node scripts/style-lint.mjs --self-test
```

Then run the `octocode-skills` review against this folder.

Upstream references: [Google developer documentation style guide](https://developers.google.com/style), [Diátaxis](https://diataxis.fr/), and [agents.md](https://agents.md/).
