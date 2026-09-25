# Lock identity decision

Agents need the same answer to “does another live lease cover this path?” across
Rust, Claude, Codex, Pi, and SQLite-only integrations, including before file creation.
The lock service must not create placeholder files or depend on a model deciding
whether two paths overlap.

## Options considered

| Option | Benefit | Cost / decision |
| --- | --- | --- |
| Native filesystem comparison rules | Allows distinct case-sensitive names to proceed independently. | Volume/directory flags and Unicode versions differ; every DB client would need platform-specific logic. Defer. |
| One canonical caseless lease namespace | Same deterministic comparison in every client; handles absent names. | Case/normalization aliases conflict even when the filesystem distinguishes them. **Selected.** |
| Reserve the nearest existing parent for every new path | Avoids guessing missing filename identity. | Serializes unrelated new files in one directory. Too broad for normal agent work. |
| OS locks or a separate broker | Can enforce some process-level ownership rules. | Does not preserve the SQLite-only contract or solve arbitrary path/tree aliases by itself. Outside this repair. |

The architectural priority is exclusion with a small interoperable contract. The
product tradeoff is explicit conservative conflicts for case/normalization aliases;
ordinary unrelated paths remain independent. Avoiding another daemon or routing
model keeps the skill usable by generic agents.

## Chosen rules

1. Resolve the workspace and path against the real filesystem. Follow each symlink
   before processing a subsequent `..`; keep nonexistent suffixes as logical paths.
   Bound link traversal and propagate permission, cycle, and traversal errors.
2. Check workspace containment using the case-preserving resolved path. Folded
   comparison keys never authorize filesystem access or an escape from the workspace.
3. Compare lease components using Unicode 16 canonical caseless matching:
   `NFD(casefold(NFD(component)))`. Preserve the stored spelling. Pin the Rust data
   versions; the Python reference requires matching Unicode 16 for path leases.
4. A file lease conflicts at an equal path. A tree lease also conflicts with
   descendants. Comparison respects component boundaries, not raw string prefixes.
5. Acquire under `BEGIN IMMEDIATE`: validate active presence, inspect active leases,
   reject an overlap, or insert an acquisition ID with expiry measured after the
   writer transaction is acquired. Never keep the transaction open during a model
   call or while waiting for another agent.
6. Renew/unlock require the original acquisition ID and live owner/lease. A failed
   renewal means stop writing. Expired leases are never revived. Existing same-owner
   overlaps still conflict; callers must retain and renew their existing lease.

Lease inspection uses the exact same overlap function as acquisition, through a
connection-local SQLite function. The SQL schema is unchanged. All participants
must upgrade together after old workers stop and their leases are released or their
presence expires; the earlier comparison algorithm is not compatible with the new
one for mixed-client acquisition.

## Limits kept explicit

These are cooperative path reservations, not mandatory OS locks. They do not fence
an uncooperative writer, alias hard links, or freeze directory/symlink topology.
Use a tree reservation when changing directory structure; reacquire after a rename
or symlink change. Monotonic acquisition IDs prevent stale renew/unlock calls from
altering a newer lease, but cannot stop an arbitrary write after expiry.

The selected policy is intentionally stronger than case-sensitive filename equality.
It is a communication contract, not a claim to emulate every filesystem's Unicode,
mount, or naming semantics. Only macOS ARM64 has live vendor validation here.

## Evidence and validation

The old implementation failed regressions for symlink-plus-parent traversal and
missing-file case aliases. The repaired tests cover those paths, dangling links,
cycles, workspace escapes, canonical Unicode aliases, tree boundaries, concurrent
case-alias contenders, and both directions of Python/Rust conflict checks.

- [POSIX pathname resolution](https://pubs.opengroup.org/onlinepubs/9699919799/basedefs/V1_chap04.html#tag_04_13): link contents are resolved with the remaining path.
- [Apple APFS filename behavior](https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/APFS_Guide/FAQ/FAQ.html): case and normalization behavior vary; ASCII lowercase is insufficient.
- [Rust canonicalize](https://doc.rust-lang.org/std/fs/fn.canonicalize.html): resolves links for existing paths; missing paths require deliberate handling.
- [caseless implementation](https://docs.rs/caseless/0.2.2/src/caseless/lib.rs.html): canonical caseless comparison applies normalization and full case folding.
- [Agent Mail guard](https://github.com/Dicklesworthstone/mcp_agent_mail/blob/main/src/mcp_agent_mail/guard.py): inspected prior art uses case detection for guard matching; this is evidence of a different policy, not proof of its lease guarantees.
