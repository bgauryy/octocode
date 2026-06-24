# Similar Systems

Read this only when redesigning `octocode-awareness` or comparing it with other agent memory systems.

## Design Signals

- Agentica uses a local `EXPERIENCE.md` card index plus append-only events and generated skills. Signal: keep a readable experience layer separate from generated/retrievable machinery. Source: https://github.com/shibing624/agentica
- Basic Memory skills teach agents when and how to use a memory MCP server, including note/task/schema/reflection workflows. Signal: the skill should teach protocol use, not bury all behavior inside memory storage. Source: https://docs.basicmemory.com/integrations/skills
- OpenMemory MCP exposes explicit memory tools such as add/search/list/delete and emphasizes local, shared memory across MCP clients. Signal: use small, obvious verbs and keep user-owned local storage. Source: https://mem0.ai/blog/introducing-openmemory-mcp
- Memento pairs SQLite persistence, FTS5 keyword search, sqlite-vec, and local embeddings. Signal: SQLite plus FTS is a good v1 base; vector search can stay optional until it earns the dependency. Source: https://mcpmarket.com/server/memento-1
- Agent-Skills-for-Context-Engineering includes a `memory-systems` skill whose description routes persistent semantic memory, entity tracking, temporal validity, graph/vector retrieval, and consolidation. Signal: keep this skill narrower and operational: durable experience, schemas, recall, and file coordination. Source: https://github.com/muratcankoylan/Agent-Skills-for-Context-Engineering/blob/main/skills/memory-systems/SKILL.md
- The awesome-agent-skills catalog lists NVIDIA NeMo-RL `session-memory` as durable working-session memory for coding agents. Signal: coding-agent memory is a recognized skill category; trigger wording should mention long-running work and handoffs. Source: https://github.com/VoltAgent/awesome-agent-skills

## Current Position

`octocode-awareness` is intentionally simpler than a full memory server:

- One SQLite store with column-scoped records: global memories, workspace handoffs, repo-channel notifications, verification intents, and cross-process file locks via `scripts/awareness.py`.
- `scripts/schema.mjs` for Zod-validated protocol payloads; `scripts/show-memories.py` for an HTML viewer.
- No vector dependency in v1.
- No background daemon or Docker requirement.
