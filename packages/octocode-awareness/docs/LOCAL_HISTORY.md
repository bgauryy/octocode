# LocalGit history

LocalGit stores optional recoverable workspace bytes. It is evidence for inspection and recovery, not the source of coordination truth, actor intent, or verification.

## Ownership

The host owns capture through `@octocodeai/octocode-awareness/host`. Routine agents cannot create captures or checkpoints. Their History surface is limited to:

- `history.status`: report capability, resolved storage paths, and recovery pressure.
- `history.timeline`: list bounded recoverable operations.
- `history.read`: read one exact historical version.
- `history.restore`: preview or apply one bound restore.

SQLite stores operation, version, restore, and durability metadata. Immutable file objects live under `<workspace>/.octocode/.localGit`, partitioned by canonical store and physical workspace identity. A new database schema generation receives a new store ID and LocalGit namespace; older database and namespace pairs remain separate until explicitly migrated or removed. The LocalGit layout label reported by `history.status` is independent of the database schema generation. Read exact paths from `history.status`; do not derive them.

The LocalGit backend does not modify the repository index, `HEAD`, branches, remotes, configuration, or hooks. It does not require network access.

## Capture

A host records before or after file state at a trusted mutation or lifecycle boundary. Capture publishes immutable objects and refs, then records durability in SQLite. Missing, unstable, omitted, and unknown file states remain explicit.

A routine edit does not need a durable evidence point unless the host lifecycle policy calls for one. Captured bytes must not be copied into Messages or Memory when a compact operation, file, side, and digest reference is sufficient.

## Read

Use `history.timeline` to find an operation, then `history.read` for the required file and side. Follow every executable continuation before treating a bounded response as complete. Decode the declared encoding before comparing bytes.

Recheck the current workspace independently. A historical digest establishes the retained version, not the correctness of the current file.

## Retention and reclamation

`history.status` reports automatic object pruning as disabled. LocalGit objects are never removed by a background task. It also reports whether confirmed manual reclamation is available, the one-hour minimum grace period, and the per-call object and byte bounds.

`history evidence` is an operator-only CLI command; it is intentionally absent from the routine agent/API/Pi operation catalog. It defaults to `action: report`. Report mode is a dry run: it lists grace-aged loose objects that are unreachable from every SQLite history object ID and every LocalGit ref, but it does not delete them. The default grace period is one day; callers may select any value from one hour through one year. Run `history evidence --help` for its schema-derived flags.

Reclamation is deliberately explicit:

1. Run `history evidence` with `action: report`, inspect every page, and resolve any terminal diagnostic.
2. Run the same selection with `action: reclaim` and `confirm: reclaim`.
3. Follow the returned executable continuation while `partial` is true. A call removes at most the requested `limit` (maximum 100 objects) and at most 64 MiB of loose-object storage.

Each reclaim call takes the Awareness metadata maintenance handshake before scanning. It refuses to run while a file capture is in `capturing` state or a sealed experience archive is still pending publication. While the handshake is held, new capture and seal transitions cannot begin; durable experience archive receipts also retain their recorded commits and descendants. Reachability is recomputed on every page, so a report is never treated as deletion authorization or as a stable deletion plan.

Deletion uses the native snapshot version as a compare-and-delete precondition. A changed object, unsafe path, scan bound, or durability limitation remains explicit in the result. `objects`, `reclaimed_objects`, and `reclaimed_bytes` describe only deletions that committed; a terminal diagnostic has no continuation and requires operator inspection. Reclamation scans loose objects only and does not run Git garbage collection or alter pack files, the workspace repository, refs, index, `HEAD`, branches, remotes, configuration, or hooks.

## Restore

Restore has two phases:

1. Call `history.restore` with `action: preview`, the source operation, side, and selected files.
2. Inspect the returned paths, expected current digests, target states, expiry, and protection pressure.
3. After authorization, call `history.restore` with `action: apply` and the exact preview ID.

Apply fails closed if the preview expired or its workspace, actor, path, digest, mode, or protection binding changed. It captures undo evidence before writing. A multi-file apply can report a partial outcome because SQLite and workspace files cannot form one transaction.

Every applied restore returns verification debt. Run the applicable checks and record the observed result with `work.verify`; restored bytes do not prove a successful repair.

## Migration and backup

Back up the canonical SQLite database and every matching LocalGit namespace together. Database migration preserves the stable store ID and verifies available object reachability. It does not silently relocate an unknown or incomplete sidecar.

Retain the migration source and its LocalGit data until the destination has passed independent verification and the cutover has been exercised. See [Database migration](DB.md#database-migration).
