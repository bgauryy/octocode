**Short answer:** On a normal master, GET on an expired string key deletes the key lazily and returns a null reply, as if the key never existed. On a read-only replica, GET also returns null but does not delete the key. I read the code and did not run it.

**Call path**
- `getCommand` calls `getGenericCommand` (`src/t_string.c:456-472`). That calls `lookupKeyReadOrReply(c, argv[1], shared.null[c->resp])` (`src/t_string.c:459`).
- If the lookup returns NULL, the client gets `shared.null[c->resp]`. That is a nil bulk reply (`$-1`) in RESP2 and `_` in RESP3. The type check and `addReplyBulk` are never reached.
- `lookupKeyReadOrReply` calls `lookupKeyRead`, which calls `lookupKey` with `LOOKUP_NONE` (`src/db.c:402-404`, `src/db.c:~395`).
- `lookupKey` finds the entry and then calls `expireIfNeeded` (`src/db.c:319`). If that returns anything other than `KEY_VALID`, `val` is set to NULL.

**Expiry check**
- `keyIsExpired` (`src/db.c:2946`) compares `commandTimeSnapshot()` with the key's expire time. The key counts as expired when `now > when`.
- It always returns 0, meaning not expired, while the server is loading or when `allow_access_expired` is set.

**What `expireIfNeeded` does** (`src/db.c:3004`)
- **Master:** it calls `deleteExpiredKeyAndPropagate` and returns `KEY_DELETED`. That function (`src/db.c:2898`) goes through `deleteKeyAndPropagate` with `NOTIFY_EXPIRED`. That in turn:
  - deletes the key;
  - fires an `expired` keyspace event;
  - calls `keyModified`;
  - propagates a DEL, or UNLINK if lazy-free is in use, to the AOF and replicas via `propagateDeletion` (`src/db.c:~2924`);
  - increments `stat_expiredkeys` (`src/db.c:~2878-2883`).
- **Read-only replica:** `src/db.c:3036-3039` returns `KEY_EXPIRED` without deleting, because the master drives expiry. GET still returns null. If the client is the master link (`CLIENT_MASTER`), the key is treated as valid.
- **Cases where a master returns `KEY_EXPIRED` without deleting:**
  - `confAllowsExpireDel()` is false (`src/db.c:~3043`).
  - The expire action is paused (`src/db.c:~3052`).
- **Key in a slot-trim job:** `KEY_TRIMMED` is returned for it. It also comes back as a miss.

**Stats and events:** because the lookup ends with `val == NULL`, `lookupKey` fires a `keymiss` event and increments `stat_keyspace_misses` (`src/db.c:~345-351`). The keyspace-hit counter is not incremented.

**Uncertainty:** a few `src/db.c` line numbers are approximate, marked with `~`. I saw those regions in output without exact line prefixes. The `lookupKeyRead` location is the least certain. I did not check every code path that could set `allow_access_expired`.