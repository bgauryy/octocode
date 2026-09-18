# Exclusive path protection

Ordinary overlap is advisory. Use `work.protect` only when concurrent changes cannot merge safely, such as a database migration, generated singleton, dependency lockfile, or broad mechanical rewrite.

## Protocol

`work.protect` has three actions:

| Action | Meaning |
|---|---|
| `acquire` | Protect exact paths with a rationale and bounded lease |
| `wait` | Observe the owner until release or expiry |
| `release` | Remove only protection owned by the caller |

Before acquiring protection, inspect the path with `work.show`. If another actor is active, use `message.send` to resolve ownership or compatibility. Never bypass active peer protection.

Protection is not permission to edit. The caller still needs user or host authorization for the underlying mutation.

## Completion

Lease expiry removes coordination protection but does not prove the owner finished or abandoned the change. Inspect the current file and Work state before reacquiring.

After editing:

1. Run the declared check.
2. Update the owning Work attempt.
3. Record the observed result with `work.verify`.
4. Release owned protection when it no longer guards an active mutation.

Do not infer success from release, expiry, absence from the workboard, or a peer Message.

Host mutation guards use the same protection state. A real conflict blocks the write; infrastructure failure follows the separate degraded fail-open policy described in [Host lifecycle hooks](HOOKS.md).
