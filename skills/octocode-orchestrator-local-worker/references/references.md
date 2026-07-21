# References

Sources consulted to research, create, and validate this skill.

## Skills.sh API Results
| Skill | Source | Installs | Used for |
|-------|--------|----------|----------|
| qwen-delegation | athola/claude-night-market | 115 | Closest “delegate execution, retain reasoning” worker skill; pattern borrowed, Qwen CLI not copied |
| delegation-core | athola/claude-night-market | 126 | Decision matrix / offload philosophy; adapted to Ollama allowlist |
| gemini-delegation | athola/claude-night-market | 107 | Sibling provider skill; confirmed multi-provider pack, not used as code |
| local-model-triage | unsigned-gg/agentic | 25 | Serving failure modes (ctx, tools, quant) → ollama-invoke.md; **different job** (harness triage ≠ offload) |
| ollama-optimizer | luongnv89/skills | 181 | Hardware tier → max model size heuristics; kept light in model-selection.md |
| thinking-model-selection | tjboudreaux/cc-thinking-skills | 121 | Inspected; mental-model skill — not LLM routing; classify-then-match borrowed only |
| ollama (various setup skills) | yoanbernabeu/grepai-skills, rawveg/skillsforge-marketplace, balloob/llm-skills (skills.sh), etc. | 26–719 | Confirmed marketplace gap: **setup ≠ orchestrator/worker** |
| advisor-orchestrator-worker | shubhamsaboo/awesome-llm-apps | 108 | Name overlap only; not Ollama sealed-packet offload — skipped as pattern source |

## GitHub Sources Inspected
| File | Owner/Repo | Path | Quality | Notes |
|------|-----------|------|---------|-------|
| delegation-core | athola/claude-night-market | plugins/conjure/skills/delegation-core/SKILL.md | High | Philosophy + complexity/volume matrix adapted |
| qwen-delegation | athola/claude-night-market | plugins/conjure/skills/qwen-delegation/SKILL.md | High | Worker invoke/save pattern; not copied wholesale |
| token-conservation | athola/claude-night-market | plugins/conserve/skills/token-conservation/SKILL.md | Medium | Delegation-check step as token-budget trigger |
| local-model-triage | unsigned-gg/agentic | packages/skills/local-model-triage/SKILL.md | High | Local harness fault domains (serving → tier → wiring) |
| hermes-mcp README | mlennie/hermes-mcp | README.md | Medium | Cloud MCP → full local Hermes agent (tools/cron/browser); **heavier than this skill** |
| cascadeflow README | lemony-ai/cascadeflow | README.md | High | In-loop cascade: small first → quality validate → escalate; harness not skill |
| llm-use README | llm-use/llm-use | README.md | Medium | Planner + workers + Ollama toolkit; not skill-shaped |
| kodama-summariser README | senthamizharasim/kodama-summariser | README.md | Medium | **Map-reduce webpage summarize** prior art; runtime is **Groq** (not Ollama) — do not cite as Ollama |

## Papers / formal articles
| Source | URL | Finding |
|--------|-----|---------|
| FrugalGPT (Chen, Zaharia, Zou) | https://arxiv.org/abs/2305.05176 | LLM cascade + reliability scoring; cheap models first, escalate when score fails — validates VERIFY→cascade |
| Cascade routing (Dekoninck et al.) | https://arxiv.org/html/2410.10347v2 | Routing ∪ cascading; quality estimators critical |
| Multi-LLM routing survey | https://arxiv.org/html/2506.06579 | Routing vs hierarchical cascade for efficient inference |
| Ollama Thinking | https://ollama.com/blog/thinking | Official `--think` / `--think=false` / API `think` — bulk default off |

## Registry and Marketplace Surfaces
| Surface | URL or Query | Finding |
|---------|-------------|---------|
| skills.sh API | q=ollama orchestrator worker delegation; q=local model delegate (2026-07-20) | Still no strong Ollama sealed-packet orchestrator/worker skill; hits are setup, triage, or unrelated orchestrator names |
| Serper | Claude Code delegate local Ollama worker dual agent | Community dual-runtime experiments; weak forum evidence |
| Exa | orchestrator worker local LLM (github); FrugalGPT domain-filtered | Cascade / multi-agent repos + papers; not skill-shaped |
| Ollama library | https://ollama.com/library/gemma4 | Gemma 4 tags, sampling, thinking, multimodal → family-playbooks.md |
| Ollama library | https://ollama.com/library/qwen2.5 | Structured JSON / instruction-following → family prefs |
| Ollama catalog research | library + tools/thinking + `ollama show` (2026-07-20) | Developer matrix → `ollama-local-models.md` |

## Local Sources
| File | Path | Notes |
|------|------|-------|
| octocode-subagent | /Users/guybary/.claude/skills/octocode-subagent/SKILL.md | Spawn/synthesize; this skill complements, does not replace |
| model-routing | /Users/guybary/.claude/skills/octocode-subagent/references/model-routing.md | Host catalog tiers vs local Ollama offload; smallest-capable |
| agentic-flow-best-practices | /Users/guybary/.cursor/skills/agentic-flow-best-practices/SKILL.md | Orchestrator-workers / model router framing |
| agent-skills-guide | /Users/guybary/.cursor/skills/octocode-search-skill/references/agent-skills-guide.md | Progressive disclosure, gates, description hygiene |
| live ollama CLI | `ollama list` / `show` / `ps` | Exact-match routing; warm-model preference |
| usage-matrix | `references/usage-matrix.md` | Surface when/how |
| article dogfood | `evals/fixtures/article-*.txt`, `.octocode/worker/dogfood*` | Fetch≠worker; quote substring verify; cascade on partial grounding |
| serving knobs (2026-07-21) | Ollama API `keep_alive` / `format` / `options.num_ctx` + Thinking blog | Default `--keepalive 5m`; `--temperature`/`--num-ctx` via HTTP; silent truncation risk |
| validation research (2026-07-20) | `.octocode/worker/validate-research/` | Re-checked code+articles; Thinking blog grounded_rate 3/3; kodama provenance corrected |
| Wikipedia LLM | https://en.wikipedia.org/wiki/Large_language_model | Held-out article body for grounded summarize |
| web-summarizer-ai | vishruth-tech/web-summarizer-ai | Map-reduce oversized webpage lead (inspect before citing runtime) |
| Ollama webpage summarizers (many) | e.g. glance-cli, chrome extensions, small Python scripts | Confirm **fetch-then-summarize** pattern is common; almost never skill-shaped with verify gates |
