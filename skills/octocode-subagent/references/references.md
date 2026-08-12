# References

Research trail for `octocode-subagent`. Host-agnostic skill; Pi/Cursor/Claude are example hosts only.

## Specs / Docs

| Source | Finding |
|--------|---------|
| docs.langchain.com multi-agent / subagents / handoffs / router / skills | portable topologies; right context per agent |
| LangGraph interrupts / Send fan-out | HITL gates; merge reducers |
| a2a-protocol.org specification | Agent Card, task lifecycle |
| OpenAI Agents SDK handoffs / agents-as-tools | ownership vs manager-as-tool |
| arXiv:2503.13657 MAST | failure modes: design, misalignment, weak verification |
| arXiv:2305.14325 multi-agent debate | independent critics improve factuality |
| Anthropic multi-agent research (2025) | scout fan-out, citation pass, scale effort to complexity |
| Rubber-duck debugging (classic) | restatement surfaces assumptions without new tools |
| Premortem / devil’s advocate (decision lit.) | attack the plan before commitment |
| Self-consistency (Wang et al. themes) | majority over independent samples |
| FrugalGPT / RouteLLM themes | model tier routing |

## Design choice
Pi-specific tool names were removed so this skill installs on any host. Map `coordinate.md` actions to the local spawn API. Challenge techniques (duck / interview / mimic) live in this skill; KPI measurement stays in `octocode-graph-eval`.

## Local Ollama offload (merged from former orchestrator-local-worker)

| Source | Finding |
|--------|---------|
| qwen-delegation | athola/claude-night-market | 115 | Closest “delegate execution, retain reasoning” worker skill; pattern borrowed, Qwen CLI not copied |
| delegation-core | athola/claude-night-market | 126 | Decision matrix / offload philosophy; adapted to Ollama allowlist |
| gemini-delegation | athola/claude-night-market | 107 | Sibling provider skill; confirmed multi-provider pack, not used as code |
| local-model-triage | unsigned-gg/agentic | 25 | Serving failure modes (ctx, tools, quant) → ollama-invoke.md; **different job** (harness triage ≠ offload) |
| ollama-optimizer | luongnv89/skills | 181 | Hardware tier → max model size heuristics; kept light in model-selection.md |
| thinking-model-selection | tjboudreaux/cc-thinking-skills | 121 | Inspected; mental-model skill — not LLM routing; classify-then-match borrowed only |
| ollama (various setup skills) | yoanbernabeu/grepai-skills, rawveg/skillsforge-marketplace, balloob/llm-skills (skills.sh), etc. | 26–719 | Confirmed marketplace gap: **setup ≠ orchestrator/worker** |
| advisor-orchestrator-worker | shubhamsaboo/awesome-llm-apps | 108 | Name overlap only; not Ollama sealed-packet offload — skipped as pattern source |

Merged into this skill as `references/local-ollama.md` + Ollama refs/scripts.
