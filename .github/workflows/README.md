# GitHub Actions workflows

| Workflow | Trigger | Purpose |
|---|---|---|
| `ci.yml` | Pull requests and pushes to `main` | Lint, typecheck and test; Rust checks; Windows process containment; native script tests; end-to-end tool health and native Node tests |
| `release.yml` | Manual dispatch (optional `ref`) | Builds every platform binary, runs each on its own OS and CPU (`smoke-binary.cjs`; musl in Alpine, gnu also at glibc 2.28), packs the npm tarballs into `release-packages` with `SHA256SUMS`, and runs the upgrade-path check on Linux, macOS and Windows |
| `nightly.yml` | Daily schedule and manual dispatch | The full union, pager and read_share matrices (`*_full_matrix`), which the default run only samples |

No workflow publishes. `ci.yml` builds no release artifacts: its `e2e` job builds the packages, a throwaway debug build
(`build:dev`) that `test:tools` drives through the CLI and MCP server and the
native package's Node tests load. It runs on Linux, macOS and Windows after the
`native` job passes. The
`native-windows` job runs the process-containment tests on Windows
(a bounded-run timeout, a cancelled `gh` token lookup, the job object and the
LSP memory cap each leave no surviving process); the
`native-scripts` job runs the native package's typecheck and script tests without a build,
and the `native` job runs the native package scripts unchanged (`fmt:rust:check`,
`lint` (oxlint, then clippy), `test:rust` with the nextest `ci` profile, `deny`, `shear`), then
regenerates the tool and config contracts (pinned
`cargo-typify`) and fails when a checked-in copy differs. Every Rust dev tool a job installs
(nextest, deny, shear, typify, zigbuild, xwin) takes its version from
`packages/octocode-native/dev-tools.json` through `rust-quality.mjs pins`; a test in
`rust-quality.test.mjs` fails on any workflow install that bypasses it. Release builds run in `release.yml` (locally: `build:target <platform>`, `build:all`,
or `dev.mjs build:publish`); its tarballs are what the owner publishes.

Run the same checks locally:

```bash
node skills-dev/octocode-dev/scripts/dev.mjs lint:ci
node skills-dev/octocode-dev/scripts/dev.mjs typecheck:ci
node skills-dev/octocode-dev/scripts/dev.mjs test:ci
node skills-dev/octocode-dev/scripts/dev.mjs build:dev && node skills-dev/octocode-dev/scripts/dev.mjs test:tools
yarn workspace @octocodeai/octocode-native test:node
yarn workspace @octocodeai/octocode-native typecheck && yarn workspace @octocodeai/octocode-native test:scripts && node --test packages/octocode-native/tests/audit-native.test.mjs
# needs the pinned cargo-typify: cargo install --locked $(node skills-dev/octocode-dev/scripts/rust-quality.mjs pins cargo-typify)
yarn workspace @octocodeai/config check:tool-contract && yarn workspace @octocodeai/config check:config-contract
# nightly.yml, from packages/octocode-native
cargo nextest run --locked -p octocode-native --run-ignored only -E 'test(/full_matrix/)'
```

`test:ci` starts with the skill linter over `skills/` and the skill script tests; the shared skill workspace (`skills/common`) runs in the workspace lint and test runs. After editing a skill, run `dev.mjs skills:check`, which adds the shared workspace's lint and test.

npm publishing (from the `release-packages` tarballs) and Homebrew tap updates are manual; see the
[release guide](../../skills-dev/octocode-dev/docs/RELEASE.md).
