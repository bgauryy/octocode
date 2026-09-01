# Octocode Brainstorming

`octocode-brainstorming` explores fuzzy ideas before anyone commits to a feature, library, workflow, or product direction. It answers questions such as “Is this worth building?”, “Has anyone tried it?”, and “What is the sharper version?” with a creative but evidence-backed decision.

## The Problem

Searching only the user's first wording misses better framings; enthusiasm without evidence creates roadmap debt. The skill reframes, checks the relevant surfaces, exposes conflicts, and recommends one practical next move.

## Capabilities

- Alternative framings that prevent the first wording from anchoring the whole search.
- A surface plan that explains which sources matter: local code, web resources, GitHub, packages, and exact code reads.
- Prior-art mapping across official resources, papers, repositories, packages, and local code.
- Claim tracking with confidence and a next proof instead of unsupported assertions.
- Cross-surface research where web findings lead to code reads and code findings refine web queries.
- Perspective review through critical, entrepreneurial, and product lenses.
- Conflict handling that concedes weak or contradictory evidence before the verdict.
- A final decision shape such as build RFC, prototype, narrow, park, or do not build.

## Operating Model

The workflow is:

```text
FRAME -> DIVERGE -> RESEARCH -> CROSS-POLLINATE -> STRESS-TEST -> SYNTHESIZE -> DECIDE
```

The agent turns the idea into testable framings, researches the useful surfaces, follows cross-surface leads, and stress-tests the thesis. It reports what survived, what weakened, and what next step could change the decision.

## User Experience

The output is a decision brief rather than code: the strongest angle, supporting evidence, uncertainty, and a small next action. Ideas ready for design can hand off to RFC work.

## Installation

Install the published skill with:

```bash
npx octocode skill --name octocode-brainstorming
```

## Optional web search keys

The skill can use Tavily, Serper, and Exa. Put keys in `<octocode-home>/.env`, not a skill-local file. Octocode home defaults to `~/.octocode` and honors `$OCTOCODE_HOME`.

```bash
TAVILY_API_KEY=tvly-...
SERPER_API_KEY=...
EXA_API_KEY=...
```

Get keys: [Tavily](https://app.tavily.com/) · [Serper](https://serper.dev/) · [Exa](https://dashboard.exa.ai/). One is enough. Scripts load process env, trusted workspace `.octocode/.env`, then Octocode home without overwriting an existing value.

Verify a key is working:

```bash
node <skill_dir>/scripts/tavily-search.mjs --check
node <skill_dir>/scripts/serper-search.mjs --check
node <skill_dir>/scripts/exa-search.mjs --check
```

The agent chooses engines by need and adds independent coverage when it can change the decision. DuckDuckGo is the no-key fallback; reduced coverage is reported.

## Maintainer Notes

Keep the README focused on the reasoning model: divergent framing, resource-first research, surface loops, conflict concessions, and decision usefulness. Keep operational details, eval mechanics, and web adapter behavior in the agent-facing skill file and references.
