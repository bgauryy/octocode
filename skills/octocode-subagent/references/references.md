# References

Research trail for `octocode-subagent` (deep pass). Sources actually consulted.

## Local Sources

| File | Path | Notes |
|------|------|-------|
| Agents prompt | `packages/octocode-pi-extension/src/prompts/sections/agents.md` | gate + packets |
| spawnAgent / AgentMessage | `…/src/tools/agent-tools.ts` | lean default, no skills param, wait=idle |
| spawnSubagent | `…/src/tools/spawn-subagent-tool.ts` | chrome gate, typed skills |
| Registry | `…/src/subagents.ts` | four specialists + OCTOCODE_SKILL_NAMES |
| Specialist prompts | `…/subagents/*/SYSTEM_PROMPT.md` | per-role prefixes |
| Agent communication | `skills/octocode-prompt-optimizer/references/agent-communication.md` | A2A vs MCP vs handoff |

**Non-SoT:** `packages/octocode-pi-extension/docs/TOOLS.md` may drift — prefer source above.

## Specs / Docs

| Source | Finding |
|--------|---------|
| docs.langchain.com multi-agent / subagents / handoffs / router / skills | pattern cost matrix; supervisor≠router |
| LangGraph interrupts / Send fan-out | HITL gates; merge reducers |
| a2a-protocol.org specification + life-of-a-task | terminal immutable; auth-required |
| OpenAI Agents SDK handoffs / agents-as-tools | ownership transfer vs manager-as-tool |
| arXiv:2503.13657 MAST | multi-agent failure modes |
| DeepMind AGI→ASI / Levels of AGI | collectives yes; unbounded RSI no |
| arXiv self-evolving agents surveys | bounded improve / verifier-critic |

## Subagent research runs
LangGraph techniques · A2A/ASI · Pi harness audit · critical edit plan — reconciled into synthesize/browser + Pi API corrections.
