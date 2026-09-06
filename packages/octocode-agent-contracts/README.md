# @octocodeai/agent-contracts

Shared host contracts for paths, local SQLite control data, entities,
permissions, discovery, Agent Skills, protocols, and prompt fragments.

Import the narrow published subpath that owns a contract. Production packages
must not use the aggregate root as an internal convenience barrel.

See [the architecture guide](ARCHITECTURE.md) for ownership and dependency
rules. Package architecture documents own host and core boundaries; program-level
completion gates live in [`DESIGN/LEFTOVERS.md`](../../DESIGN/LEFTOVERS.md).
