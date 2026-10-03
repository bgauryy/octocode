# Communication runtime

The installed skill contains `SKILL.md` and `scripts/`. CLI and MCP use the same Python package and SQLite store; Node is needed only for the JavaScript host adapters and maintainer checks.

## Request flow

`agents-communication` (or the PowerShell launcher) starts `communication.py` → `communication/cli.py`. `catalog.json` supplies command schemas and tool profiles; `catalog.py` resolves them and `validation.py` validates inputs. MCP binds calls to an identity and delegates to the same CLI dispatcher.

`store.py` coordinates identity, messages and completion. Focused modules implement peers, leases, documents, activity, entity views, health and retention. `schema.sql` owns database identity, tables, constraints and relationships; `database.py` caches its metadata and validates opened stores against it. The catalog derives entity discovery and relationships from these sources. Paths use frozen Unicode case folding so lease identity is consistent across Python versions.

`dispatch.py` owns durable delivery state; `transport.py` implements native host submission. `proxy.py` supervises requested workers. `host_hooks.py`, the Pi modules and `hooks/` integrate host events and structured-edit admission. Delivery submission and recipient completion remain separate operations.

`view.py` serves the local dashboard; `view_data.py` reads its bounded database views. Dashboard assets live beside the Python modules and require no frontend build.

## Sources and verification

- [SKILL.md](SKILL.md): worker workflow, also injected unchanged without frontmatter by managed workers.
- [Host setup](scripts/docs/HOST_SETUP.md): lifecycle, delivery, profiles and administrative discovery, loaded on demand.
- [Database protocol](scripts/docs/DB.md): served directly by `db protocol`.
- [Service protocol](scripts/docs/SERVICE_PROTOCOL.md), [hooks](scripts/docs/HOST_HOOKS.md) and [edit guards](scripts/docs/HOST_LEASE_GUARDS.md): host integration contracts.
- [Operations, recovery and retention](scripts/docs/OPERATIONS.md): health, recovery and storage procedures; lease rules live in [DB.md](scripts/docs/DB.md#path-leases).

Edit these sources in place. The only copied runtime module is `scripts/octocode_config.py`, refreshed by `src/build-skill.mjs` from the monorepo config package. `src/pack-skill.mjs` packages and checks the extracted bundle. `yarn verify` runs syntax, links, regression tests and the extracted CLI/MCP recovery smoke test.
