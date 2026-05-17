# Agentic Flow Research Launchpad

Use this as seed material, not a fixed stack. Project-local conventions win. Start with official docs, then inspect GitHub/source when behavior affects design.

Use available source-search tooling first, such as Octocode local/GitHub search. If shell search is appropriate, a useful local query is:

```bash
rg -n "MCP|OpenAI|ADK|LangGraph|context|memory|cache|handoff|eval|attention|schema|hooks|github" references/resources.md
```

Research loop:

1. Open the relevant docs URL for concepts and terminology when external docs are allowed.
2. Search the GitHub/source repo for implementation details, examples, tests, and issues using Octocode or the available source-search tool.
3. Search within source for `tool`, `handoff`, `memory`, `session`, `callback`, `hook`, `trace`, `schema`, `eval`, `checkpoint`, `middleware`.
4. Carry forward findings that affect architecture, protocol, context, safety, or verification.
5. If the project uses another framework, search its official docs and GitHub source the same way.

## Core Patterns

- **Anthropic - Building Effective Agents**: https://www.anthropic.com/engineering/building-effective-agents
  Patterns: augmented LLM, chains, routing, parallelization, orchestrator-workers, evaluator-optimizer, agents, tool design.

## Protocols And MCP

- **MCP Architecture**: https://modelcontextprotocol.io/docs/learn/architecture
  Scope, participants, layers, primitives; MCP is context/tool protocol, not orchestration.
- **MCP Server Concepts**: https://modelcontextprotocol.io/docs/learn/server-concepts
  Tools, resources, prompts, discovery.
- **MCP Prompts**: https://modelcontextprotocol.io/docs/concepts/prompts
  Reusable prompt templates.
- **MCP GitHub**: https://github.com/modelcontextprotocol/modelcontextprotocol
  Search source/spec for protocol behavior and examples.

## Agent SDKs And Runtimes

- **OpenAI Agents docs**: https://openai.github.io/openai-agents-python/agents/
  Agents, tools, handoffs, guardrails, sessions, structured outputs.
- **OpenAI orchestration**: https://openai.github.io/openai-agents-python/multi_agent/
  LLM-led vs code-led, agents-as-tools, handoffs, hybrids.
- **OpenAI lifecycle**: https://openai.github.io/openai-agents-python/ref/lifecycle/
  RunHooks and AgentHooks around models, tools, agents, handoffs.
- **OpenAI Agents GitHub**: https://github.com/openai/openai-agents-python
  Search examples/tests/source for handoff, lifecycle, tracing, tool behavior.

- **Google ADK sessions/state/memory**: https://adk.dev/sessions/
  Session, state, searchable memory.
- **Google ADK callbacks**: https://google.github.io/adk-docs/callbacks/
  Agent/model/tool lifecycle callbacks.
- **Google ADK GitHub**: https://github.com/google/adk-python
  Search source/examples for runners, tools, callbacks, memory services.

- **LangGraph overview**: https://docs.langchain.com/oss/python/langgraph
  Durable stateful agents, persistence, streaming, human-in-loop, memory, tracing.
- **LangGraph persistence**: https://docs.langchain.com/oss/python/langgraph/persistence
  Threads, checkpoints, snapshots, cross-thread memory.
- **LangGraph GitHub**: https://github.com/langchain-ai/langgraph
  Search source/examples for checkpoints, interrupts, commands, stores, middleware.

- **LangChain docs**: https://docs.langchain.com/
  Model/tool abstractions, middleware, agents, retrieval, eval-related guides.
- **LangChain GitHub**: https://github.com/langchain-ai/langchain
  Search source/examples for tools, structured output, middleware, retrievers.

## Schemas And Structured Output

- **Zod JSON Schema**: https://zod.dev/json-schema
  Convert Zod to JSON Schema for model/tool/MCP structured-output APIs.
- **Zod GitHub**: https://github.com/colinhacks/zod
  Search issues/source for JSON Schema, strict objects, discriminated unions.
- **JSON Schema**: https://json-schema.org/
  Cross-language schema interoperability.

## Models, Prompts, Cache

- **OpenAI prompt caching**: https://platform.openai.com/docs/guides/prompt-caching
  Static-before-dynamic prompts, cached tokens.
- **OpenAI prompting**: https://platform.openai.com/docs/guides/prompting
  Prompt versions, variables, eval-linked iteration.
- **OpenAI reasoning models**: https://platform.openai.com/docs/guides/reasoning
  Reasoning tokens, effort, output budget, incomplete responses.
- **OpenAI model comparison**: https://platform.openai.com/docs/models/compare
  Current context windows, output limits, feature fit.

## Context Attention

- **Deep Agents context engineering**: https://docs.langchain.com/oss/javascript/deepagents/context-engineering
  Input/runtime context, compression, offloading, summarization, context isolation, skills.
- **Lost in the Middle**: https://arxiv.org/abs/2307.03172
  Long-context models can miss middle-position information.
- **Chroma Context Rot**: https://www.trychroma.com/research/context-rot
  Adding tokens can degrade usefulness.

## Memory And RAG Baselines

- **LangGraph memory**: https://docs.langchain.com/oss/python/concepts/memory
  Short-term/thread and long-term memory.
- **Deep Agents memory**: https://docs.langchain.com/oss/python/deepagents/long-term-memory
  Agent/user/org memory, read-only vs writable, background consolidation.
- **mem0 GitHub**: https://github.com/mem0ai/mem0
  Cross-session memory patterns.
- **RAG techniques GitHub**: https://github.com/NirDiamant/RAG_Techniques
  Retrieval pattern examples and eval baselines.

## Collaboration And Interop

- **AutoGen GitHub**: https://github.com/microsoft/autogen
  Multi-agent conversation and human-in-loop examples.
- **A2A GitHub**: https://github.com/a2aproject/A2A
  Agent-to-agent interop protocol ideas.
- **OpenAI Cookbook GitHub**: https://github.com/openai/openai-cookbook
  Practical examples for tools, structured outputs, retrieval, agents.
