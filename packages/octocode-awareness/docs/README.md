# Octocode Awareness Docs

Use this directory as the technical map for `@octocodeai/octocode-awareness`.

The short version: awareness gives local AI agents a shared memory and coordination layer. The SQLite database under the global Octocode home is canonical; the CLI is the control plane; hooks and the Pi bridge automate lifecycle edges; workspace `.octocode/` files are generated repo projections for agents and humans.

The agent-facing model is: **before** work, inspect repo status, other agents, memories, gotchas, handoffs, signals, and wiki context; **during** work, lock files, communicate, and record durable facts; **after** work, verify, reflect, refresh wiki context when useful, housekeep stale state, and improve skills/workflows when repeated patterns emerge.

## Mental Model

Context and tokens are the working circulation of an agent run: they move goals, constraints, evidence, warnings, and next actions to the place where decisions happen. Healthy awareness keeps that circulation fresh and bounded. Too little context starves the run; too much stale context clogs it.

Docs can become like excess weight. A giant `MEMORY.md` or chat-derived wiki may feel safe, but it makes every future run carry mass before it can think. Keep Markdown lean, move rows to CSV/HTML/query views, and use `attend --compact` plus `query workboard` as the active circulation.

Social exchange is part of the loop. Signals, refinements, handoffs, and user corrections bring new ideas and perspectives into the shared system. Keep them traceable and resolve or consolidate them so they improve the collective map instead of becoming more weight.

## Reading Paths

