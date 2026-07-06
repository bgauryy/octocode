# Octocode Skills

`octocode-skills` gives an agent the ability to work on Agent Skills as first-class artifacts. It can find, inspect, evaluate, improve, install, author, and explain `SKILL.md` folders without treating them like ordinary markdown.

Use it whenever the task is about a skill's trigger, workflow, references, README, installation target, publication quality, or marketplace fit.

## The Problem

Skills are small, but they are operational. A weak description can trigger at the wrong time. A bloated `SKILL.md` can waste context. A missing gate can make install behavior risky. A nice README can still fail to explain what the agent actually does.

This skill gives the agent a rubric and workflow for judging skill quality from real files instead of summaries.

## Capabilities

- Skill discovery across local paths, GitHub paths, and marketplace-style sources.
- Inspection of real `SKILL.md` content before recommending or installing.
- Quality review for trigger clarity, workflow, evidence, gates, output UX, specificity, portability, and risk.
- Description tuning so a skill activates at the right time without keyword stuffing.
- Install planning across provider, scope, destination, copy-vs-symlink mode, and conflict handling.
- Skill creation from evidence, with references and an audit trail.
- README review for user value, capabilities, operating model, installation, and maintainer clarity.
- Gates before installs, overwrites, symlinks, config changes, or file writes.

## Operating Model

The workflow is:

```text
UNDERSTAND -> DISCOVER -> INSPECT -> JUDGE -> RECOMMEND -> USER GATE -> ACT -> VERIFY
```

The agent resolves the skill identity, reads the real files, applies the rubric, asks before risky actions, performs the smallest useful change, and verifies the result with the repo's quality checks.

## User Experience

Users can ask whether a skill is worth installing, why it fails to trigger, how to rewrite it, or how to publish it cleanly. The answer should be concrete: what is strong, what is risky, what should change, and what gate is needed before writing or installing.

For authors, the skill keeps `SKILL.md` lean while moving conditional depth into references and keeping the README friendly to humans.

## Installation

Install the published skill with:

```bash
npx octocode skill --name octocode-skills
```

## Maintainer Notes

Keep this README focused on skill quality as a user-facing workflow. Keep deeper rubric rules, discovery surfaces, install details, creation templates, and recovery behavior in the agent-facing skill file and references.
