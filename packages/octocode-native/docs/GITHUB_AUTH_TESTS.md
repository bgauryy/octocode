# GitHub authentication tests

Authentication crosses configuration, OAuth HTTP, credential persistence, and
optional GitHub CLI discovery. Tests use synthetic credentials and local HTTP
fixtures. They do not log in to a real GitHub account or save OS credentials.

## Coverage matrix

| Flow | Test location | Check |
|---|---|---|
| Process, workspace, global `.env` | `src/config.rs`; `packages/octocode-config/tests/dotenv-fallback.test.ts` | Source precedence across aliases, blank fallback, classification opt-out, bootstrap protection, redaction |
| Outgoing credential selection through CLI/MCP | [Packaged checks](#packaged-checks) | Synthetic GitHub token names and the classification key reach a loopback provider from the winning source |
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
| Packaged interfaces | [Packaged checks](#packaged-checks) | Native reads a Node-written home and Node decrypts the native rewrite; CLI and stdio MCP; explicit home isolation; home logout and OS-delete failure reporting; corrupt-home gh fallback |

Paths without a package prefix are relative to
`crates/runtime-base/src/providers/github`, except `src/config.rs` and `tests/`,
which are relative to `crates/runtime-base` and `crates/cli`, respectively.

## Run

From `packages/octocode-native`:

```sh
cargo test -p octocode-native --lib providers::github::
cargo test -p octocode-native --lib config::
cargo test -p octocode-cli --test integration auth_discovery::
cargo test -p octocode-cli --test integration cli::auth_
cargo test -p octocode-cli --bin octocode auth_tests
```

## Packaged checks

An agent runs these by hand on the built CLI and MCP server, in a fresh `OCTOCODE_HOME` and `HOME` with
synthetic tokens, judging each answer by reading it ([method](../../octocode-benchmark/CHECKS.md)).
GitHub calls go to a loopback server (`GITHUB_API_URL`) that logs the `Authorization` header it receives.

- [ ] Token precedence: set `GITHUB_TOKEN`, `GH_TOKEN`, the legacy `OCTOCODE_TOKEN` and
  `GITHUB_PERSONAL_ACCESS_TOKEN`, a home `.env` and a workspace `.env` to different
  fake values, then remove them one at a time; the loopback log shows the value the configuration docs rank
  first, on CLI and MCP. `auth status` names the same source. The classification key reaches its loopback
  provider the same way.
- [ ] Stored login: a credential saved in the home is used by a second CLI process and a new MCP server; `auth logout` removes it, and a token in the environment still wins.
- [ ] Corrupt home credential: the next call falls back to `gh auth token` (a stub `gh` on `PATH`) and says so.

A real OS credential-store save and load is not covered: it needs an isolated credential-store environment.
OS-store compatibility uses injected store boundaries, so a denied keychain delete is reported as a partial
failure, not as success.
