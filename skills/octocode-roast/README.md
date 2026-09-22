# Octocode Roast

Deliver a memorable, evidence-backed code critique with ranked impact and practical repair paths.

## Use when

- You request a roast, brutal review, debt ranking, or hot-path autopsy.
- Security, correctness, performance, design, testing, or maintainability problems need sharper framing.
- A blunt but constructive review would be more useful than a polite one.

## Not for

- A standard architectural review → `octocode-architect`
- Behavior-preserving cleanup → `octocode-clean-agentic-code`
- Writing or fixing documentation → `octocode-documentation`

## Commitments

- Target code patterns, never people.
- Cite or drop every major finding.
- Rank security, data loss, correctness, and production impact above style.
- Redact secrets and keep sensitive findings restrained.
- Do not edit until the request authorizes fixes or you accept a repair at the checkpoint.

## Workflow

```text
TARGET → INSPECT → INVENTORY → AUTOPSY → CHECKPOINT → REDEEM
```

Pattern matches remain leads until verified. Humor supports the finding; it never replaces evidence.

## Install

```bash
npx -y octocode skill install octocode-roast
```

## Maintainer verification

Run the `octocode-skills` review against this folder.
