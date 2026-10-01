On a master, GET on an expired string key returns a null reply (nil), exactly as if the key didn't exist. As a side effect, the key is deleted from the database and the deletion is propagated.

**Flow**
1. `getGenericCommand` calls `lookupKeyReadOrReply(c, c->argv[1], shared.null[c->resp])` and returns immediately if the result is NULL (`src/t_string.c:~460`). That reply is the null for the client's RESP version.
2. `lookupKey` (`src/db.c:296`) finds the entry, then calls `expireIfNeeded(db, key, val, expire_flags)` (`src/db.c:319`). GET is a read, so `LOOKUP_WRITE` is not set and `EXPIRE_FORCE_DELETE_EXPIRED` is not added. If the result isn't `KEY_VALID`, `val` is set to NULL.
3. `expireIfNeeded` (`src/db.c:3004`) returns `KEY_VALID` only if the key isn't logically expired. On a master with no special flags, it reaches the "Perform deletion" step. There it calls `deleteExpiredKeyAndPropagate(...)` and returns `KEY_DELETED`. The comments above the function say this deletion may propagate a DEL or UNLINK to the AOF and replicas.
4. Back in `lookupKey`, the miss branch (`src/db.c:~345`) runs:
   - `notifyKeyspaceEvent(NOTIFY_KEY_MISS, "keymiss", ...)` fires, unless `LOOKUP_NONOTIFY` is set.
   - `stat_keyspace_misses++` runs, unless `LOOKUP_NOSTATS` is set.

**Cases where the key is not deleted**
- **Read-only replica:** it returns `KEY_EXPIRED`, so the client still gets nil, but the key isn't deleted. The replica waits for the master's DEL. If the current client is the master link, the key is treated as valid.
- **Lazy-expire deletion disabled:** `confAllowsExpireDel()` is false, so it returns `KEY_EXPIRED` and the key is not deleted.
- **Expire actions paused:** `isPausedActionsWithUpdate(PAUSE_ACTION_EXPIRE)` returns `KEY_EXPIRED` and the key is not deleted.
- **Key in a slot-trim job:** it returns `KEY_TRIMMED` (or `KEY_VALID` if trimmed access is allowed).

**Not verified:** I didn't read `lookupKeyReadOrReply`, `deleteExpiredKeyAndPropagate` or `keyIsExpired` bodies. I also didn't check whether GET is called with any non-default lookup flags, which would change the above.