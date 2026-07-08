# Parallel Roasting — Multi-Agent Sin Hunting

> **Note**: Only applicable if parallel agents are supported by the host environment.

## When to Spawn Subagents
- Large codebase with 5+ distinct modules/directories
- Multiple sin categories to hunt (security + performance + architecture)
- Monorepo with separate packages to roast

## How to Parallelize
1. Use the host's task tracker (or an in-chat checklist) to identify independent roast domains
2. Use the host's parallel subagent mechanism to spawn subagents per domain/sin category
3. Each agent hunts sins independently using local tools
4. Merge findings, deduplicate, prioritize by severity
5. **IF** the host cannot run true parallel work → **THEN** execute the same domains sequentially

Each worker returns this compact contract:

| Field | Requirement |
|---|---|
| Scope | Exact files/directories inspected |
| Findings | `file:line`, evidence, impact, confidence, repair move |
| Non-findings | High-risk patterns checked but not found |
| Limits | Missing tools, unreadable paths, or reduced coverage |

## Smart Parallelization Tips
- **Phase 1 (Acquire Target)**: Keep sequential — need unified scope
- **Phase 2-3 (Obliterate + Inventory)**: Parallelize across domains
  - Agent 1: Hunt CAPITAL OFFENSES (security, data loss, auth bypass)
  - Agent 2: Hunt FELONIES (god functions, any abuse, N+1 queries)
  - Agent 3: Hunt CRIMES + SLOP (magic numbers, AI hallucinations)
- **Phase 4-6 (Autopsy + Redemption)**: Keep sequential — needs unified prioritization
- Use the host's task tracker to track sins found per agent
- Each code-research worker should use `octocode-research` when installed; otherwise report reduced coverage and let the coordinator ask the user before installation.

## Example
- Goal: "Roast entire repo with 50+ files"
- Agent 1: Hunt security sins across all files.
- Agent 2: Hunt architectural sins, including large files and dependency spread.
- Agent 3: Hunt performance sins, including N+1 patterns and blocking calls.
- Merge: Combine into unified Hall of Shame, sort by severity

## Anti-patterns
- Don't parallelize small codebases (<10 files)
- Don't spawn agents for single-file roasts
- Don't parallelize redemption phase (fixes need sequential execution)
