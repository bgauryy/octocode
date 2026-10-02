On a master, GET on an expired string key returns a null reply (nil), as if the key never existed. As a side effect, the key is lazily deleted and the deletion is propagated. I did not read `deleteKeyAndPropagate`, the lookup code outside `src/db.c:316-352`, or the `lookupKeyReadOrReply` definition. The propagation and the null reply therefore rest on the comments in `src/db.c`, the call in `src/t_string.c:459` and the `lookupKey` snippet.

**Flow**
- `getCommand` calls `getGenericCommand` (`src/t_string.c:470-472`).
- That calls `lookupKeyReadOrReply(c, c->argv[1], shared.null[c->resp])`. If the result is NULL, it returns with the null reply and never reaches the type check or `addReplyBulk` (`src/t_string.c:459-460`).
- The lookup path calls `expireIfNeeded(db, key, val, expire_flags)`. If the result is not `KEY_VALID`, it sets `val = NULL` (`src/db.c:319-322`).

**Expiry check and deletion in `expireIfNeeded` (`src/db.c:3004-3073`)**
- `keyIsExpired` treats the key as expired when `now > when`, using `commandTimeSnapshot()` (`src/db.c:2951-2954`). It never reports expiry while loading or when `server.allow_access_expired` is set (`src/db.c:2948`).
- On a master with default flags, it reaches `deleteExpiredKeyAndPropagate` and returns `KEY_DELETED` (`src/db.c:3064-3072`). That helper calls `deleteKeyAndPropagate(db, keyobj, NOTIFY_EXPIRED, NULL)` (`src/db.c:2898-2900`). The function's header comment says this may propagate a DEL/UNLINK to the AOF and replicas (`src/db.c:2981-2983`).

**Miss accounting**
- A miss fires the `keymiss` keyspace event unless `LOOKUP_NONOTIFY` or `LOOKUP_WRITE` is set (`src/db.c:347-348`).
- It also increments `server.stat_keyspace_misses` unless `LOOKUP_NOSTATS` or `LOOKUP_WRITE` is set (`src/db.c:349-350`).

**Cases where the key is not deleted**
- **Replica:** the key is not deleted, but `KEY_EXPIRED` is returned, so GET still replies nil. If the client is the master link (`CLIENT_MASTER`), the key counts as valid (`src/db.c:3043-3046`).
- **Cluster mode:** the same `CLIENT_MASTER` check applies, so a key read by the master-link client is treated as valid (`src/db.c:3043-3044`).
- **Config forbids lazy-expire deletion** (`confAllowsExpireDel`): `KEY_EXPIRED` is returned with no deletion (`src/db.c:3050-3051`).
- **`EXPIRE_AVOID_DELETE_EXPIRED` flag:** `KEY_EXPIRED` is returned with no deletion (`src/db.c:3055-3056`).
- **Expire action paused:** `KEY_EXPIRED` is returned with no deletion (`src/db.c:3061`).
- **`EXPIRE_ALLOW_ACCESS_EXPIRED` flag:** the value is returned as if valid (`src/db.c:3021-3023`).