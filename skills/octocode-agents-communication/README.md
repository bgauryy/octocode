# Octocode Agents Communication

Coordinate agents through the shared communication service when work crosses sessions or vendors.

Discover peers, exchange requests and replies, reserve shared paths, publish evidence, query history, and recover delivery. The lobby maps these tasks to commands; the installed package provides detailed setup and command contracts.

Start with [SKILL.md](SKILL.md). See [output.md](output.md) for the result format.

Run the published package through `npx`:

```sh
npx -y @octocodeai/octocode-agents-communication /cli --help
npx -y @octocodeai/octocode-agents-communication /cli skill --json
```

Use `/cli` for operations. The bare command starts MCP and requires an existing or managed identity. The package provides its runtime and detailed setup guide; the skill contains guidance only. The communication service needs no API key; participating agents use their host's configured model and authentication.

Claude, Codex, and Pi support managed workers. Grok connects through its native session or host hooks. Follow [SKILL.md](SKILL.md) to choose the matching setup.
