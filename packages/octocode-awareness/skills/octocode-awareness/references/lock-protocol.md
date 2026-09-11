# Exclusive Path Protection

Normal source and documentation edits are advisory. Use `work.protect` only when concurrent changes cannot be merged safely: migrations, generated singletons, dependency lockfiles, or broad mechanical rewrites.

- `action: acquire` declares exact paths, rationale, and bounded TTL.
- `action: wait` observes protection; it does not prove peers finished.
- `action: release` removes only protection owned by the caller.

Before acquisition, inspect the path with `work.show` and coordinate through `message.send` when another actor is present. Never bypass an active peer protection. Expiry removes coordination state; it never proves completion. After editing, run the declared check and record its observed result with `work.verify`.
