**Short answer:** On a master, GET returns a null reply (nil). Before replying, it lazily deletes the expired key and propagates a DEL to replicas and the AOF. On a read-only replica, GET also returns nil, but the key is not deleted.

**Call path**
- `getCommand` calls `getGenericCommand` (`src/t_string.c:456`). That calls `lookupKeyReadOrReply(c, key, shared.null[c->resp])`. If the lookup returns NULL, the null reply is sent and the function returns (`src/t_string.c:459-460`).
- `lookupKeyReadOrReply` calls `lookupKeyRead` and sends the supplied reply when the result is NULL (`src/db.c:402-405`). I did not open `lookupKeyRead`. I'm assuming it passes through to `lookupKey`.
- `lookupKey` finds the entry and calls `expireIfNeeded` (`src/db.c:319`). If that returns anything other than `KEY_VALID`, `lookupKey` sets `val = NULL` (`src/db.c:319-322`).
- A miss increments `stat_keyspace_misses` and fires a `keymiss` event unless flags suppress them (`src/db.c:346-351`).

**Inside `expireIfNeeded` (`src/db.c:3004`)**
- If the key is expired, a master deletes it. It calls `deleteExpiredKeyAndPropagate` and returns `KEY_DELETED` (`src/db.c:3059-3068`).
- `deleteExpiredKeyAndPropagate` (`src/db.c:2898`) calls `deleteKeyAndPropagate` (`src/db.c:2847`). That function does these things:
  - It deletes the key with `dbGenericDelete`, lazily if `lazyfree_lazy_expire` is set.
  - It fires the `expired` keyspace notification.
  - It calls `keyModified`.
  - It calls `propagateDeletion`, which sends a DEL or UNLINK to replicas and the AOF.
  - It increments `stat_expiredkeys`.
- On a replica (`masterhost != NULL`), `expireIfNeeded` returns `KEY_EXPIRED` without deleting (`src/db.c:3036-3041`). It waits for the master's DEL. The client still gets nil.
- It also returns `KEY_EXPIRED` without deleting in these cases:
  - `confAllowsExpireDel()` is false (`src/db.c:3045`).
  - The `EXPIRE_AVOID_DELETE_EXPIRED` flag is set (`src/db.c:3049-3050`).
  - The expire action is paused (`src/db.c:3054`).
- If the client is the master link (`CLIENT_MASTER`), keys are never treated as expired (`src/db.c:3037`).

**Uncertainty:** I couldn't run `git rev-parse` because `/dev/null` is blocked in this sandbox. I read the files as checked out and did not confirm they are at 20bb2cf. I didn't read `keyIsExpired`, `lookupKeyRead` or `confAllowsExpireDel`.