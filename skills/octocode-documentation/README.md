# Octocode Documentation

Create, repair, or review documentation for humans and coding agents, with style guidance, Markdown checks, and verified claims. The flow lives in `SKILL.md`.

```bash
npx -y octocode skill install octocode-documentation
node scripts/style-lint.mjs README.md     # maintainer check
node scripts/style-lint.mjs --self-test
```

Then run the `octocode-skills` review against this folder. Sources: [Google developer documentation style guide](https://developers.google.com/style) · [Diátaxis](https://diataxis.fr/) · [agents.md](https://agents.md/)
