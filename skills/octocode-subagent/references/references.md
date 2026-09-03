# References

Research trail for `octocode-subagent`, including the merged orchestration contract. The skill is platform-independent; Pi, Cursor, and Claude are example hosts only.

## Specifications and research

| Source | Repository or venue | Score | Finding |
|---|---|---:|---|
| docs.langchain.com multi-agent / subagents / handoffs / router / skills | LangChain docs | — | portable topologies; right context per agent |
| LangGraph interrupts / Send fan-out | LangGraph docs | — | HITL gates; merge reducers |
| a2a-protocol.org specification | A2A specification | — | Agent Card, task lifecycle |
| OpenAI Agents SDK handoffs / agents-as-tools | OpenAI docs | — | ownership vs manager-as-tool |
| arXiv:2503.13657 MAST | arXiv | — | failure modes: design, misalignment, weak verification |
| arXiv:2305.14325 multi-agent debate | arXiv | — | independent critics improve factuality |
| Anthropic multi-agent research (2025) | Anthropic | — | scout fan-out, citation pass, scale effort to complexity |
| Anthropic multiagent systems research | Anthropic | — | correlated consensus and conflicting-goal risks |
| OpenAI Agents SDK orchestration | OpenAI docs | — | manager versus handoff, deterministic orchestration, filtered context |
| Agent Skills specification | agentskills.io | — | progressive disclosure and focused references |
| Rubber-duck debugging (classic) | general practice | — | restatement surfaces assumptions without new tools |
| Premortem / devil’s advocate (decision literature) | general research | — | attack the plan before commitment |
| Self-consistency (Wang et al. themes) | research literature | — | majority over independent samples |
| FrugalGPT / RouteLLM themes | research literature | — | model tier routing |

## Design choice
The design omits Pi-specific tool names so this skill installs on any host. Map `coordinate.md` actions to the local spawn API. The former `octocode-orchestrator` contract now lives here: framing, authority, budgets, TDD, evaluation routing, Awareness, and completion surround the delegation mechanics. Challenge techniques live here; full KPI measurement stays in `octocode-eval-benchmark`.

## Local Ollama offload (merged from former orchestrator-local-worker)

| Source | Repository | Score | Finding |
|---|---|---:|---|
| qwen-delegation | athola/claude-night-market | 115 | Closest “delegate execution, retain reasoning” worker skill; pattern borrowed, Qwen CLI not copied |
| delegation-core | athola/claude-night-market | 126 | Decision matrix / offload philosophy; adapted to Ollama allowlist |
| gemini-delegation | athola/claude-night-market | 107 | Sibling provider skill; confirmed multi-provider pack, not used as code |
| local-model-triage | unsigned-gg/agentic | 25 | Serving failure modes (ctx, tools, quant) → ollama-invoke.md; **different job** (harness triage ≠ offload) |
| ollama-optimizer | luongnv89/skills | 181 | Hardware tier → max model size heuristics; kept light in model-selection.md |
| thinking-model-selection | tjboudreaux/cc-thinking-skills | 121 | Inspected; mental-model skill — not LLM routing; classify-then-match borrowed only |
| ollama (various setup skills) | yoanbernabeu/grepai-skills, rawveg/skillsforge-marketplace, balloob/llm-skills (skills.sh), etc. | 26–719 | Confirmed marketplace gap: **setup ≠ orchestrator/worker** |
| advisor-orchestrator-worker | shubhamsaboo/awesome-llm-apps | 108 | Name overlap only; not Ollama sealed-packet offload — skipped as pattern source |

Merged into this skill as `references/local-ollama.md` + Ollama refs/scripts.
