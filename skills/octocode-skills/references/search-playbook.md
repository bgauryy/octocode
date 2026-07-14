# Search Playbook

Load when discovering skill candidates. Why: sets depth, fan-out, and angles before shopping registries.

## Set depth

- Quick: enough to recommend one best candidate with caveats.
- Research: compare broadly; stop when more search won't change the pick.
- Install: inspect source, support files, destinations, conflicts before approval.
- Improve/rate/review/create: inspect target + local examples + `skill-anatomy.md` first.
- Weak results: broaden once, then report the gap.

## Parallel three-surface fan-out

For every PUBLIC query, fan out IN PARALLEL, then dedupe by `(owner/repo, skill name)`:

1. Octocode/GitHub — via `octocode.md` / `octocode-research`.
2. skills.sh API — install-ranked (below).
3. Web search — topic + "agent skill"/"SKILL.md"; confirm real `SKILL.md` before recommend.

Skip public surfaces for local/org-private scopes — Octocode only.

## Search angles

Name (exact, hyphenated, aliases) · subject · workflow verbs · ecosystem (agent/IDE/lang/MCP) · safety (gate, verify, scripts).

## Skills.sh API

```bash
curl 'https://www.skills.sh/api/search?q={{SEARCH_KEY}}&limit=100' --compressed \
  -H 'User-Agent: Mozilla/5.0'
```

Sort by `installs` desc → top 5 inspect targets → fetch each `SKILL.md` via Octocode. Installs are a tiebreaker, not a blind recommend. Unreachable → leaderboard + GitHub topics; lower confidence (`recovery.md`).

## Sparse discovery

Seed from `topic:agent-skills` and maintained collections: `anthropics/skills`, `vercel-labs/skills`, `obra/superpowers`, `microsoft/skills`, `trailofbits/skills`.

Next: when picking a registry load `references/discovery-surfaces.md`; when judging load `references/quality-rubric.md`; when ranking load `references/quality-signals.md`.
