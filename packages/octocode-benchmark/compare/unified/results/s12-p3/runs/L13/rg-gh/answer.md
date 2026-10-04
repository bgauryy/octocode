**Answer:** GET on an expired string key returns a null reply (`$-1` in RESP2, `_` in RESP3), as if the key didn't exist. On a master, the key is also deleted on that access (lazy expiry) and the deletion is propagated. On a read-only replica, the key is not deleted, but the reply is still null.

**Evidence (commit 20bb2cfc54):**
- `getCommand` calls `getGenericCommand` (`src/t_string.c:470-472`). That calls `lookupKeyReadOrReply(c, c->argv[1], shared.null[c->resp])` (`src/t_string.c:459`). If the lookup returns NULL, the null reply has already been sent and the function returns (`src/t_string.c:459-460`).
- `lookupKeyReadOrReply` calls `lookupKeyRead`, which uses `LOOKUP_NONE` (`src/db.c:373-375, 402-405`). If the result is NULL, it sends the null reply (`src/db.c:404`).
- `lookupKey` finds the entry and calls `expireIfNeeded`. Anything other than `KEY_VALID` sets `val = NULL` (`src/db.c:319-322`).
- `expireIfNeeded` (`src/db.c:3004`) goes through these cases for an expired key:
  - **Replica:** with `server.masterhost` set and no `EXPIRE_FORCE_DELETE_EXPIRED`, it returns `KEY_EXPIRED` without deleting. The replica waits for the master's DEL. GET does not set `LOOKUP_WRITE`, so it never forces deletion. If the client is the master link, the key is treated as valid.
  - **Deletion disabled or paused:** it returns `KEY_EXPIRED` without deleting if `confAllowsExpireDel()` is false, or if expire actions are paused (`isPausedActionsWithUpdate(PAUSE_ACTION_EXPIRE)`).
  - **Master (normal case):** it calls `deleteExpiredKeyAndPropagate` and returns `KEY_DELETED`. That function calls `deleteKeyAndPropagate(db, keyobj, NOTIFY_EXPIRED, NULL)` (`src/db.c:2898-2900`).
- Stats and events: on the miss path, `lookupKey` fires a `keymiss` keyspace event and increments `stat_keyspace_misses` (`src/db.c:346-350`). The `expired` notification comes from the deletion itself.

**Uncertainty:** I did not open `deleteKeyAndPropagate` itself. The `expired` event and the DEL/UNLINK propagation come from its signature, its call site and the header comments (`src/db.c:2907-2925`). Whether DEL or UNLINK is sent depends on the lazyfree configuration, which I did not check.