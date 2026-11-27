Load when a lint, the clippy ratchet or a workspace check fails, or when re-recording provider answers.

# Lint checks and recorded answers

## Checks

`scripts/` is `skills-dev/octocode-dev/scripts/`.

- **oxlint**: the root `.oxlintrc.json` is the only config (type-aware, warnings denied). Older violations sit in `.oxlintrc.baseline.json`, one override per file; it only shrinks: `scripts/oxlint-baseline.mjs` fails on a stale entry or an unlisted violation, and `--write` only removes entries. The sibling core lints with copies of both files; `$DEV verify` runs `core-lint-sync.mjs`, which fails on drift and skips without the sibling.
- **Workspace scripts**: `lint:oxlint`, `lint:own` (every other check; none in mcp-cli), `lint` (both), `verify:own` (`verify` without oxlint), `test:coverage` (floors; `test:ci` runs it). Typecheck is `tsgo --noEmit` over the root `tsconfig.base.json`. CI runs `lint:ci` and `typecheck:ci`.
- **Clippy ratchet** (`$DEV lint:rust`, `scripts/rust-lint.mjs`, local only; CI keeps `-D warnings`): production-only lints and bans from `packages/octocode-native/clippy-ratchet.toml` (fixed sleeps, `Vec::truncate`, child processes, HTTP client construction, unbounded `fs::read`, non-atomic `fs::write`). Older findings are counted per crate in `clippy-baseline.json`: growth fails and names the files, a shrink is stale until `--write`, `--adopt` adds a newly listed lint. A site that must stay says why: `#[expect(clippy::disallowed_methods, reason = "...")]`. Tests are not judged. Clippy `pedantic` and `nursery` are never enabled wholesale: adopt single lints into the ratchet.
- **Rules that stay tests**: `module_boundaries` (use paths and layers) and `no_error_code_is_spelled_as_a_string` (JSON strings); clippy sees neither.
- **Recorded provider answers**: native mocks replay `test-utils/provider-answers`. `$DEV answers:record [id,...]` re-records them live through the built CLI (cases in `scripts/providers/recorded-plan.mjs`); it refuses a recording that holds the token or a private repository, loses a field path when trimmed, or makes the tool ask anything else.
