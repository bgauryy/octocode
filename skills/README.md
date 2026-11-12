# Octocode skills

These published Agent Skills guide research, design, writing, measurement, tool use, and agent-workflow execution. Start at the linked skill. Multi-file skills have an output guide. A separate README serves readers who need setup, examples or operating instructions. The shared authoring standard is in [octocode-skills](octocode-skills/SKILL.md).

Saved skill output lives under the project's `.octocode/` folder: reports and authored artifacts (research reports, RFCs, briefs, reviews, eval runs) in `<workspace>/.octocode/<skill-name>/`, and runtime captures and scratch files in its `tmp/` subfolder, so every file a skill writes stays under its own folder. A path the user names takes precedence; edits to the project's own code, docs, prompts, or skills stay at their source paths.

| Need | Owner / behavior |
|---|---|
| Report format, save trigger and destination | Each skill's `output.md` |
| HTML report | Bundled `scripts/render-report.mjs`, no npm dependencies; Graph View has its own renderer. Generate only when requested or accepted; `--open` only when asked. |
| Human reading | One report, folder or `.octocode/` collection becomes a local page with navigation, search, contents, sources, editor/doc links, Mermaid, light/dark and print styles; no automatic metadata card. |
| Agent discovery | `--manifest`: query-filtered pages of 20 with continuations. `--all`: full catalog for file processing. |
| Visual verification | `--json --strict` rejects validation warnings; generation remains browser-unverified. Run Render check and inspect readability. |
| Shared guidance and templates | [Common package](common/README.md); edit canonical sources, regenerate consumers, check with `node skills/common/build.mjs --check`. |

| Skill | Use when |
|---|---|
| [Get started](octocode-get-started/) | Understand Octocode, install CLI/MCP and skills, configure scope and credentials, and verify a first task. |
| [Research](octocode-research/SKILL.md) | A factual code, repository, dependency or change-impact question needs source evidence. |
| [Debugging](octocode-debugging/SKILL.md) | Observe a runtime failure, hang, leak or slowdown with debuggers, profilers and traces, interpret an existing traceback, dump, profile or trace, and verify a fix. |
| [PR Review](octocode-pr-review/SKILL.md) | A pull request, branch, or local diff needs a merge verdict with proven findings. |
| [Test Plan](octocode-test-plan/SKILL.md) | A feature, change, PR or bug fix needs a test plan another agent will run: mapped code, runnable cases with oracles, and success criteria. |
| [Messages proxy](octocode-messages-proxy/SKILL.md) | Use Claude and Codex native messaging, coordinate mixed-vendor peers through Octocode, or relay to A2A agents with a small model. |
| [Workflows](octocode-workflows/) | Select, design or resume an explicit workflow across agents, tools, services or artifacts; define handoffs and acceptance. |
| [Scraping](octocode-scraping/) | Public pages need fetching or a reusable source corpus. |
| [Chrome DevTools](octocode-chrome-devtools/) | A live browser is needed for rendering, interaction, or diagnostics. |
| [Brainstorming](octocode-brainstorming/) | An open problem, idea or comparison needs checked sources and an evidence-based recommendation. |
| [Graph](octocode-graph/SKILL.md) | Build a code graph and read its hubs, importers, callers, cycles, issues, impact or Git co-change, then prove the rows. |
| [Graph View](octocode-graph-view/) | Map repository structure, retain graph evidence, and optionally render an interactive viewer. |
| [Engineer](octocode-engineer/) | A technical choice or architecture needs trade-offs, failure modes and a check: data structures, storage, concurrency, memory, performance, reliability, APIs, security, testing, boundaries or refactors. |
| [RFC Generator](octocode-rfc-generator/SKILL.md) | A consequential technical choice needs its decision, design, and plan in one RFC. |
| [Roast](octocode-roast/) | A blunt, evidence-backed code critique is requested. |
| [Clean Agentic Code](octocode-clean-agentic-code/SKILL.md) | Proven code or instruction redundancy should be removed. |
| [Documentation](octocode-documentation/SKILL.md) | Technical documentation needs writing, repair, or review. |
| [Agentic](octocode-agentic/SKILL.md) | An agent, its prompts or its tooling needs design, review, debugging or measurement: context, memory, tool and MCP contracts, routing, delegation, loop control, security, prompt rewrites and conversation audits, evals and skill activation, agent-to-agent messages and local agent coordination. |
| [Skills](octocode-skills/SKILL.md) | Find, create, improve, check, install or sync Agent Skills; tune descriptions and reference routes. |

These are public skills. Repository-only development helpers live in [skills-dev](../skills-dev/) and are not part of the public selection catalog. A skill whose frontmatter sets `metadata.excluded-hosts` (space-separated host IDs) stays installable by the octocode CLI but is skipped by a listed host that honors it; Get started names `octocode-code`, whose own setup it would contradict.

## Install and review

```bash
npx -y octocode skill list
npx -y octocode skill install octocode-research
node skills/octocode-skills/scripts/skill-review.mjs --collections skills skills-dev
```

The reviewer checks structure and local links. Test representative user requests to judge trigger quality; a structural pass cannot prove activation behavior.
