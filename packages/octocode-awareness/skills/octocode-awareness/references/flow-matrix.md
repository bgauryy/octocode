# Awareness Flow Matrix

Use the live schema for exact fields: `schema commands --compact` and `schema command <concept> <operation> --compact`.

| Need | Operation | Boundary |
|---|---|---|
| Orient | `context.orient` | Read only what can change the next action. |
| Coordinate | `message.list/send/reply/resolve` | Send only decision-changing information. |
| Share ownership | `work.create/list/show/claim/update/depend` | Track only work needing shared ownership, dependencies, or resumption. |
| Protect a path | `work.protect` | Exceptional non-mergeable work only. |
| Close verification debt | `work.verify` | Mark only an observed check result. |
| Reuse evidence | `memory.recall/record` | Store verified lessons, not status. |
| Recover bytes | `history.status/timeline/read/restore` | Capture is host-owned; apply only an authorized preview. |

Reads are observational. Mutations require the caller's existing authorization and scope.
