# octocode-dev

Repo-internal skill and toolbox for developing the Octocode monorepo. It owns the task runner that replaced the root `package.json` scripts, the change pipeline (core contract → generated contract → native → interfaces), the repository's developer docs, and the tool-audit lanes. It is not published.

Entry point: [`SKILL.md`](SKILL.md).

```bash
node skills-dev/octocode-dev/scripts/dev.mjs --help      # all repo tasks
node skills-dev/octocode-dev/scripts/dev.mjs build:dev   # fast local build
node skills-dev/octocode-dev/scripts/dev.mjs verify      # full repo contract
```

| Folder | Contents |
|---|---|
| [`scripts/`](scripts/README.md) | `dev.mjs` task runner, workspace health, docs gate, dependency dedupe, dev setup, publish guard, build helpers, tool inventory |
| [`docs/`](docs/DEVELOPMENT.md) | Developer docs: [development](docs/DEVELOPMENT.md), [adding config](docs/ADDING_CONFIG.md), [release](docs/RELEASE.md), [tool quality bar](docs/TOOL_QUALITY.md) |

User-facing docs stay in the repository `docs/` folder (`<repo>/docs/README.md`).
