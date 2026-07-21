# References Audit Trail

Sources consulted for `octocode-eval`. Not loaded at runtime unless linked from the lobby.

## GitHub Sources Inspected

| File | Owner/Repo | Path | Quality | Notes |
|------|-----------|------|---------|-------|
| README + program.md | karpathy/autoresearch | README.md, program.md | strong | keep/discard loop |
| README | karpathy/llm-council | README.md | strong | multi-model eval |

## Web / Articles

| Surface | URL | Finding |
|---------|-----|---------|
| Karpathy | https://karpathy.bearblog.dev/year-in-review-2025/ | RLVR, bench distrust |
| Karpathy | https://medium.com/@karpathy/software-2-0-a64152b37c35 | eval criterion as program |
| Anthropic | https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents | graders, pass@k, capability vs regression |
| BinEval | https://arxiv.org/html/2606.27226v1 | atomic yes/no questions |
| Hamel | https://hamel.dev/blog/posts/evals-faq/why-is-error-analysis-so-important-in-llm-evals-and-how-is-it-performed.html | open/axial coding → taxonomy |
| OpenAI | https://openai.com/index/why-we-no-longer-evaluate-swe-bench-verified/ | contamination / mismeasurement |
| Wolfe | https://cameronrwolfe.substack.com/p/llm-bench | anatomy of LLM benchmarks |

## Local Sources

| File | Path | Notes |
|------|------|-------|
| Thesis | Awareness / harness docs when present | `Agent = Model + Harness`; goal→KPI sensors (path may live outside this monorepo) |
| Improve loop | skills/*/references/improve-loop.md | stubs route to this skill’s owner copy |
| Eval harnesses | skills/octocode-research\|brainstorming\|rfc-generator\|orchestrator-local-worker | cases + binaryQuestions / live grades |
| KPI template | skills/octocode-rfc-generator/references/rfc-kpi.md | leading/lagging pattern |
| Synthesis (2026) | https://www.aroy.sh/posts/llm-agent-evals/ | deterministic checks as agent unit tests; binary over Likert |
