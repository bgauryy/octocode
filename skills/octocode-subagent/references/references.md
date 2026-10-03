# References

Load when you audit where the orchestration and Ollama rules come from. Hosts named in this skill (Pi, Cursor, Claude) are examples. One primary source per topic.

| Topic | Primary source | Applied in |
|---|---|---|
| Topologies: subagents, handoffs, router | LangChain multi-agent docs | `references/orchestration-contract.md` |
| Fan-out, effort scaled to complexity, correlated consensus | Anthropic multi-agent research system (2025) | `references/spawn-gate.md`, `references/decompose.md` |
| Multi-agent failure modes, independent critics | arXiv:2503.13657 (MAST) | `references/challenge.md`, `references/coordinate.md` |
| Cross-host handoff, task lifecycle | a2a-protocol.org specification | `references/coordinate.md` |
| Progressive disclosure | Agent Skills specification (agentskills.io) | `references/decompose.md` |
| Tier routing for cost | FrugalGPT (arXiv:2305.05176) | `references/model-selection.md` |
| Local worker offload: delegate execution, retain reasoning | athola/claude-night-market `qwen-delegation` | `references/local-ollama.md` |
| Model tags and capabilities | ollama.com/library and `ollama show` | `references/model-selection.md` |
