**Answer:** GET treats the expired key as missing and replies with a null. On a master, this lookup also deletes the key and propagates a DEL. This is from reading the code. I didn't run it.

**Flow**
1. `getGenericCommand` calls `lookupKeyReadOrReply(c, c->argv[1], shared.null[c->resp])`. If that returns NULL, it replies with the shared null (RESP2 null bulk, RESP3 null) and returns. The function is in `src/t_string.c` at about lines 468–478. That range is approximate because I only saw the surrounding match window. I did not open `lookupKeyReadOrReply` itself, so I'm assuming it goes through `lookupKeyRead`.
2. `lookupKey()` in `src/db.c` finds the entry, then calls `expireIfNeeded(db, key, val, expire_flags)`. GET does not use `LOOKUP_WRITE`, so `expire_flags` has no `EXPIRE_FORCE_DELETE_EXPIRED`. If the result is not `KEY_VALID`, it sets `val = NULL`.
3. With `val` NULL, `lookupKey` takes the miss branch, because GET sets none of `LOOKUP_NONOTIFY`, `LOOKUP_WRITE` or `LOOKUP_NOSTATS`. It fires a `keymiss` keyspace event and increments `stat_keyspace_misses`. I saw this in the `lookupKey` source but did not check the line numbers.
4. In `expireIfNeeded`, `src/db.c:3004`, the outcome depends on the instance's role and state:
   - **Master (default):** the key is expired, `deleteExpiredKeyAndPropagate(db, key)` runs, and the function returns `KEY_DELETED`. This deletes the key and propagates a DEL to the AOF and replicas.
   - **Replica:** if `server.masterhost != NULL` and `EXPIRE_FORCE_DELETE_EXPIRED` isn't set, it returns `KEY_EXPIRED` without deleting. GET still sees a miss, and the replica waits for the master's DEL.
   - **Command from the master link:** if `server.current_client` has `CLIENT_MASTER`, it returns `KEY_VALID`, so keys are never treated as expired.
   - **Expiry deletion disabled or paused:** if `confAllowsExpireDel()` is false, or the expire action is paused, it returns `KEY_EXPIRED` and doesn't delete. GET still sees a miss.

**Uncertainty:** I read `expireIfNeeded` and `lookupKey`, but not `keyIsExpired`, `deleteExpiredKeyAndPropagate` or `lookupKeyReadOrReply`. The exact `t_string.c` line numbers are approximate.