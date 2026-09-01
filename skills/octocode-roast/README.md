# Octocode Roast

`octocode-roast` makes code critique memorable without making it careless. Use it for a roast, brutal review, debt ranking, or sharp explanation of what is wrong with a codebase. It targets code patterns, never people.

## The Problem

Polite review may not land; harsh feedback without evidence is noise. A useful roast pairs each major jab with code, impact, confidence, and a repair path.

## Capabilities

- Explicit target precedence, so user-specified files are reviewed before staged or branch-wide changes.
- Evidence-backed findings with `file:line` citations, impact, confidence, and repair paths.
- Severity tiers that keep security, data loss, correctness, and production impact above style noise.
- Tone calibrated to the request and the sensitivity of the code.
- Secret-safe handling for credentials, security findings, and production-sensitive paths.
- Language-specific smell patterns and code-search strategies.
- A top-offender autopsy when one pattern explains many issues.
- Redemption paths and a checkpoint before edits are made.

## Operating Model

The workflow is:

```text
TARGET -> INSPECT -> INVENTORY -> AUTOPSY -> CHECKPOINT -> REDEEM
```

The agent scopes the target, gathers exact evidence, ranks the damaging patterns, and writes the roast after the proof is in. Pattern matches remain leads until verified; humor is seasoning, not evidence.

## User Experience

Users get critique that is hard to ignore and easy to act on: the strongest roast, ranked findings, an autopsy, and repair paths. If the request did not already authorize fixes, the skill waits at a fix checkpoint before editing.

## Installation

Install the published skill with:

```bash
npx octocode skill --name octocode-roast
```

## Maintainer Notes

Keep this README focused on the balance: memorable critique, real citations, tone safety, and repairability. Keep detailed issue catalogs, tone personas, language-specific checks, and redemption flow in the agent-facing skill file and references.
