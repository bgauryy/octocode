# References

Sources consulted to research and create this skill.

## GitHub Sources Inspected
<!-- Repos read for loop-design patterns (not SKILL.md files). -->
| File | Owner/Repo | Path | Quality | Notes |
|------|-----------|------|---------|-------|
| README | karpathy/autoresearch | README.md | High | Closed research loop: edit one artifact → fixed time budget → one honest metric → keep/discard → repeat; human edits the `program.md` "skill", agent edits the work; "loopy era". Sourced the autonomy/leash + budget-comparability framing. |
| README | assafelovic/gpt-researcher | README.md | High | Canonical deep-research loop: planner/executor split, generate questions → gather+source-track → aggregate; tree-like depth×breadth; maintains memory/context. Sourced full-research-loop shape. |

## Registry and Marketplace Surfaces
| Surface | URL or Query | Finding |
|---------|-------------|---------|
| Web (Loop Engineering guide) | tosea.ai/blog/loop-engineering-ai-agents-complete-guide-2026 | prompt→context→harness→loop engineering; 5-part loop anatomy; verification hierarchy (deterministic>judge>self-report); failure-mode table; 10 rules. Primary source for the core-5 upgrade. |
| Web (Anthropic) | anthropic.com/engineering/multi-agent-research-system | Orchestrator-worker, parallel subagents with isolated context windows. Sourced context-isolation guidance. |
| Web (Karpathy 2025) | autonomy slider / context engineering (search results) | Partial autonomy, human oversight, keep AI on a leash; context = just-right info for the next step. |

## Local Sources
| File | Path | Notes |
|------|------|-------|
| Agentic Auto-Scheduling (COMPILOT) | `docs/context/Agentic Auto-Scheduling/Agentic-Auto-Scheduling.md` | Core Act→Observe→Learn→Repeat closed-loop pattern; grounded-feedback beats open-loop (RQ6); delegate correctness to a verifying tool (RQ7); two-stage cheap-then-formal check; premature-stop mitigation; best-of-K to escape local optima. |
| LLMLOOP | `docs/context/LLMLOOP/LLMOOP.md` | Multiple chained feedback loops, each with a retry budget and a clean-or-budget stop; loop ordering (must-pass check first); re-running earlier loops when state changes. |
| self_harness | `docs/context/self_harness/self_harness.md` | Verify-before-conclude framing reinforced in the gates and code-check loop. |
| octocode-skills: agent-skills-guide | `skills/octocode-skills/references/agent-skills-guide.md` | Skill structure, progressive disclosure, validation-loop pattern, context discipline. |
| octocode-skills: skill-lint | `skills/octocode-skills/references/skill-lint.md` + `scripts/skill-lint.mjs` | Lint rules (≤100-line SKILL.md, ≤150-line refs, description style, link conditions); ran to validate. |
| octocode-engineer | `skills/octocode-engineer/SKILL.md` | House style: CLI-first transport, schema-first calls, evidence/anchor carrying, treat-snippets-as-leads. |
| octocode-local MCP instructions | session MCP server instructions | Tool names (`oqlSearch`, `ghSearchCode`, …), `status` semantics, remote-as-local bridge. |
