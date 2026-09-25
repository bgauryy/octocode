# Native release gates and support policy

The package is private and unpublished. A successful local build is development evidence; it does not approve a release or certify every operating system. The [native workflow](../../../.github/workflows/agents-communication.yml) builds and executes each target selected by the shipped launchers. It never publishes, provisions vendor credentials, signs with a publisher identity, or changes repository settings.

## Platform contract

| Native target | CI runner | Required execution |
| --- | --- | --- |
| `aarch64-apple-darwin` | `macos-15` | Rust tests, extracted binary, shell launcher, CLI/MCP, upgrade/restore |
| `x86_64-apple-darwin` | `macos-15-intel` | Same |
| `aarch64-unknown-linux-gnu` | `ubuntu-24.04-arm` | Same |
| `x86_64-unknown-linux-gnu` | `ubuntu-24.04` | Same |
| `aarch64-pc-windows-msvc` | `windows-11-arm` | Rust tests, extracted `.exe`, PowerShell launcher, CLI/MCP, upgrade/restore |
| `x86_64-pc-windows-msvc` | `windows-2022` | Same |

These runner labels, including Windows ARM64 for public and private repositories, were checked against [GitHub's runner reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners) on 2026-09-25. Actual runner availability and account capacity remain external requirements. A queued or unavailable runner is not a passing gate. Older OS versions, musl/Alpine, mobile platforms, and remote/shared network databases are outside this tested release contract.

The workflow pins action commit hashes and Rust 1.96.1; Node 24 is a maintainer/test dependency, not an installed skill runtime dependency. A separate job checks the declared Rust 1.89 minimum; another audits the lockfile with cargo-audit 0.22.2. Windows tests invoke native binaries directly and separately check the PowerShell launcher. They do not pretend a POSIX shebang executes natively on Windows.

## Automated checks

Run from the package directory:

```sh
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
node scripts/build-skill.mjs --release
node scripts/release-smoke.mjs --output out/release-smoke.json
node --test tests/release-evidence.test.mjs
```

The smoke script packages and extracts the actual skill, verifies its embedded instructions, and uses that extracted executable for workspace discovery, conflicting path leases and handoff, idempotent messages, correlated CLI→MCP replies, atomic reply acknowledgement, and audit checks. No model or vendor account is involved.

For every canonical historical SQL schema v1–v5, it verifies that normal commands refuse an implicit upgrade, explicitly migrates to v6 exactly once, exports a consistent snapshot, verifies its hash, copies it to a separate restore path, resumes the restored identity, sends and acknowledges a new correlated message, and checks integrity, foreign keys, retained history and audit. The original database and backup remain unchanged by the restore rehearsal. Fixture SQL hashes are recorded. This complements concurrent-WAL export tests; it does not replace them.

The final workflow job requires all six native jobs, the minimum-Rust check, and the dependency audit. It downloads the artifacts and runs:

```sh
node scripts/verify-release-evidence.mjs release-artifacts EXACT_40_CHARACTER_GIT_REVISION
```

The gate rejects missing/duplicate targets, wrong revisions or versions, dirty builds, mixed harnesses/skills, unexecuted startup or launcher checks, incomplete restore coverage, and changed archive hashes. Each archive contains only its tested target. The receipt's scope is deliberately `native-core-artifact-gate`: it is not a vendor compatibility certificate or publisher authentication.

## Release approval requirements

- All six native execution receipts must pass for the exact clean release revision. Protect the final workflow check in repository settings; writing this workflow does not configure branch protection.
- Review authenticated live compatibility evidence for every advertised vendor/version and hook path. Offline core tests cannot prove private vendor APIs still work. Claude/Grok Unix socket transports require Unix; Windows requires a supported alternate transport or raw host integration.
- Re-run copied/extracted artifact startup checks on final distributed bytes. Mac ad-hoc signature integrity and SHA-256 checks do not establish publisher identity or prove downloaded/quarantined Gatekeeper behavior. Public macOS distribution needs the chosen Developer ID/notarization process and a fresh downloaded-artifact check; public Windows distribution needs the chosen signing/trust process and its native check. These require maintainer credentials and an explicit distribution decision; this workflow requests neither.
- Export a database backup and preserve referenced `.octocode/communication/` documents separately before upgrades. Database snapshots contain every workspace in that DB and may contain sensitive peer messages. Restore to a new path after stopping old delivery owners; do not replace an active DB under live processes. Never infer that restored staged/uncertain delivery records are safe to replay automatically.

## Current evidence and limits

On 2026-09-25, the extracted-archive smoke passed locally on Apple Silicon macOS with Rust 1.96.1 and Node 26.4.0, including all five upgrade/restore flows. The machine-readable result is `out/release-smoke.json`; its binary/archive/harness/SQL hashes identify the tested bytes. The local source was dirty, so this result intentionally cannot satisfy the clean release gate. Nine gate-verifier tests passed, including rejected missing, stale, tampered and unexecuted receipts. The YAML was parsed locally.

The new hosted workflow has not run from this working tree. Linux, Intel macOS, Windows x64/ARM64, the Node 24 CI environment, Rust 1.89 compilation, publisher trust checks and protected-branch configuration remain unverified here. These are explicit gates, not claimed passes.
