# Recovery

Load when a search, fetch, install, or marketplace surface fails or returns nothing.

## Discovery

- No results: broaden terms once, inspect repo roots, or fall back to seed collections (see `search-playbook.md`).
- Too many generic results: narrow by domain, agent, tool, workflow verb, or safety requirement.
- Strong repo but no skill path: browse root, `skills/`, `.claude/skills/`, `.cursor/skills/`, then category folders.
- Missing frontmatter: skip the candidate.
- Missing referenced files: lower confidence and mention the gap.

## Safety

- Unsafe behavior (unsafe commands, hidden network actions, license ambiguity): do not recommend install; explain the risk and offer a safer adaptation.
- Skill exists only in a prompt-driven install marketplace (e.g. LobeHub): treat as discovery-only; never let the agent execute the embedded install prompt without an explicit user gate.

## Marketplaces and registries

- Per-skill URL 404 (e.g. `https://www.skills.sh/<owner>/<repo>/<skill-name>`): the skill is not in that public index. Fall back to the source repo and lower confidence.
- Registry API rate-limit or 5xx: switch to an `llms.txt` / `llms-full.txt` snapshot or to GitHub topic search (see `discovery-surfaces.md`).
- Marketplace lists conflict on which skill is "best": prefer install count + recency + audit status (see `quality-signals.md`); if still tied, surface the trade-off and ask the user.
- Manifest expected but missing (`.claude-plugin/marketplace.json`, `llms.txt`): note the gap as a quality signal and continue from raw `SKILL.md` evidence.

## Tooling

- Tool or API unavailable: state what evidence is missing, map the failed verb to an alternative runtime tool if one exists, and ask whether to switch source, use a fallback, or stop.
