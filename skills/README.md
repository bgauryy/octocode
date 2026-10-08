# Octocode skills

These Agent Skills guide research, design, writing, measurement, and tool use. Each folder has `SKILL.md` for agent activation and workflow, `output.md` for result formats, and a short `README.md` for people. The shared authoring standard is in [octocode-skills](octocode-skills/SKILL.md).

| Skill | Use when |
|---|---|
| [Research](octocode-research/) | A code, repository, or change-impact claim needs evidence. |
| [Scraping](octocode-scraping/) | Public pages need fetching or a reusable source corpus. |
| [Chrome DevTools](octocode-chrome-devtools/) | A live browser is needed for rendering, interaction, or diagnostics. |
| [Brainstorming](octocode-brainstorming/) | An open idea or claim needs independent research directions and current, checked evidence. |
| [Architect](octocode-architect/) | A software boundary or refactor needs an architecture decision. |
| [RFC Generator](octocode-rfc-generator/) | A consequential technical choice needs its decision, design, and plan in one RFC. |
| [Roast](octocode-roast/) | A blunt, evidence-backed code critique is requested. |
| [Clean Agentic Code](octocode-clean-agentic-code/) | Proven code or instruction redundancy should be removed. |
| [Documentation](octocode-documentation/) | Technical documentation needs writing, repair, or review. |
| [Agentic Prompts](octocode-agentic-prompts/) | Prompts or agent flows need a behavioral rewrite. |
| [Skills](octocode-skills/) | Agent Skills need creation, review, tuning, or installation. |
| [Eval Benchmark](octocode-eval-benchmark/) | A change needs a measured baseline and verdict. |
| [Agents Communication](octocode-agents-communication/) | Agents or sessions need shared messages, evidence, or handoffs. |

Tested skills live in [skills-beta](../skills-beta/). Repository development skills live in [skills-dev](../skills-dev/).

## Install and review

```bash
npx -y octocode skill list
npx -y octocode skill install octocode-research
node skills/octocode-skills/scripts/skill-review.mjs skills
```

The reviewer checks structure and local links. Test representative user requests to judge trigger quality; a structural pass cannot prove activation behavior.
