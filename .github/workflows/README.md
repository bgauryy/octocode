# GitHub Actions workflows

| Workflow | Trigger | Purpose |
|---|---|---|
| `ci.yml` | Pull requests and pushes to `main` | Lint and test; Rust checks; Windows process containment; native script tests; end-to-end tool health and native Node tests |
| `nightly.yml` | Daily schedule and manual dispatch | The full union, pager and read_share matrices (`*_full_matrix`), which the default run only samples |

CI publishes nothing: no release builds, platform packages, or uploaded
binaries. Only the `e2e` job builds the packages, a throwaway debug build
(`build:dev`) that `test:tools` drives through the CLI and MCP server and the
native package's Node tests load. It runs on Linux, macOS and Windows after the
`native` job passes. The
`native-windows` job runs the process-containment tests on Windows
(a bounded-run timeout, a cancelled `gh` token lookup, the job object and the
LSP memory cap each leave no surviving process); the
`native-scripts` job runs the native package's script tests without a build,
and the `native` job runs the native package scripts unchanged (`fmt:rust:check`,
`lint`, `test:rust` with the nextest `ci` profile, `deny`, `shear`), then
regenerates the tool and config contracts (pinned
`cargo-typify` 0.8.0) and fails when a checked-in copy differs. Release builds stay local (`build:target <platform>`, `build:all`,
or `dev.mjs build:publish`).

Run the same checks locally:

```bash
node skills-dev/octocode-dev/scripts/dev.mjs lint:ci
node skills-dev/octocode-dev/scripts/dev.mjs test:ci
node skills-dev/octocode-dev/scripts/dev.mjs build:dev && node skills-dev/octocode-dev/scripts/dev.mjs test:tools
yarn workspace @octocodeai/octocode-native test:node
yarn workspace @octocodeai/octocode-native test:scripts && node --test packages/octocode-native/tests/audit-native.test.mjs
# needs cargo-typify 0.8.0 (cargo install cargo-typify --version 0.8.0 --locked)
yarn workspace @octocodeai/config check:tool-contract && yarn workspace @octocodeai/config check:config-contract
# nightly.yml, from packages/octocode-native
cargo nextest run --locked -p octocode-native --run-ignored only -E 'test(/full_matrix/)'
```

`test:ci` starts with `skills:check`: the skill linter over `skills/`, the shared report-viewer check, and the skill script tests. Run it alone with `dev.mjs skills:check` after editing a skill.

npm publishing, Homebrew tap updates, and binary uploads are manual; see the
[release guide](../../skills-dev/octocode-dev/docs/RELEASE.md).
