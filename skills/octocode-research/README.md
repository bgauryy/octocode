# Octocode Research

`octocode-research` is the broad technical research skill for evidence-first answers. It can map a landscape, validate a technical direction, investigate behavior, or plan a change using local code, GitHub, npm, PRs, history, artifacts, binaries, OQL packets, papers, specs, and official docs.

It is a research skill, not an implementation session.

## How it works

The skill classifies the request as map, validate, investigate, or plan, then selects only the useful evidence surfaces: local code, GitHub, npm, history, artifacts, specs, papers, or official docs. It starts broad enough to avoid tunnel vision, deep-reads the strongest anchors, marks confidence explicitly, and recommends one next action.

## Good asks

- "Research this technical area and tell me what matters."
- "Why does this tool or package behave this way?"
- "What options exist across local code, GitHub, npm, and docs?"
- "What evidence supports this implementation direction?"
- "Find the files, repos, packages, or docs that prove the answer."

## What you get

- A one-line scope: question, corpus, mode, and active surfaces.
- A small hypothesis map before the agent narrows too early.
- Evidence grouped by surface rather than dumped as raw results.
- Exact anchors such as file:line, repo path, package id, PR number, commit, artifact fact, or fetched formal URL.
- Confidence marked as confirmed, likely, or uncertain.
- One recommended next action.

## Research modes

| Mode | Best for | User result |
|---|---|---|
| Map | Prior art or "what exists?" | Landscape clusters and the strongest evidence. |
| Validate | "Is this direction worth it?" | A verdict with supporting and weakening signals. |
| Investigate | "Why does this happen?" | Root cause or behavior explanation with proof. |
| Plan | "What path should we take?" | Current-state evidence, options, and a safe next step. |

## Use another skill when

- The idea is still fuzzy or market-like: use `octocode-brainstorming`.
- The user wants code changes, review, or refactoring: use `octocode-engineer`.
- The user specifically wants repeated Act -> Observe loops: use `octocode-loop`.
- The result should be a full RFC or proposal: use `octocode-rfc-generator`.

## User value

This skill gives users a researched answer with traceable proof and honest uncertainty. It is useful when the answer spans more than one source but does not yet require code edits.
