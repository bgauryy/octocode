# Awareness documentation

The live CLI schema is the command reference. Run `npx @octocodeai/octocode-awareness schema commands --compact` for the complete routine inventory, or `schema command <concept> <operation> --compact` for one exact contract.

| Document | Purpose |
|---|---|
| [API](API.md) | Runtime exports, subpaths, results, and continuations |
| [How Awareness works](HOW_IT_WORKS.md) | Routine flow and trust boundaries |
| [Configuration](CONFIGURATION.md) | Workspace policy, storage scopes, and hook ownership |
| [Database](DB.md) | Canonical SQLite ownership and copy-on-write migration |
| [Entity links](ENTITY_LINKS.md) | Canonical entity families and relationships |
| [Host hooks](HOOKS.md) | Native-versus-shell lifecycle ownership |
| [LocalGit history](LOCAL_HISTORY.md) | Host-owned capture, inspection, and restore |
| [Exclusive path protection](LOCKS.md) | Exceptional protection protocol |
| [Efficient routine use](MEMORY_NAVIGATION.md) | Token budgets, bounded reads, and continuations |
| [Storage scopes](STORAGE_SCOPES.md) | Global, repository, and explicit database selection |
| [Verification](VERIFY.md) | Package and workflow checks |

Agent-facing operating procedures live in [`skills/octocode-awareness`](../skills/octocode-awareness/SKILL.md). The package [architecture](../ARCHITECTURE.md) defines layer ownership.
