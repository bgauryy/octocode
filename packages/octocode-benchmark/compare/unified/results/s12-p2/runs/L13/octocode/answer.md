**Short answer:** on a normal master, GET on an expired string key lazily deletes the key and replies with a null (nil in RESP2, `_` in RESP3). It also counts as a keyspace miss. The key is treated as missing, with no error and no value returned.

**Call path**
- `getCommand` calls `getGenericCommand`, which calls `lookupKeyReadOrReply(c, c->argv[1], shared.null[c->resp])` (`src/t_string.c:470-471`, `src/t_string.c:459`). If that returns NULL, GET replies with the null object and returns `C_OK` (`src/t_string.c:459-460`).
- I did not open `lookupKeyReadOrReply` itself. I infer it calls `lookupKey` from its name and from the call chain, but that link is unverified.
- `lookupKey` finds the entry and calls `expireIfNeeded(db, key, val, expire_flags)` (`src/db.c:319`). A GET is a read, so `LOOKUP_WRITE` is not set and `EXPIRE_FORCE_DELETE_EXPIRED` is not added (`src/db.c:311-312`).
- If the result is anything other than `KEY_VALID`, `val` is set to NULL (`src/db.c:319-321`).

**Inside `expireIfNeeded` (`src/db.c:3004`)**
- If the key has not expired, or `EXPIRE_ALLOW_ACCESS_EXPIRED` is set, it returns `KEY_VALID` (`src/db.c:3021-3023`).
- On a replica it returns `KEY_EXPIRED` without deleting (`src/db.c:3045`). The replica waits for the master to send a DEL. When the command comes from the master client, the key is treated as valid (`src/db.c:3044`).
- It also returns `KEY_EXPIRED` without deleting in these cases:
  - lazy-expire deletion is disabled by config (`confAllowsExpireDel()`, `src/db.c:3050-3051`);
  - the caller passed `EXPIRE_AVOID_DELETE_EXPIRED` (`src/db.c:3055-3056`);
  - expire actions are paused (`src/db.c:3061`).
- Otherwise it calls `deleteExpiredKeyAndPropagate` and returns `KEY_DELETED` (`src/db.c:3064-3072`). That deletes the key with the `NOTIFY_EXPIRED` event (`src/db.c:2898-2899`). As the comment above `deleteExpiredKeyAndPropagate` explains, a DEL/UNLINK is propagated to replicas and the AOF (`src/db.c:2907-2918`).

**Miss accounting**
Because `val` is NULL, `lookupKey` takes the miss branch (`src/db.c:346-352`). Neither `LOOKUP_NONOTIFY` nor `LOOKUP_WRITE` is set for a plain GET, so it fires the `keymiss` keyspace event and increments `stat_keyspace_misses`. The hit path, which updates LRU/LFU and `stat_keyspace_hits`, is skipped.

**Uncertainty**
I did not check `lookupKeyReadOrReply`, so any extra flags it passes are unverified. I read only the code, not the tests.