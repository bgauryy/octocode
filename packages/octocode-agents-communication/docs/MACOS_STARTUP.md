# macOS copied-executable startup observation

On macOS 26.5.2 ARM64, 20 fresh copies of the release CLI were launched with
`--help` in a disposable-directory probe. Eighteen exited successfully; two were
killed at the 10-second diagnostic deadline. Three took more than 3 seconds.
Samples from all three showed only `_dyld_start` and a 96 KB footprint, before
application initialization. One later completed at 6.34 seconds.

Evidence: `.octocode/benchmarks/communication-improvements/cold-start/`, including
result.json, process states and 1-second stack samples. A separate copied-skill
regression timed out at 60 seconds; its isolated repeat passed in 0.87 seconds.
These observations do not establish a signing, Gatekeeper or cache root cause.
The executable has a linker-generated ad-hoc signature; a later production-readiness
review confirmed `codesign --verify --strict` passes. Similar symptoms are
[reported in the Codex repository](https://github.com/openai/codex/issues/17447);
that report is corroboration, not proof of this machine's cause.

Keep a stable installed executable and use bounded host startup deadlines. Eval
harnesses may validate a newly frozen copy using read-only `--help` before
starting agents; a failed setup remains a failed artifact. Do not retry mutations
blindly, disable OS security, or interpret a pre-main stall as a DB deadlock.
The runtime cannot enforce its own timeout before execution reaches main.

This platform limitation remains open. It is separate from successful warm CLI,
MCP, hook, transaction and native delivery measurements.

## Distribution safeguards

The build and package scripts now enforce bounded artifact checks:

- An unchanged executable is verified in place and keeps its inode. Previously
  every rebuild made a fresh copy even when Cargo produced identical bytes.
- A changed executable is checked before replacing the installed file. On a
  matching host, its read-only `--help` must return the expected package contract
  within 10 seconds. Failure keeps the previous executable; there is no retry.
- On macOS, Darwin binaries must also pass strict code-signature verification.
  This checks signature integrity, not Developer ID identity or notarization.
- Packaging verifies checksums, creates an unpublished archive, extracts it into
  a fresh directory, and repeats native signature/startup checks. The matching
  host's executable must embed the exact packaged `SKILL.md`; the extracted
  POSIX launcher must also start. Only then is the archive atomically published.
  Failure keeps the previous archive and removes staging files. Incomplete or
  unexpected files in platform binary directories fail packaging.
- Foreign-target startup and Windows launcher checks are explicitly reported as
  needing native CI. A checksum pass must not be presented as execution proof.

Five artifact tests exercise inode preservation, failed-candidate rollback,
startup timeout and malformed output, extracted binary/skill/launcher checks,
archive rollback, skill drift, checksum errors and staging-file rejection.
They use isolated executable fixtures and test the publication gates; they do
not establish the frequency or cause of the macOS loader stall.

These safeguards reduce unnecessary fresh copies and reject an artifact that
fails the observed startup gate. They do not prove that a later downloaded copy
will avoid the platform stall. No security setting, quarantine attribute or
signature requirement is disabled.

## Matched signing experiment

A later 2026-09-25 experiment froze release SHA-256
`04fd55f909c58aa503660795113782097f277d575e01ddaf95ed80367ec0713c`
and compared ten fresh unchanged copies against ten fresh copies explicitly
re-signed with `codesign --force --sign -`. Pair order alternated; both arms
received strict signature verification and the same 10-second read-only startup
deadline. No publisher keys, quarantine changes or OS security bypasses were used.
Apple documents the dash pseudo-identity as [ad-hoc signing without an identity](https://developer.apple.com/documentation/security/seccodesignatureflags/adhoc).

| Observation | Linker-signed baseline | Explicit ad-hoc signing |
| --- | --- | --- |
| Successful starts | 10/10 | 10/10 |
| Deadline failures | 0 | 0 |
| Median startup | 474.44 ms | 577.89 ms |
| Maximum startup | 498.28 ms | 587.03 ms |
| Executable bytes | 22,481,712 | 22,369,104 |
| Signature flags | `0x20002` (ad-hoc, linker-signed) | `0x2` (ad-hoc) |
| Developer Team identity | Absent | Absent |

[Frozen plan, raw results and reproducible probe](../../../.octocode/benchmarks/communication-production-readiness/macos-signing/2026-09-25T18-27-55.630Z/result.json)
are retained. No trial crossed the three-second sampling threshold, so this run
did not reproduce a dyld stall. Explicit signing changed the identifier/CDHash
and was slower in this small run; it did not establish a startup reliability
improvement. The build therefore retains the valid linker signature and does not
add speculative re-signing. Earlier failed cold-copy observations remain valid
evidence of an unresolved intermittent issue.

Prefer one stable installed executable and existing persistent MCP/Pi connections
for repeated coordination. A process cannot enforce its own deadline before
`main`; the external build/pack and host startup gates remain the containment.
No extra preflight process is added to every message or lock operation.

## Public distribution boundary

The current ad-hoc signature is not a production publisher identity. Apple
requires an appropriate Developer ID certificate, hardened runtime and secure
timestamp for notarized external distribution. Ad-hoc re-signing would not
satisfy those requirements and is not justified as a fix for the observed stall.
See Apple's [notarization requirements](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution)
and [external-build signing workflow](https://developer.apple.com/documentation/xcode/creating-distribution-signed-code-for-the-mac/).

Signing and notarization remain a release prerequisite requiring the publisher's
identity and CI credentials; this task did not create, export or use signing keys,
submit software externally, or change the package's private/unpublished status.
Bundled SHA-256 files detect byte mismatch but do not authenticate the publisher.
A read-only `security find-identity -v -p codesigning` check on 2026-09-25 found
zero valid code-signing identities on this machine. Public signing cannot be
completed here without the publisher supplying that prerequisite.

## Dependency review

On 2026-09-25, `cargo audit` 0.22.2 checked this package's lockfile against RustSec
revision `e2111519ba6d14a5da59a7b2e5c8083ae8a37c01` (1,271 advisories): zero known
vulnerabilities and no warnings. This is a dated advisory scan, not a claim that
all dependencies or vendor CLIs are vulnerability-free. Re-run it during release
CI alongside native-target startup checks. No dependencies were changed by this
distribution hardening.
