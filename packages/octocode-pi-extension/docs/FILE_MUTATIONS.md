# File mutation architecture

## Ownership

`@octocodeai/octocode-extension-rust` is the extension's dedicated native package.
It has no dependency on the research tools' `octocode-engine`, Pi, the agent host,
or a database service. Filesystem algorithms are adapted from the local
`octocode-agent` Rust filesystem service; host contracts are not copied.

| Owner | Responsibility |
| --- | --- |
| `file-tool.ts` | Public schema, whole-batch preflight, dispatch, delete policy, rendering |
| `edit-tool.ts` | Matching, ambiguity/overlap checks, freshness requirements, edit evidence |
| `write-tool.ts` | Full-write validation and prepared operation |
| `file-text.ts` | Unicode validation and mapping normalized spans to original text |
| `file-mutation-target.ts`, `path-guard.ts` | Allowed-root policy and canonical path/alias checks |
| `native-files.ts` | Size limit, native API adaptation and AbortSignal bridge |
| `file-state.ts` | Process-local queues and read-state lifecycle; native internal state writes |
| `file-mutation-receipt.ts` | Post-commit read-state/UI bookkeeping and warnings |
| `octocode-extension-rust` | Descriptor-relative snapshots, bounded reads/hashes, replacement/deletion, sync, native queues and line diff |

The Pi package retains an engine dependency for its bundled research CLI. Direct
extension filesystem and diff calls use the dedicated extension native package.
The package exposes `NativeErrorCodes` and `nativeErrorCode`; Pi branches on those
stable values instead of parsing diagnostic messages.

## Mutation flow

1. Validate every query's fields and Unicode before starting its operation.
2. Resolve allowed roots and canonical targets; reject duplicate targets.
   Capture a native snapshot with file and ancestor identities plus a SHA-256
   content digest. Edit snapshots return bytes from that same bounded read.
3. Prepare edit matches and run line diff asynchronously in the native worker.
   All batch preflights complete before the first mutation.
4. Enter the canonical process-local queue. Revalidate host path policy and
   aliases. Rust opens directories without following symlinks and verifies the
   expected snapshot; replacement also rechecks before the commit point.
5. Create parents as needed and write a same-directory exclusive temporary with
   appropriate initial permissions. Check cancellation while writing and before
   commit. Sync the temporary, then rename an existing target or exclusively link
   a new target (Windows uses a non-replacing relative rename). Delete removes
   the captured leaf, including a leaf symlink.
6. Sync the parent directory where supported and return the native receipt.
   Windows flushes file data and explicitly reports `durable: false`. Completed commits
   remain successful if cancellation arrives later. Failed post-commit sync or
   cleanup produces warnings; `durable` reports the sync outcome separately.
7. Refresh read state and UI bookkeeping. Failures after commit are warnings,
   never a claim that the filesystem mutation did not occur.

Internal MCP catalog and effort-setting writes use the same native replacement
primitive. Read-state refresh uses native bounded snapshots without transferring
content back to JavaScript. A prepared edit supplies its existing digest to the
freshness check, avoiding another read and JavaScript hash of the same file.
Small already-in-memory hashes still use Node's native crypto implementation.

## Text and path semantics

Successful writes record read state. Exact and normalized anchors validate current
content; ambiguous exact anchors fail even when matches overlap. `replaceAll`
uses nonoverlapping replacements. Position-only `lineRange` edits require a fresh
recorded read; supplied matching `oldText` makes the range self-verifying.

Edits reject invalid UTF-8 and NUL-containing binary input. Strings reject unpaired
surrogates. Matching uses an LF view; replacements map back to original text.
BOM and LF/CRLF/lone-CR bytes outside selected spans survive unchanged. Inserted
newlines use the selected region's style, then the containing line/file.

The host resolves allowed symlinks before passing canonical paths to Rust.
Native operations reject symlink traversal, including a parent swapped to a link.
Delete canonicalizes only its parent and removes a leaf link itself. Allowed
roots remain cwd, home, temporary directories, and configured `ALLOWED_PATHS`.

Both existing-file reads and new content are limited to **64 MiB** per file.
The native reader checks metadata and enforces the limit while streaming, so a
file growing during the read cannot bypass it. Oversized existing write/edit/delete
queries fail preflight. Native file I/O and edit-preparation diff run in workers;
synchronous native diff is reserved for the existing rendering API.
Prepared evidence caches its changed lines in a WeakMap, so fresh results do not
repeat diff computation during rendering. Results loaded from persisted history
initialize that cache synchronously on their first render.

## Build and package

Build the dedicated native workspace before Pi:

```sh
yarn workspace @octocodeai/octocode-extension-rust build
yarn workspace @octocodeai/pi-extension build
yarn workspace @octocodeai/octocode-extension-rust verify
yarn workspace @octocodeai/pi-extension test
```

The loader selects one exact-version prebuilt optional package: macOS arm64/x64,
Linux x64 glibc/musl, Linux arm64 glibc, or Windows x64 MSVC. Published installs
need no Rust compiler and ship no source-build fallback. Missing or unsupported
native loading fails explicitly. The native package's `platforms.cjs` owns the
matrix; `build:all` and `platforms:check` build and validate every package. The
Extension Rust CI workflow executes Rust, N-API and actual tarball-install tests
on all six runtimes.

On Windows, `ALLOWED_PATHS` separates entries with semicolons or commas, retaining
drive colons; Unix uses colons or commas. Queue and duplicate-preflight keys
collapse Windows case and namespace aliases. Distinct case-only names in a
case-sensitive Windows directory may be conservatively rejected in a batch;
read-state authority remains keyed by the exact canonical path.

## Limits and further offload

Preflight is not a transaction: a later runtime failure retains earlier commits.
Recursive parent creation can leave empty directories after a failure. Snapshot
checks and process-local queues are cooperative preconditions, not OS-level
compare-and-swap against arbitrary external editors. Descriptor pinning prevents
symlink traversal; an external actor can still rename a pinned directory or change
a leaf after the final check. No universal adversarial containment claim is made.

Successful file and directory syncing is reported, not proof against every
filesystem, power-loss, or hardware failure. Replacement changes the file identity;
extended attributes, ownership and hard-link relationships are not preserved.
Unix preserves ordinary mode bits. Windows copies and protects the effective DACL
of an existing file, freezing subsequent inheritance. Explicit private mode 0600
creates a protected caller/SYSTEM DACL before writing bytes. Windows has no
portable directory-entry persistence contract, so successful operations
include a warning with `committed: true, durable: false`.

Line diff now belongs to the dedicated native package, with exact empty/final-line
semantics and both sync and async entrypoints. The JavaScript Myers implementation
and opt-in native fallback are removed. End-to-end timings include NAPI copies;
Rust is not assumed faster for every small workload.

MCP protocols, dynamic skill policy, plans, context, approvals and worker control
remain TypeScript. Registry locking still has a synchronous wait; moving it safely
requires an asynchronous caller/lease contract, so it is a distinct future change.
