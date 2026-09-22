# Octocode Brainstorming

Explore an uncertain idea before committing to a feature, workflow, library, or product direction.

## Use when

- The worth-building question is still open.
- You need distinct framings, prior art, or adjacent solutions before a decision.
- The right outcome is one of: Build RFC, Prototype, Narrow, Park, or Do Not Build.

## Not for

- Implementing a settled decision → start directly or use `octocode-rfc-generator`
- Researching a specific factual technical question → `octocode-research`
- Writing or reviewing documentation → `octocode-documentation`

## Workflow

```text
FRAME → DIVERGE → RESEARCH → CROSS-POLLINATE → STRESS-TEST → SYNTHESIZE → DECIDE
```

A Build verdict hands off to `octocode-rfc-generator`. The output is a decision brief or an approved saved artifact.

## Install

```bash
npx -y octocode skill install octocode-brainstorming
```

## Optional search credentials

Store Tavily, Serper, or Exa keys in the process environment, a trusted workspace `.octocode/.env`, or the Octocode home `.env`. One configured provider is sufficient.

```bash
node scripts/tavily-search.mjs --check
```

## Maintainer verification

Run the `octocode-skills` review against this folder.
