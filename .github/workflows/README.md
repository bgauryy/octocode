# GitHub Actions workflows

| Workflow | Trigger | Purpose |
|---|---|---|
| `ci.yml` | Pull requests and pushes to `main` | Lint and test (no builds) |

CI never builds anything: no TypeScript build, no native addons, binaries, or
platform packages. Build locally (`build:dev`, `build:target <platform>`,
`build:all`, or `dev.mjs build:publish` for a release).

Run the same checks locally:

```bash
node skills-dev/octocode-dev/scripts/dev.mjs lint:ci
node skills-dev/octocode-dev/scripts/dev.mjs test:ci
```

npm publishing, Homebrew tap updates, and binary uploads are manual; see the
[release guide](../../skills-dev/octocode-dev/docs/RELEASE.md).
