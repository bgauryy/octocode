On a master, GET on an expired string key returns a null reply (`shared.null[c->resp]`, RESP2 nil or RESP3 null). Along the way it deletes the key lazily, fires an expired event and propagates a DEL. On a read-only replica it returns the same null reply but does not delete the key.

**Path**
- `getCommand` calls `getGenericCommand` (`src/t_string.c:470-472`).
- `getGenericCommand` calls `lookupKeyReadOrReply(c, c->argv[1], shared.null[c->resp])`. If that returns NULL, it returns immediately, so the client gets the null reply (`t_string.c:459-460`).
- I did not read the body of `lookupKeyReadOrReply`. That it ends up in `lookupKey` is inferred from the `lookupKey` code below, not shown directly.
- `lookupKey` finds the entry and calls `expireIfNeeded` (`src/db.c:298`, `db.c:319`). If the result is not `KEY_VALID`, it sets `val = NULL` (`db.c:319-322`).
- With `val` NULL, `lookupKey` takes the miss branch. It fires a `keymiss` keyspace event and increments `stat_keyspace_misses` (`db.c:346-350`). I only saw the first lines of that branch, up to line 350.

**Expiry check and deletion**
- `keyIsExpired` returns true when `now > when` (`db.c:2954`). `now` is `commandTimeSnapshot()`, so it is not a fresh clock read (`db.c:2951`). It always returns false while loading or when `allow_access_expired` is set (`db.c:2948`).
- On a master, `expireIfNeeded` takes the deletion path (`db.c:3063-3072`):
  - It calls `deleteExpiredKeyAndPropagate` and returns `KEY_DELETED`.
  - The delete helper's visible tail does the following (`db.c:2898-2906`):
    - It fires the expired keyspace notification.
    - It calls `keyModified`.
    - It calls `propagateDeletion`, which sends a DEL or UNLINK to the AOF and replicas.
    - It increments `stat_expiredkeys`.
  - I did not read the earlier part of that function (before line 2898).

**Cases where the key is not deleted**
- On a replica without a write flag, it returns `KEY_EXPIRED` and leaves the key in place, waiting for the master's DEL (`db.c:3045`). GET therefore still returns null.
- A client with the `CLIENT_MASTER` flag (the replication link) never sees keys as expired (`db.c:3044`).
- It returns `KEY_EXPIRED` without deleting when `confAllowsExpireDel()` is false (`db.c:3050-3051`). GET does not set `EXPIRE_FORCE_DELETE_EXPIRED`.
- It also returns `KEY_EXPIRED` without deleting when the `LOOKUP_NOEXPIRE` flag is set (`db.c:3055-3056`). GET does not set that flag either.
- It also returns `KEY_EXPIRED` without deleting when the expire action is paused (`db.c:3061`). The caller still sees NULL, so GET still replies null.

**Uncertainty:** I read `lookupKey` and `expireIfNeeded` directly, but not `lookupKeyReadOrReply` or the head of `deleteKeyAndPropagate`. The lazy-free choice between DEL and UNLINK is therefore unverified.