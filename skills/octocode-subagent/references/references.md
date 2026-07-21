# References

Research trail for `octocode-subagent`. Host-agnostic skill; Pi/Cursor/Claude are example hosts only.

## Specs / Docs

| Source | Finding |
|--------|---------|
| docs.langchain.com multi-agent / subagents / handoffs / router / skills | portable topologies |
| LangGraph interrupts / Send fan-out | HITL gates; merge reducers |
| a2a-protocol.org specification | Agent Card, task lifecycle |
| OpenAI Agents SDK handoffs / agents-as-tools | ownership vs manager-as-tool |
| arXiv:2503.13657 MAST | multi-agent failure modes |
| DeepMind AGI→ASI / Levels of AGI | collectives yes; unbounded RSI no |
| FrugalGPT / RouteLLM themes | model tier routing |

## Design choice
Pi-specific tool names (`spawnSubagent`, `AgentMessage`, chrome gates) were removed from this skill so it installs on any host. Map `coordinate.md` actions to the local spawn API.
