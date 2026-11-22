# Skill requests

Ordinary requests per public skill, for the checks in [SKILLS.md](../../SKILLS.md). Give a tester only the request
text. "Own" requests should activate the skill; "neighbour" requests should activate the named other skill (or none),
not this one. Add or reword requests freely; keep them in a user's voice, with no skill names and no steps.
Do not pool results across different request wordings.

| Skill | Own job | Neighbour (should go elsewhere) |
|---|---|---|
| octocode-research | How does request retry work in this repo, and who calls it? · Why did the cache TTL change last year? · Which version of a library fixed this bug? | The test is failing with a stack trace (→ debugging) |
| octocode-debugging | This service hangs after ~50 requests, find out why · A flaky test fails one run in ten · Memory grows steadily in this worker | Explain how the scheduler is structured (→ research) |
| octocode-engineer | Should this cache be write-through or write-back for our load? · Plan a refactor to split this module · Compare two data structures for this lookup | The build crashes on startup (→ debugging) |
| octocode-graph | Which files import this module? · What would break if this function changed? · Show me an interactive map of this repo. | Why did this function return the wrong value? (→ debugging) |
| octocode-pr-review | Review PR #123 · Review the pull request on this branch | Tear this code apart, no mercy (→ roast); review my local diff with no PR |
| octocode-roast | Roast this module · Brutally critique this design | Review PR #123 normally (→ pr-review) |
| octocode-clean-agentic-code | Remove dead code and unused exports from this package · Delete leftover debug logs without changing behavior | Refactor this module's architecture (→ engineer) |
| octocode-documentation | Rewrite this README for newcomers · Write a runbook for this deploy · Shorten this doc without losing facts | Write an RFC for the migration (→ rfc-generator) |
| octocode-rfc-generator | Write a design proposal for moving to a queue · Draft a migration plan with rollout | Write a README (→ documentation) |
| octocode-test-plan | Write a test plan for this feature, someone else will run it · Risk-rank test cases for this bug fix | Write the tests and run them (no skill: code task) |
| octocode-agentic | Why does my agent call the wrong tool? · Review this system prompt and tool contract · Design an eval for my agent | Coordinate three agents on a task (→ orchestrator) |
| octocode-orchestrator | Split this task across agents and track dependencies · Resume a stalled multi-agent run | Design the agent's prompt (→ agentic) |
| octocode-workflows | Design a multi-step process with retries and rollback · Resume this checkpointed pipeline | Assign work among agents live (→ orchestrator) |
| octocode-messages-proxy | Ask Codex what it thinks of this plan and bring back its answer | Split a task across several agents (→ orchestrator) |
| octocode-brainstorming | Give me alternatives to this approach, checked against sources · Challenge this proposal | Pick the data structure for this workload (→ engineer) |
| octocode-scraping | Extract the pricing table from this public page · Crawl these docs into a local folder | Click through a logged-in dashboard (→ chrome-devtools) |
| octocode-chrome-devtools | Log in and screenshot this page, show console errors · Record network calls for this flow | Pull text from a public static page (→ scraping) |
| octocode-get-started | Install Octocode in my editor · Set up GitHub login for the CLI | How does the code search tool work internally (→ research) |
| octocode-skills | Create a skill for X · Check this SKILL.md for trigger problems · Find a skill that does Y | Write a README (→ documentation) |

Add a row when a public skill is added; remove it when the skill is.
