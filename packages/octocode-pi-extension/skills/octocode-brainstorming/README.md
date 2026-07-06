# Octocode Brainstorming

`octocode-brainstorming` gives an agent a disciplined way to explore fuzzy ideas before anyone commits to a feature, library, workflow, or product direction.

The skill is built for the moment when a user asks, "Is this worth building?", "Has anyone already tried this?", or "What is the sharper version of this idea?" It keeps the conversation creative, but it makes the final answer answerable to evidence.

## The Problem

Early ideas are fragile. If the agent searches only for the user's first wording, it may miss better framings. If it jumps straight to enthusiasm, it can turn a vague hunch into roadmap debt. If it only lists prior art, it can still avoid the harder question: what should we do next?

This skill makes the agent diverge before it converges. It frames the idea several ways, checks evidence across the right surfaces, notices conflict, and then recommends one practical next move.

## Capabilities

- Alternative framings that prevent the first wording from anchoring the whole search.
- A surface plan that explains which sources matter: local code, web resources, GitHub, packages, and exact code reads.
- Top-resource-first prior-art mapping, so articles, docs, papers, and official sources seed repo and package research.
- Claim tracking, with confidence and next-query thinking instead of unsupported assertions.
- Cross-surface loops where web findings lead to code reads and code findings send the agent back to resources.
- Perspective review through critical, entrepreneurial, and product lenses.
- Conflict handling that concedes weak or contradictory evidence before the verdict.
- A final decision shape such as build RFC, prototype, narrow, park, or do not build.

## Operating Model

The workflow is:

```text
FRAME -> DIVERGE -> RESEARCH -> CROSS-POLLINATE -> STRESS-TEST -> SYNTHESIZE -> DECIDE
```

The agent first turns the idea into testable framings. It then researches the strongest surfaces, follows leads from one surface into another, and stress-tests the emerging thesis. The answer is not complete until the agent has stated what survived review, what was weakened, and what next step would change the decision.

## User Experience

For users, this skill feels like a product-minded technical partner. It does not merely say "yes" or "no." It names the better angle, shows which evidence supports it, marks uncertainty, and gives a next action that is small enough to be useful.

The output is a decision brief rather than code or design. When the idea becomes ready for architecture or implementation planning, it can hand off cleanly to research or RFC work.

## Installation

Install the published skill with:

```bash
npx octocode skill --name octocode-brainstorming
```

## Configuration — web search keys

The skill runs web research via Tavily and/or Serper. Put your keys in the **unified Octocode env file** — not in a skill-local `.env`:

```bash
# ~/.octocode/.env  (macOS/Linux default; Windows: %APPDATA%\.octocode\.env)
TAVILY_API_KEY=tvly-...
SERPER_API_KEY=...
```

Get keys: [Tavily](https://app.tavily.com/) · [Serper](https://serper.dev/) · both is fine, one is enough.

**How the keys reach the skill:**
- Under the `octocode-agent` / Pi extension — `propagateOctocodeEnv` runs at session start and injects `~/.octocode/.env` into `process.env`. Every subprocess (bash calls, hooks, script invocations) inherits the full env automatically.
- When the scripts are run standalone from the published build — `octocode-config.mjs` is bundled alongside each script and loads the same file directly.

**Key priority:** `process.env` (shell / agent session) > `~/.octocode/.env` (global) > `<project>/.octocode/.env` (project, when trusted). The search scripts never overwrite an already-set value.

Verify a key is working:

```bash
node <skill_dir>/scripts/tavily-search.mjs --check
node <skill_dir>/scripts/serper-search.mjs --check
```

The provider ladder is: Tavily → Serper → DuckDuckGo (no key needed, lower quality). The skill uses whichever exits 0 first; with no key it falls back to DuckDuckGo and reports the limitation once.

## Maintainer Notes

Keep the README focused on the reasoning model: divergent framing, resource-first research, surface loops, conflict concessions, and decision usefulness. Keep operational details, eval mechanics, and web adapter behavior in the agent-facing skill file and references.
