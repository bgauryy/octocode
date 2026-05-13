# Agentic Flow — Reference Resources

Curated repos organized by skill section. Each entry maps to a specific concept in `SKILL.md`.

## Memory

- **[mem0ai/mem0](https://github.com/mem0ai/mem0)** — Universal cross-session memory layer; reference for the `MemoryService` pattern and short/long-term/episodic memory types.
- **[NirDiamant/Agent_Memory_Techniques](https://github.com/NirDiamant/Agent_Memory_Techniques)** — 30 notebooks (buffers, vector stores, knowledge graphs, episodic/semantic memory, Mem0, Zep, MemGPT); best map of the `memory` state kind.
- **[MemoriLabs/Memori](https://github.com/MemoriLabs/Memori)** — MCP-compatible agent-native memory infrastructure; shows how to wire `MemoryQuery`/`MemoryWrite` as a service separate from the runtime.

## RAG

- **[NirDiamant/RAG_Techniques](https://github.com/NirDiamant/RAG_Techniques)** — One notebook per advanced RAG pattern (HyDE, RAPTOR, self-RAG, corrective RAG); use as a `retrieve` node library.
- **[NirDiamant/Controllable-RAG-Agent](https://github.com/NirDiamant/Controllable-RAG-Agent)** — LangGraph graph-based RAG; reference for `classify → retrieve → reason → review` node wiring.
- **[GiovanniPasq/agentic-rag-for-dummies](https://github.com/GiovanniPasq/agentic-rag-for-dummies)** — Modular agentic RAG with LangGraph; clear reference for the delta-cache pattern (retrieve only changed docs).
- **[deepset-ai/haystack](https://github.com/deepset-ai/haystack)** — Production RAG pipelines with typed component contracts; maps to the `NodeInput`/`NodeOutput` protocol pattern.

## Agentic Flow Frameworks

- **[langchain-ai/langgraph](https://github.com/langchain-ai/langgraph)** — Stateful graph flows, checkpointers for durable execution, human-in-the-loop gates; maps 1:1 to the Flow Model layers.
- **[openai/openai-agents-python](https://github.com/openai/openai-agents-python)** — Lightweight multi-agent framework; reference for `AgentHandoff` protocol, tool-permission boundaries, and built-in tracing.
- **[microsoft/autogen](https://github.com/microsoft/autogen)** — Multi-agent conversation framework; best reference for nested delegation and human-in-the-loop gate patterns.
- **[a2aproject/A2A](https://github.com/a2aproject/A2A)** — Agent2Agent open protocol (Linux Foundation); reference spec for interoperable handoff envelopes across runtimes.
- **[FellouAI/eko](https://github.com/FellouAI/eko)** — TypeScript agentic workflow framework; reference for `classify → delegate → act → observe` in TypeScript codebases.

## Education & Tutorials

- **[NirDiamant/GenAI_Agents](https://github.com/NirDiamant/GenAI_Agents)** — 50+ notebooks covering single-agent to multi-agent patterns; use as a golden-trajectory library.
- **[NirDiamant/agents-towards-production](https://github.com/NirDiamant/agents-towards-production)** — Prototype → enterprise deployment path; covers observability, evals, and MLOps.
- **[microsoft/ai-agents-for-beginners](https://github.com/microsoft/ai-agents-for-beginners)** — 12-lesson course (AutoGen + Semantic Kernel + agentic RAG); lesson order matches the Operating Flow of this skill.
- **[dair-ai/Prompt-Engineering-Guide](https://github.com/dair-ai/Prompt-Engineering-Guide)** — Guides and notebooks for prompting, RAG, and agents; reference for §9 Prompt Quality patterns.
- **[NirDiamant/Prompt_Engineering](https://github.com/NirDiamant/Prompt_Engineering)** — 22 prompting technique notebooks; each maps to a §9 prompt-quality check.

## Cookbooks & Developer Hubs

- **[openai/openai-cookbook](https://github.com/openai/openai-cookbook)** — Official OpenAI examples for agents, RAG, function calling, and structured outputs.
- **[anthropics/claude-cookbooks](https://github.com/anthropics/claude-cookbooks)** — Official Anthropic notebooks for tool use, multi-step agents, and gate/human-approval flows.
- **[anthropics/courses](https://github.com/anthropics/courses)** — Anthropic's educational courses: tool use, multi-agent, advanced retrieval.
- **[GoogleCloudPlatform/generative-ai](https://github.com/GoogleCloudPlatform/generative-ai)** — Gemini + Vertex AI agent notebooks; reference for Google ADK session/memory wiring.
- **[oracle-devrel/oracle-ai-developer-hub](https://github.com/oracle-devrel/oracle-ai-developer-hub)** — Notebooks organized by capability (agents, RAG, memory) on Oracle AI + OCI.
- **[Shubhamsaboo/awesome-llm-apps](https://github.com/Shubhamsaboo/awesome-llm-apps)** — 100+ runnable agent and RAG apps; good source of golden-trajectory examples and eval baselines.
