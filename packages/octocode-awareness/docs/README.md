# Octocode Awareness Docs

Use this directory as the technical map for `@octocodeai/octocode-awareness`.

The short version: awareness gives local AI agents a shared memory and coordination layer. The SQLite database under the global Octocode home is canonical; the CLI is the control plane; hooks and the Pi bridge automate lifecycle edges; workspace `.octocode/` files are generated repo projections for agents and humans.

## Reading Paths

| You want to... | Start here |
|---|---|
| Understand the product and install it | [SKILLS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/SKILLS.md) |
| Understand the two `.octocode` locations | [DB.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/DB.md), [WIKI.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/WIKI.md) |
| Tell an agent how to install awareness | [SKILLS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/SKILLS.md#install-in-5-minutes) |
| Understand the full system flow | [HARNESS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/HARNESS.md) |
| Understand the active-memory-navigation prototype decision | [MEMORY_NAVIGATION.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/MEMORY_NAVIGATION.md) |
| Inspect exactly what is stored | [DB.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/DB.md) |
| Debug edit coordination or pending verification | [LOCKS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/LOCKS.md) |
| Understand memory-driven self-improvement | [REFLECTION.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/REFLECTION.md) |
| Generate or review workspace `.octocode/` repo context | [WIKI.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/WIKI.md) |
| Install or debug host hooks | [HOOKS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/HOOKS.md) |

## Feature Coverage

| Feature | Primary docs | Main commands / APIs |
|---|---|---|
| Workspace health | [HARNESS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/HARNESS.md), [DB.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/DB.md) | `workspace status` |
| Global home vs repo projection | [DB.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/DB.md), [WIKI.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/WIKI.md), package README | `OCTOCODE_MEMORY_HOME`, `repo inject --out` |
| Database schema and storage | [DB.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/DB.md) | `maintenance init`, library DB helpers |
| Scope model | [DB.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/DB.md), [SKILLS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/SKILLS.md) | `--workspace`, `--artifact`, `--repo`, `--ref` |
| Agent registry | [DB.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/DB.md), [SKILLS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/SKILLS.md) | `agent register`, `agent list` |
| Memory recall | [SKILLS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/SKILLS.md), [DB.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/DB.md) | `memory recall` |
| Active memory navigation prototype | [MEMORY_NAVIGATION.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/MEMORY_NAVIGATION.md), [HARNESS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/HARNESS.md) | Not shipped yet; proposed `memory navigate` |
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
| Host hooks | [HOOKS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/HOOKS.md), [SKILLS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/SKILLS.md) | `hooks install`, `hooks check`, `hooks remove`, `hook run` |
| Smart briefing | [HOOKS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/HOOKS.md), [SKILLS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/SKILLS.md) | `notify-deliver.sh`, `UserPromptSubmit` |
| Harness guard | [HOOKS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/HOOKS.md), [REFLECTION.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/REFLECTION.md) | `harness-guard.sh`, `OCTOCODE_ALLOW_HARNESS_APPLY` |
| Pi bridge | [HOOKS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/HOOKS.md), [SKILLS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/SKILLS.md) | `wirePiAwarenessHooks(pi)` |
| Custom host/library integration | [DB.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/DB.md), [HOOKS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/HOOKS.md), package README | `@octocodeai/octocode-awareness` exports |
| Maintenance and cleanup | [SKILLS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/SKILLS.md), [DB.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/DB.md) | `maintenance digest`, `maintenance self-test` |
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
