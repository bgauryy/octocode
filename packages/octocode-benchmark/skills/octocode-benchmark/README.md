# Octocode benchmark skill

By-hand CLI comparison from markdown questions: a fresh isolated agent per question and
per tool arm (baseline CLI vs `npx octocode tools …`), plus one blind judge agent per
question. No harness, no JSON, no schemas — results measured in total characters through the model (model-in delivered into context + model-out commands/args + final answer).

Start at `SKILL.md`. Concrete run recipe (preflight, measurement wrapper, spawn packets,
outputs) in `references/run-with-agents.md`; matchup-README convention in
`references/matchup-readme.md`.
