# Octocode Brainstorming

Explore an uncertain idea before committing to a feature, workflow, library, or product direction.

## Use when

- You need distinct framings, prior art, or adjacent solutions.
- The worth-building question is still open.
- The right next decision can be Build RFC, Prototype, Narrow, Park, or do not build.

## Capabilities

- Prevents the first wording from anchoring the investigation.
- Plans evidence across local code, repositories, packages, and the web.
- Tracks claims, sources, confidence, conflicts, and the next proof.
- Stress-tests ideas through critical, entrepreneurial, and product lenses.
- Produces a compact decision brief or an approved saved artifact.

## Workflow

```text
FRAME → DIVERGE → RESEARCH → CROSS-POLLINATE → STRESS-TEST → SYNTHESIZE → DECIDE
```

Use `octocode-rfc-generator` after a Build verdict and `octocode-research` when a factual technical question becomes the main task.

## Install

```bash
npx octocode skill install octocode-brainstorming --platform codex
```

## Optional search credentials

The skill can check Tavily, Serper, or Exa credentials through its scripts. Store keys in the process environment, a trusted workspace `.octocode/.env`, or the Octocode home `.env`; never put them in the skill folder.

```bash
node scripts/tavily-search.mjs --check
node scripts/serper-search.mjs --check
node scripts/exa-search.mjs --check
```

One configured provider is sufficient. The host web tool performs searches; these scripts only verify credential availability.

## Maintainer verification

Run the `octocode-skills` review against this folder.
