# Octocode-specific Rust development

Load only inside the Octocode monorepo. Read the checkout's `AGENTS.md` and `skills-dev/octocode-dev/SKILL.md` first. Those live owners override examples here; do not copy these project policies into unrelated Rust repositories.

Paths below are relative to the repository root. They are source locations to inspect in that checkout, not dependencies required in a standalone skill bundle.

## Ownership

| Area | Source owner to inspect |
|---|---|
| Crate responsibilities and dependency direction | `packages/octocode-native/ARCHITECTURE.md`; `skills-dev/octocode-dev/docs/DEVELOPMENT.md` |
| Public schemas, descriptions, instructions and limits | Sibling core `../octocode-mcp-host/packages/octocode-core`; `@octocodeai/config` generates `packages/octocode-config/contract/` |
| Execution, admission, security, errors and response shaping | Native runtime; reusable algorithms in engine; CLI/MCP and Node launchers adapt/delegate |
| Config/home/env resolution | `@octocodeai/config` rules; do not duplicate loading in tool/host adapters |
| Toolchain, features, lint/profile settings and tool pins | Native `Cargo.toml`, `rust-toolchain.toml`, `dev-tools.json`; task scripts are authoritative |
| Shared skill resources | `skills/common`; generated copies must be regenerated, not hand-edited |

For contract changes: build core, run `node skills-dev/octocode-dev/scripts/dev.mjs contracts:regen`, verify the changed text in generated output, rebuild native and affected consumers, then restart the MCP server used for verification. Use generated wire types. Declare new field effects or confirm documented ancestor inheritance; the declaration is not a behavioral test.

## Development and checks

- Use `node skills-dev/octocode-dev/scripts/dev.mjs <task>` from the root. `build:dev` targets the host for local work; use native release/profiling builds for production performance measurements. Cross-platform/release builds require that task scope.
- Keep one build owner on the shared repository `target/`; focused fixers run focused tests. Do not create a private target directory to evade contention, clean shared artifacts or format the whole tree during a concurrent build. Follow `AGENTS.md` for retries and cleanup.
- Read `packages/octocode-native/scripts/rust-tests.cjs` for the current package/feature selections; use the package's `test:rust` or existing focused runner. Check manifest doctest settings rather than assuming nextest covered examples.
- `lint:rust` owns the Clippy ratchet. Inspect its source/config before changing suppressions or baselines; floors and baseline counts must not grow to pass a change. Existing tool versions and optional verification selections live in `dev-tools.json`.
- After native behavior changes, rebuild the affected interface and exercise CLI config/schema plus the changed call; use a fresh MCP process. Check each result row, errors and warnings rather than relying on the envelope exit status.
- For skills/prose changes, run `skills:check` and applicable documentation verification. A prose-only change does not justify a native rebuild.

## Evidence and lifetime invariants

- Preserve all selected tool evidence. Walk response pages before tool-level continuations; verify reconstruction exactly once without gaps/duplicates. Keep indivisible values whole and disclose terminal limits/unread scope.
- Do not add guessed timeouts, match/file caps or truncation as a resource fix. Follow the owning contract/resource policy and return a continuation or explicit refusal.
- Preserve request registration, cancellation, blocking-worker completion and shutdown invariants. Test admission failure, pre-start drop, cancellation while running and close with outstanding resources.
- Native launchers/hosts must not gain fallback tool implementations. Match CLI and MCP schemas, fields and observable behavior.

Do not commit or stash in this shared checkout; leave source changes for the owner. Stop only processes started for this task, by PID/task ID. Remove task scratch and private artifacts before handoff, preserving cited evidence and other sessions' changes.
