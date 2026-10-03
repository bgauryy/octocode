# References

Load when you audit where the orchestration and Ollama rules come from. Hosts named in this skill (Pi, Cursor, Claude) are examples only; map `references/coordinate.md` actions to the local spawn API.

## Orchestration

- LangChain multi-agent, subagents, handoffs, router, skills: portable topologies; right context per agent.
- LangGraph interrupts and Send fan-out: HITL gates; merge reducers.
- a2a-protocol.org specification: Agent Card, task lifecycle.
- OpenAI Agents SDK handoffs, agents-as-tools, orchestration: manager versus handoff; filtered context.
- arXiv:2503.13657 MAST: failure modes (design, misalignment, weak verification).
- arXiv:2305.14325 multi-agent debate: independent critics improve factuality.
- Anthropic multi-agent research (2025) and multiagent systems research: scout fan-out, citation pass, effort scaled to complexity; correlated consensus and conflicting-goal risks.
- Agent Skills specification (agentskills.io): progressive disclosure.
- Rubber duck, premortem, devil's advocate, self-consistency, FrugalGPT, RouteLLM: assumption surfacing, attack before commitment, majority voting, tier routing.

The former `octocode-orchestrator` contract lives here. Full KPI measurement stays in `octocode-eval-benchmark`.

## Local Ollama (merged from the former orchestrator-local-worker)

- athola/claude-night-market `qwen-delegation` and `delegation-core`: "delegate execution, retain reasoning" and the offload matrix. `gemini-delegation`: sibling, not used.
- unsigned-gg/agentic `local-model-triage`: serving failure modes (ctx, tools, quant) in `references/ollama-cli.md`.
- luongnv89/skills `ollama-optimizer`: hardware tier to model size, kept light in `references/model-selection.md`.
- tjboudreaux/cc-thinking-skills `thinking-model-selection`: classify-then-match only.
- Ollama setup skills (yoanbernabeu/grepai-skills, rawveg/skillsforge-marketplace, balloob/llm-skills): setup is not orchestrator/worker. shubhamsaboo/awesome-llm-apps `advisor-orchestrator-worker`: name overlap only.
