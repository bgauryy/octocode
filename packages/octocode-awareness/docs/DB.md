# Awareness database

SQLite is the canonical Awareness coordination store. The schema is versioned, fingerprinted, and validated when opened. An unrecognized or changed schema fails without mutation.

## Ownership

The database owns coordination identity and state:

- Agents and sessions.
- Plans, tasks, dependencies, attempts, files, and claims.
- Exceptional path protection and verification receipts.
- Message threads, reads, delivery cursors, events, and acknowledgements.
- Memories and evidence references.
- Host hook receipts and continuity records.
- LocalGit operation, version, restore, and durability metadata.

LocalGit file objects are not SQLite rows. Back up the database and its matching LocalGit namespace together when History is in use.

Run `npx @octocodeai/octocode-awareness schema entities --all --compact` for the canonical entity inventory. Each relation reports one owner plus machine-readable access, retention, deletion, and `cleanup_operation` policy. That operation is always executable: policy-prunable, lease, event, derived-index, and expiring-preview rows point to `maintenance retention`; intentionally durable rows point to `maintenance store-retire`. `read-write` relations are mutated only through their owner; `derived-index` relations are rebuilt from canonical rows. Shared event infrastructure is identified separately from Context. Session rows retain their ended state. Local-history restore rows combine expiring ready previews with retained terminal receipts, so maintenance deletes only expired ready previews rather than the entire relation. These catalog values feed the operator view instead of being copied there. See [Entity links](ENTITY_LINKS.md) for relationships.

## Store identity

Each canonical store has an application ID, schema version, and stable store ID. The schema generation owns both the metadata version and default filename (`awareness-v5.sqlite3` for the current generation). A breaking DDL change must bump that generation. Default resolution then creates or opens the new generation without mutating the prior file; package releases that do not change DDL continue to share the same generation.

The store ID namespaces LocalGit evidence independently of the database filename. A fresh generation receives a fresh store ID and therefore a separate LocalGit namespace. Previous database and LocalGit pairs remain available for explicit migration or cleanup; they are never merged automatically. Callers must use paths returned by `history.status` instead of deriving a namespace.

The physical database and normalized workspace identity form the coordination boundary. Separate databases never coordinate implicitly. Explicit database paths remain strict: a generation or fingerprint mismatch fails without changing the selected file.

## Database migration

Migration lives only under `@octocodeai/octocode-awareness/admin`. The current migration contract recognizes exact `v3` and `v4` schema-generation predecessors and writes a separate `v5` destination; altered predecessor fingerprints still fail without mutation. Message rows from those generations receive deterministic per-kind expiration during the copy. Migration is a three-stage copy-on-write process:

1. Call `previewDatabaseMigration(sourcePath, destinationPath, options)` with different paths. Preview opens the source read-only, recognizes an exact predecessor schema, validates row classification, and reports every transformation or omission.
2. Call `applyDatabaseMigration` only after reviewing the preview. Apply copies from a read snapshot into a temporary canonical store, checks the source digest, verifies the destination, and publishes a new destination file.
3. Call `verifyDatabaseMigration` independently. Check schema fingerprint, integrity, foreign keys, relation counts, event replay order, store identity, and available LocalGit reachability before cutover.

Stop all writers before cutover. Recheck the source digest, retain the predecessor database and matching sidecars under an explicit rollback name, and then select the verified destination. Do not edit rows or schemas by hand.

Migration classifies predecessor-only follow-up rows into canonical owners. A blocker, handoff, actionable Work item, or verified Memory must be unambiguous; otherwise migration stops before writing a destination. The runtime has no refinement entity.

## Retention

Every new Message has a non-null `expires_at` derived from the canonical per-kind policy. Maintenance resolves elapsed open Messages first, retains them for a separate grace window, and deletes only ancestry-safe rows. Interactions, event delivery/operational rows, locks, terminal standalone runs, superseded Memories, and unapplied restore previews have domain-owned policies. Audit events, authorization receipts, applied restore receipts, and sealed Experience evidence are retained.

Run `maintenance retention` without an action for an aggregate read-only report. Apply requires `--action apply --confirm apply-retention`. Each owner processes at most `--limit` rows in deterministic order; `partial:true` includes an executable continuation with the same `as_of` cutoff. Stale ACTIVE work is reported but can change state only when apply also includes `--fail-stale-active-runs`; the recovery writes a failure receipt.

## Store retirement

Use `maintenance store-retire` only when the whole coordination store is no longer needed. Its default report binds the store ID, recorded workspaces, exact SQLite/sidecar paths, derived LocalGit roots, blockers, and a digest. Save and review that JSON. Confirmed apply requires `--action apply --confirm retire --report-file <reviewed-json>`, rechecks the report, rejects live lifecycle state and concurrent SQLite writers, and renames each exact target to a sibling quarantine path.

Retirement is recoverable, not immediate erasure. Keep writers stopped and rename every returned quarantine path back to its exact source to recover. The service rejects symlinks, roots, home directories, missing workspaces, changed targets, and shared parent directories.

## Transaction boundaries

Domain writes use SQLite transactions. Event publication and its owning domain transition must remain atomic. Busy retry policy comes from the shared SQLite utilities; individual domain modules must not invent retry loops.

History spans SQLite, LocalGit, and workspace files, so a restore cannot be one filesystem-wide transaction. Its preview, protection, undo capture, and verification debt make partial outcomes explicit.
