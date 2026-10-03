# Octocode Brainstorming

Explore an uncertain idea before committing to a feature, workflow, library, or product direction. Agent rules live in `SKILL.md`; the 18+ exploratory mode lives in `references/exploratory.md`.

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
