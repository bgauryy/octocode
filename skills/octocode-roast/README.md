# Octocode Roast

`octocode-roast` gives an agent permission to make code critique memorable without making it careless. It is blunt, funny, evidence-backed, and aimed at code patterns rather than people.

Use it when the user explicitly wants a roast, brutal review, code-quality critique, or a sharp explanation of what is wrong with a codebase.

## The Problem

Normal review can be too polite to land. But harsh feedback without citations is just noise. A roast needs both edge and receipts: every major jab should point to code, explain impact, and offer a repair path.

This skill gives the agent a tone-controlled critique mode that stays useful, safe, and fixable.

## Capabilities

- Explicit target precedence, so user-specified files are reviewed before staged or branch-wide changes.
- Evidence-backed findings with `file:line` citations, impact, confidence, and repair paths.
- Severity tiers that keep security, data loss, correctness, and production impact above style noise.
- Tone calibration from gentle to savage based on the user's wording and the sensitivity of the code.
- Secret-safe handling for credentials, security findings, and production-sensitive paths.
- Language-specific smell patterns and code-search strategies.
- A top-offender autopsy when one pattern explains many issues.
- Redemption paths and a checkpoint before edits are made.

## Operating Model

The workflow is:

```text
TARGET -> INSPECT -> INVENTORY -> AUTOPSY -> CHECKPOINT -> REDEEM
```

The agent scopes the target, inspects enough code to cite major issues, builds an issue inventory, ranks the most damaging patterns, and writes the roast after the evidence is in. Pattern matches are leads until exact code evidence upgrades them. The humor is seasoning, not the proof.

## User Experience

Users get critique that is hard to ignore and easy to act on. The answer should lead with the strongest roast, then group findings by severity, explain why they matter, and show repair paths.

The skill waits at a fix checkpoint. It can point toward remediation, but it should not silently switch from roast mode into edit mode without the user choosing that path.

## Installation

Install the published skill with:

```bash
npx octocode skill --name octocode-roast
```

## Maintainer Notes

Keep this README focused on the balance: memorable critique, real citations, tone safety, and repairability. Keep detailed issue catalogs, tone personas, language-specific checks, and redemption flow in the agent-facing skill file and references.