| You want to... | Start here |
|---|---|
| Understand the product and install it | [SKILLS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/SKILLS.md) |
| Understand the two `.octocode` locations | [DB.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/DB.md), [WIKI.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/WIKI.md) |
| Tell an agent how to install awareness | [SKILLS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/SKILLS.md#install-in-5-minutes) |
| Understand the full system flow | [HARNESS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/HARNESS.md) |
| Understand the active-memory-navigation prototype decision | [MEMORY_NAVIGATION.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/MEMORY_NAVIGATION.md) |
| Understand context/token circulation and bloat control | [WIKI.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/WIKI.md), [MEMORY_NAVIGATION.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/MEMORY_NAVIGATION.md) |
| Inspect exactly what is stored | [DB.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/DB.md) |
| Debug edit coordination or pending verification | [LOCKS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/LOCKS.md) |
| Understand memory-driven self-improvement | [REFLECTION.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/REFLECTION.md) |
| Generate or review workspace `.octocode/` repo context | [WIKI.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/WIKI.md) |
| Install or debug host hooks | [HOOKS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/HOOKS.md) |

## Feature Coverage

| Feature | Primary docs | Main commands / APIs |
|---|---|---|
| Workspace health | [HARNESS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/HARNESS.md), [DB.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/DB.md) | `workspace status` |
| Attend packet and workboard | [WIKI.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/WIKI.md), [MEMORY_NAVIGATION.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/MEMORY_NAVIGATION.md), package skill `SKILL.md` | `attend`, `query workboard` |
| Before/during/after agent workflow | [SKILLS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/SKILLS.md), package skill `SKILL.md` | `attend`, `workspace status`, `memory recall`, `lock acquire`, `signal publish`, `verify mark`, `reflect record`, `repo inject` |
| Global home vs repo projection | [DB.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/DB.md), [WIKI.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/WIKI.md), package README | `OCTOCODE_MEMORY_HOME`, `repo inject --out` |
| Database schema and storage | [DB.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/DB.md) | `maintenance init`, library DB helpers |
| Scope model | [DB.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/DB.md), [SKILLS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/SKILLS.md) | `--workspace`, `--artifact`, `--repo`, `--ref` |
| Agent registry | [DB.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/DB.md), [SKILLS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/SKILLS.md) | `agent register`, `agent list` |
| Memory recall | [SKILLS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/SKILLS.md), [DB.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/DB.md) | `memory recall` |
| Active memory navigation direction | [MEMORY_NAVIGATION.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/MEMORY_NAVIGATION.md), [HARNESS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/HARNESS.md) | Shipped: `attend`; future: deeper deterministic navigation |
| Memory recording and forgetting | [SKILLS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/SKILLS.md), [DB.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/DB.md) | `memory record`, `memory forget` |
| File locks | [LOCKS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/LOCKS.md), [DB.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/DB.md) | `lock acquire`, `lock wait`, `lock release`, `lock prune` |
| Verification gate | [LOCKS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/LOCKS.md), [HOOKS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/HOOKS.md) | `verify audit`, `verify mark`, `stop-verify.sh` |
| Signals / agent messaging | [SKILLS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/SKILLS.md), [DB.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/DB.md) | `signal publish`, `signal list`, `signal reply`, `signal ack`, `signal resolve`, `signal prune` |
| Refinements and handoffs | [SKILLS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/SKILLS.md), [DB.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/DB.md) | `refinement set`, `refinement get`, `refinement delete`, `session capture` |
| Reflection records | [REFLECTION.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/REFLECTION.md), [DB.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/DB.md) | `reflect record` |
| Weakness mining | [REFLECTION.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/REFLECTION.md) | `reflect mine-weakness` |
| Harness guidance export | [REFLECTION.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/REFLECTION.md), [HARNESS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/HARNESS.md) | `reflect export-harness` |
| Documentation staleness | [REFLECTION.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/REFLECTION.md), [DB.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/DB.md) | `docs staleness`, `insertEditLog()` |
| Query views | [WIKI.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/WIKI.md), [DB.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/DB.md) | `query <view>` |
| LLM Wiki projections | [WIKI.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/WIKI.md), [HARNESS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/HARNESS.md) | `repo inject` |
| Context health and projection bloat | [WIKI.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/WIKI.md), package README | `attend --compact`, `query workboard`, `repo inject`, `maintenance digest --dry-run` |
| Social perspective and handoffs | [SKILLS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/SKILLS.md), [DB.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/DB.md) | `signal publish`, `signal reply`, `refinement set`, `session capture` |
| Host hooks | [HOOKS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/HOOKS.md), [SKILLS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/SKILLS.md) | `hooks install`, `hooks check`, `hooks remove`, `hook run` |
| Smart briefing | [HOOKS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/HOOKS.md), [SKILLS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/SKILLS.md) | `notify-deliver.sh`, `UserPromptSubmit` |
| Harness guard | [HOOKS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/HOOKS.md), [REFLECTION.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/REFLECTION.md) | `harness-guard.sh`, `OCTOCODE_ALLOW_HARNESS_APPLY` |
| Pi bridge | [HOOKS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/HOOKS.md), [SKILLS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/SKILLS.md) | `wirePiAwarenessHooks(pi)` |
| Custom host/library integration | [DB.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/DB.md), [HOOKS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/HOOKS.md), package README | `@octocodeai/octocode-awareness` exports |
| Maintenance and cleanup | [SKILLS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/SKILLS.md), [DB.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/DB.md) | `maintenance digest`, `maintenance self-test` |
| Skill/workflow improvement | [REFLECTION.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/REFLECTION.md), [SKILLS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/SKILLS.md), Octocode guide | `reflect mine-weakness`, `reflect export-harness`, `npx octocode skill ...`, `octocode-skills` |
| Schema discovery | [SKILLS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/SKILLS.md), [HARNESS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/HARNESS.md) | `schema commands`, `schema list`, `schema json-schema`, `schema example`, `schema validate` |
| Agent Skill install and bundled scripts | [SKILLS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/SKILLS.md), package README | `npx octocode skill --add --path {{path_to_skills_location}}/octocode-awareness --platform common`, `node scripts/awareness.mjs` |
| Easy agent install | [SKILLS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/SKILLS.md), package README | `npx @octocodeai/octocode-awareness`, `npx octocode skill --add --path {{path_to_skills_location}}/octocode-awareness --platform common` |

## Feature Coverage Check

Every public command group from `octocode-awareness schema commands --compact` is represented above. If a new command group is added, update this table in the same change as the command and add deeper detail to one of the subsystem docs.

## Maintenance Rule

Keep [HARNESS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/HARNESS.md) as the map. Put canonical details in the focused docs:

- database storage details in [DB.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/DB.md),
- active-memory-navigation tradeoffs and prototype scope in [MEMORY_NAVIGATION.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/MEMORY_NAVIGATION.md),
- lock/verification behavior in [LOCKS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/LOCKS.md),
- reflection and self-improvement in [REFLECTION.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/REFLECTION.md),
- generated repo context in [WIKI.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/WIKI.md),
- host lifecycle behavior in [HOOKS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/HOOKS.md),
- user recipes and CLI walkthroughs in [SKILLS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/SKILLS.md).

Keep the docs set in shape: add a new page only when it owns a stable subsystem. For row-heavy or fast-changing state, prefer `query`, CSV, HTML, manifest metadata, and compact references over another long Markdown narrative.
