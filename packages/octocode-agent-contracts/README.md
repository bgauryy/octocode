# @octocodeai/agent-contracts

The shared contract layer of the Octocode agentic toolkit. It owns paths, local
SQLite control data, entities, permissions, discovery, Agent Skills, host
protocols, and prompt fragments used across agent integrations.

Import the narrow published subpath that owns a contract. Production packages
must not use the aggregate root as an internal convenience barrel.

Shared system, plan, and worker fragments live under `prompts`. Awareness owns
its standing cooperation policy, command schemas, and execution API separately;
Pi imports both owners. See [Awareness's API and prompt exports](../octocode-awareness/docs/API.md).

`BEHAVIORAL_PROMPT_GUIDANCE` supplies the shared authoring rule for generated tool,
skill, and MCP guidance: define the behavior, contrast nearby choices, explain the
consequence, state the principle, and end with an action. These are semantic parts,
not mandatory headings. Keep field constraints in their schemas and preserve exact
identifiers, authority, and output contracts. Shorten authored prose; do not truncate
published schemas to meet a character budget.

`PLAN_USAGE_GUIDANCE` is shared by the decision kernel and Pi's plan tool contract.
Plans are for complex dependencies, coordinated ownership, consequential risk,
substantial work spanning sessions, or explicit planning requests. Routine fixes,
a few straightforward steps, and isolated delegation need no plan. Independent
work is delegated only when it saves time or adds coverage,
and RFCs support consequential choices that need review. The host plan tool owns
its review interaction; prompts consume its decision without asking again. Context
summaries preserve pending approvals, failures, partial results and resume calls,
decisions, evidence, and the next action. Completion responses scale to the work
without fixed headings.

See [the architecture guide](ARCHITECTURE.md) for ownership and dependency
rules. Package architecture documents own host and core boundaries.
