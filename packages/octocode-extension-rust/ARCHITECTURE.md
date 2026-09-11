# Extension native boundary

The extension calls a narrow N-API library. Filesystem and asynchronous diff work
run on libuv worker threads; no subprocess service, Pi SDK, tools-core, or engine
dependency enters this package. A small synchronous diff entry point supports UI
callers; expensive callers should use `computeLineDiffAsync`. Edit preparation
uses `generateDiffArtifactsAsync` to compute and format both diff and patch in a
single native pass, returning only the two output strings across N-API.

| Layer | Responsibility |
| --- | --- |
| Extension TypeScript | Permission roots, allowed symlinks, canonical paths, query validation, edit semantics, multi-query preparation, read receipts, presentation |
| `index.cjs` | Native loading, boundary validation, and stable JavaScript error classification |
| `src/lib.rs` | N-API tasks, owned input buffers, atomic cancellation handle, public diff boundary |
| `src/diff.rs` | Pure native line-diff computation and compact patch formatting |
| `src/filesystem.rs` | Process-local per-path mutation serialization and portable result types |
| `src/evidence.rs` | Async Awareness v1 fingerprints, streaming aggregate budgets, deadlines and final source rechecks |
| `src/git_object.rs` | Async bounded loose Git object decoding, SHA-1 integrity, private directory creation and flush barriers |
| `platforms.cjs` | Single supported-target catalog for loading, builds and package metadata |
| `src/windows.rs` | Handle-relative NT opens, reparse rejection, bounded snapshots, ACL-preserving replacement and deletion |
| `src/unix.rs` | Component-by-component no-follow traversal, bounded snapshots, replacement/deletion, durability receipts |

The JavaScript boundary converts recognized native failure prefixes into
`NativeOperationError` values. Consumers branch on `NativeErrorCodes` through
`nativeErrorCode`; message text remains diagnostic and is not a control-flow
contract. This keeps N-API transport details inside this package.

Snapshots bind the canonical path, every existing ancestor's device/inode, target
metadata, and SHA-256 digest. Missing targets are bound to their existing ancestor
chain. Hash-only snapshots stream without retaining content; content snapshots
return a Buffer. The caller supplies an explicit byte ceiling; growing reads stop
at that ceiling. Metadata and leaf identity are checked around the read.

Awareness uses the same snapshots and mutation receipts for local history. Its
memory evidence operation hashes declared paths, permission modes, lengths and
bytes using the existing v1 format and JavaScript UTF-16 path ordering. The
native task streams at most 64 KiB at once and retains descriptors for a final
source/ancestor recheck. File, byte, time and cancellation limits return a reason
without a partial fingerprint. Its deadline begins before worker enqueueing;
the host adds canonicalization to the same budget and keeps native initialization
outside the filesystem deadline. This is evidence freshness, not verification.

Private Git reads use `readGitObject`: compressed input and decoded body each have
an explicit ceiling; the zlib decoder uses two fixed 64 KiB buffers and a 64-byte
header cap. Only a valid canonical type/decimal-length header within the caller's
limit can allocate retained content. Verification mode retains no body. Decoding
checks exact length, zlib completion, trailing bytes, and the requested SHA-1,
then rechecks the pinned file and ancestors. Cancellation and deadline checks run
between chunks and include worker queue time. A blocked operating-system read or
flush itself cannot be interrupted. Missing loose objects report an explicit
unavailable/packed-unsupported error; there is no packfile or unbounded fallback.

`ensurePrivateDirectory` creates missing parents with the platform's private
permissions through the same no-follow traversal. `flushFile` pins an existing
regular file, flushes data and (on Unix) ancestor directories, then rechecks source
identity. Windows flushes a matching writable handle and reports the directory
persistence limitation. Neither primitive authorizes paths for the caller.

Writes revalidate before creating parents, after preparation, and before commit.
On Unix, each missing directory is opened with `O_DIRECTORY | O_NOFOLLOW`. An exclusive
temporary is created with its initial permission mode before bytes are written.
Existing permission bits are retained; an explicit private mode overrides them.
Default new files honor the process umask. Data is flushed
before descriptor-relative rename, or create-only `linkat` for absent targets.
Only owned temporaries get cleanup guards. Parent sync failures after commit are
warnings with `committed: true` and `durable: false`. Cancellation is checked
before commit and never rewrites a committed outcome into an abort.
An optional parent mode of 0700 creates missing history directories privately;
existing parent permissions are unchanged. On Windows this requests a protected
caller/SYSTEM DACL for each directory that the operation creates.

Delete snapshots can explicitly inspect a leaf symlink with `readlinkat`; unlink
removes that link itself. Replacement never accepts a leaf symlink. TypeScript
may resolve permitted write/edit symlinks before calling native code; Rust then
rejects traversal through any symlink in that supplied canonical path.

The algorithms are adapted from the local Octocode Agent filesystem service.
Its single-workspace policy, private checkpoint journal, subprocess protocol,
base64 transport, and application lifecycle are deliberately excluded. Native
code accepts a canonical absolute path and does not independently authorize it.

Limits are explicit: process-local serialization and snapshot checks are
cooperative preconditions, not an operating-system compare-and-swap against
arbitrary external writers. A final snapshot-to-rename/unlink gap remains;
descriptor pinning prevents symlink redirection but does not prevent another
process moving an already-open directory. Recursive creation can leave empty
parents after cancellation. Atomic replacement can change ownership, extended
attributes and hard-link relationships. Unix preserves permission bits, not ACLs.
Windows copies and protects the existing effective DACL, freezing future parent
ACL inheritance on the replacement, unless explicit private mode 0600 requests
a protected owner/SYSTEM DACL; other explicit POSIX modes are rejected. New
Windows files inherit the parent DACL by default. Windows uses its conventional
case-insensitive namespace and rejects alternate streams, device names and
trailing-dot/space aliases. It flushes file data but reports `durable: false`
because Windows has no portable directory-entry persistence contract. Successful `fsync` provides the filesystem's durability contract,
not proof against hardware power loss. There is no multi-file transaction.

Package tests exercise real addon calls, digest/content equality, metadata and
ancestor replacement, symlink policy, FIFO rejection, bounded IO, cancellation,
competing preconditions, mode retention, loader failure, and diff reconstruction.
Rust tests directly verify exclusive-temp ownership, mode at creation, and
post-commit receipt semantics.

Development builds publish addons via a unique temporary plus atomic rename.
Rebuilding never truncates a binary inode that another active host has mapped.

The Windows boundary follows Microsoft contracts for [NtCreateFile](https://learn.microsoft.com/en-us/windows/win32/api/winternl/nf-winternl-ntcreatefile),
[relative rename information](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/ns-ntifs-_file_rename_information)
and [handle security information](https://learn.microsoft.com/en-us/windows/win32/api/aclapi/nf-aclapi-getsecurityinfo).
