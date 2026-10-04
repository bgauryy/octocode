On a master, GET on an expired string key returns a null reply (nil in RESP2, `_` in RESP3). It also lazily deletes the key and propagates that deletion. A read-only replica returns the null reply but doesn't delete the key.

**Call path**
- `getCommand` calls `getGenericCommand` (`src/t_string.c:456-472`). That calls `lookupKeyReadOrReply(c, c->argv[1], shared.null[c->resp])` (`src/t_string.c:459`), and the helper is defined at `src/db.c:402`.
- If the lookup returns NULL, the null reply is sent and the command returns `C_OK`. The `checkType` and `addReplyBulk` steps never run.
- The lookup goes through `lookupKey` (`src/db.c:~296`). It finds the entry, then calls `expireIfNeeded(db, key, val, expire_flags)` (`src/db.c:319`). If that doesn't return `KEY_VALID`, `val` is set to NULL (`src/db.c:319-322`). I didn't open `lookupKeyReadWithFlags` (`src/db.c:366`), so I'm assuming it forwards to `lookupKey`.

**Inside `expireIfNeeded` (`src/db.c:3004`)**
- If `keyIsExpired` is false, or `EXPIRE_ALLOW_ACCESS_EXPIRED` is set, it returns `KEY_VALID`. Neither applies to an expired key under a plain GET.
- On a replica (`server.masterhost != NULL`), it returns `KEY_EXPIRED` without deleting, because the master drives expiry. GET sets no force-delete flag, and read-only replicas never force deletion (`src/db.c:~305`).
  - The exception is a request from the master itself (`CLIENT_MASTER`), where the key is treated as valid.
- On a master it can also return `KEY_EXPIRED` without deleting in two cases. One is when `confAllowsExpireDel()` is false. The other is when the expire action is paused (`isPausedActionsWithUpdate(PAUSE_ACTION_EXPIRE)`).
- Otherwise it calls `deleteExpiredKeyAndPropagate` (`src/db.c:3065-3069`) and returns `KEY_DELETED`.
  - That function calls `deleteKeyAndPropagate(..., NOTIFY_EXPIRED, ...)` (`src/db.c:2898`).
  - The deletion is propagated to replicas and the AOF as DEL or UNLINK (`propagateDeletion`, `src/db.c:~2915`).
  - It also fires the `expired` keyspace notification. I inferred this from the `NOTIFY_EXPIRED` argument and didn't read `deleteKeyAndPropagate`.

**Side effects on a miss**
- The `keymiss` keyspace event fires unless `LOOKUP_NONOTIFY` is set (`src/db.c:347-348`).
- `stat_keyspace_misses` is incremented unless `LOOKUP_NOSTATS` is set (`src/db.c:349-350`).

**Uncertainty**
- I didn't run the code. The line numbers marked `~` are approximate.
- I didn't read `lookupKeyReadOrReply`, `lookupKeyReadWithFlags` or `deleteKeyAndPropagate` in full.
- The flags GET passes to `lookupKey` are inferred from the code above.