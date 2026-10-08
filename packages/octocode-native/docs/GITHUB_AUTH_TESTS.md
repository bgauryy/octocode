# GitHub authentication tests

Authentication crosses configuration, OAuth HTTP, credential persistence, and
optional GitHub CLI discovery. Tests use synthetic credentials and local HTTP
fixtures. They do not log in to a real GitHub account or save OS credentials.
The packaged acceptance test deletes only its unique fictional OS-store hostname.

## Coverage matrix

| Flow | Test location | Check |
|---|---|---|
| Process, workspace, global `.env` | `src/config/mod.rs`; `packages/octocode-config/tests/dotenv-fallback.test.ts` | Source precedence across aliases, blank fallback, classification opt-out, bootstrap protection, redaction |
| Outgoing credential selection through CLI/MCP | `packages/octocode-config/tests/token-precedence.acceptance.mjs` | Synthetic GitHub token names and the classification key reach a loopback provider from the winning source |
| Explicit token and host selection | `auth/resolver/tests.rs` | Override wins; environment token is host-scoped; no storage or subprocess work when environment wins |
| Stored credential selection | `auth/resolver/tests.rs`; `auth/credential_store.rs` | Home wins over OS store; OS compatibility fallback remains; read-only inspection does not refresh |
| No usable credentials | `auth/resolver/tests.rs` | Anonymous selection with absent sources; storage errors remain visible when no fallback succeeds |
| Device login | `login/flow_tests.rs` | Pending authorization then success; complete credential saved exactly once |
| Failed device login | `login/flow_tests.rs` | Denied/expired authorization, cancellation, timeout, and failed persistence cannot report successful login |
| Refresh | `login/flow_tests.rs`; `login.rs` | Rotated tokens persisted to their source, write errors propagated, missing/expired refresh token and missing enterprise client ID rejected, concurrent refreshers post once |
| Refresh races | `login/flow_tests.rs` | Refresh cannot restore deleted home credentials or overwrite a newer login |
| Failed refresh | `auth/resolver/tests.rs` | `gh` fallback retains its own source; deleted credentials are not reused; cancellation/deadline stops work |
| `gh auth token` | `auth/discovery.rs`; `tests/auth_discovery.rs` | Host argument, explicit PATH, environment-token removal, bounded output, subprocess failure, cancellation, request pinning |
| Secret formatting | `auth/storage.rs`; `login/flow_tests.rs` | Access and refresh tokens never appear in credential/result `Debug` output |
| Main encrypted home credentials | `auth/home_store/tests.rs`; `tests/auth_discovery.rs` | Native reads an independent Node fixture; CLI selects it before gh |
| CLI logout | `src/cli/system.rs::auth_tests` (relative to `crates/cli`) | Configured-host selection, deletion error propagated, environment credentials unchanged |
| Home persistence | `auth/home_store/tests.rs`; `auth/credential_store.rs` | Save/load/update/delete, host isolation, key cleanup, corruption, wrong/missing key, bounded size, Unix permissions, symlink/hard-link rejection, concurrent writers, partial logout failure |
| Packaged interfaces | `packages/octocode-native/tests/auth-home.acceptance.mjs` | Native reads a Node-written home and Node decrypts the native rewrite; CLI, N-API `executeMcp`, stdio MCP; explicit home isolation; home logout and OS-delete failure reporting; corrupt-home gh fallback |
| Real OS-store round trip | Not exercised | Platform integration requires an isolated credential-store test environment |

Paths without a package prefix are relative to
`crates/runtime/src/providers/github`, except `src/config/mod.rs` and `tests/`,
which are relative to `crates/runtime` and `crates/cli`, respectively.

## Run

From `packages/octocode-native`:

```sh
cargo test -p octocode-native --lib providers::github::
cargo test -p octocode-native --lib config::
cargo test -p octocode-cli --test auth_discovery
cargo test -p octocode-cli --test cli auth_
cargo test -p octocode-cli --bin octocode auth_tests
```

From the repository root, after rebuilding native, CLI, and MCP:

```sh
node packages/octocode-native/tests/auth-home.acceptance.mjs
yarn workspace @octocodeai/config test:tokens:acceptance
yarn workspace octocode-mcp test:contracts:classification
```

## TDD evidence

The main-home regression first failed with a null username instead of the saved
fixture username. It now passes against the same independent Node-generated
AES-256-GCM fixture (16-byte IV). The packaged test independently decrypts native
writes with Node crypto, checking compatibility in both directions.

The refresh/logout race test failed because an in-flight refresh recreated a
deleted credential. Persistence now compares the original credential under the
home-store lock before replacing it; deletion and a newer login both reject the
stale write. Forced device login preserves existing bytes when authorization fails.

Debug-redaction regressions failed before replacing derived `Debug` on
`OAuthToken`. The missing-enterprise-client-ID test
also failed before adding a guard that prevents an OAuth request without the
host's configured client ID.

Device login tests run the real device-code request, polling, user lookup,
credential construction, and error handling against local HTTP fixtures. Some
inject the final save to exercise write failures; others persist through the real
encrypted home store. OS-store compatibility uses injected store boundaries;
a real OS-store save/load round trip remains outside this suite.

## Authentication verification receipt

On 2026-09-26, verification passed for 182 Node config tests (including coverage
thresholds), 109 native Node tests, 88 GitHub/provider unit tests, 27 native config
tests, 25 GitHub runtime integration tests, three CLI auth integration tests, and
the CLI logout unit test. All four auth integration tests passed together,
including the main-home CLI and MCP credential-pinning regressions. Earlier
auth integration runs also exposed
intermittent request timeouts; its failure output includes gh-call and HTTP-path
diagnostics so a timeout is not mistaken for a credential-selection failure.

Config, native, CLI, and MCP builds passed, along with scoped native Clippy,
formatting, and documentation checks. Packaged CLI/addon/stdio MCP fixtures
verified home persistence, independent Node decryption, separate-process writes,
home isolation, dotenv precedence, gh fallback, lazy catalog startup, and redaction.

The packaged logout test removed the home credential and preserved other hosts.
This host denied the OS-keychain delete operation; the CLI correctly returned a
partial failure. This is not a verified real OS-store round trip. The acceptance
test reports OS availability separately instead of silently treating denial as
successful OS deletion.

## Token precedence verification receipt

The subsequent source-precedence change passed 234 Node config tests, 31 native
config tests, 24 outgoing Authorization probes through real CLI/MCP processes,
18 classification availability scenarios, and three installed-helper probes.
The cross-alias regressions failed before the fix: a global canonical key could
mask a workspace alias. The acceptance probes now verify the selected synthetic
credential at a loopback provider without exposing real tokens.
