# Octocode RFC Generator

Turn a consequential technical choice into an evidence-backed RFC, design document, migration plan, architecture proposal, or measurable implementation contract. Agent rules live in `SKILL.md`.

## Install

```bash
npx -y octocode skill install octocode-rfc-generator
```

## Maintainer verification

Run each script's `--self-test` (scripts are listed in `SKILL.md` § Scripts), then the `octocode-skills` review against this folder.

## Sources

Attributions, not runtime dependencies: `octocode-research` (clasify gate: admission, one call per crossroad, credentials), `octocode-subagent` (worker packets, barriers, parent verification), `octocode-eval-benchmark` (frozen baselines, held-out outcomes, whole-workflow cost), and the [Jev documentation index](https://docs.typesafe.ai/llms.txt) for provider semantics. Native Octocode owns transport, paging, and typed validation.
