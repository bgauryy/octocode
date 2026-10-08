# Octocode Skills

Create, review, simplify, and install Agent Skills with clear activation and useful supporting files.

Start with [SKILL.md](SKILL.md), the shared standard for descriptions, lobbies, diagrams, output, related skills, docs, scripts, and configuration. See [output.md](output.md) for review and change formats.

From this folder, run `node scripts/skill-review.mjs <skill-or-collection-dir>` to check structure and links. `--json` gives structured findings; `--self-test` checks the reviewer. Editorial review and host activation tests assess meaning and selection separately.

Use [skill-sync.mjs](scripts/skill-sync.mjs) for local symlinks; `--help` explains its dry-run and apply options.
