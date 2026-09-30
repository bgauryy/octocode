On a master, GET on an expired string key returns a null reply (`$-1` in RESP2, `_` in RESP3). As a side effect, the key is lazily deleted and the deletion is propagated. A replica also returns null but leaves the key in place. The lines below are all from commit 20bb2cfc54.

**Call path**
- `getCommand` calls `getGenericCommand` (`src/t_string.c:470`).
- That calls `lookupKeyReadOrReply(c, c->argv[1], shared.null[c->resp])` (`src/t_string.c:459`).
- If the lookup returns NULL, `lookupKeyReadOrReply` sends the null reply (`src/db.c:402-405`). `getGenericCommand` then returns without a type check or `addReplyBulk` (`src/t_string.c:459-460`).

**Expiry check**
- `lookupKey` calls `expireIfNeeded(db, key, val, expire_flags)`. If the result isn't `KEY_VALID`, it sets `val = NULL` (`src/db.c:319-322`).
- `keyIsExpired` reports expired when `now > when`, using `commandTimeSnapshot()` (`src/db.c:2946-2955`).
- Expiry is skipped while loading or when `allow_access_expired` is set (`src/db.c:2948`).

**What `expireIfNeeded` does on a master**
- Nothing in `lookupKeyRead` adds the force-delete or avoid-delete flags, so `expire_flags` is 0 for a plain GET. The force flag is only added for `LOOKUP_WRITE` (`src/db.c:310-311`).
- It falls through to `deleteExpiredKeyAndPropagate(db, key)` and returns `KEY_DELETED` (`src/db.c:3073-3082`).
- That runs `deleteKeyAndPropagate` (`src/db.c:2898`, `2847`):
  - It deletes the key with `dbGenericDelete`, lazily if `lazyfree_lazy_expire` is set (`src/db.c:2879`).
  - It fires the `expired` keyspace event (`src/db.c:2884`).
  - It calls `keyModified` (`src/db.c:2885`).
  - It propagates a DEL or UNLINK to the AOF and replicas (`src/db.c:2886`).

**Stats and events**
- Because `val` is now NULL, `lookupKey` fires the `keymiss` keyspace event unless `LOOKUP_NONOTIFY` is set. It also increments `stat_keyspace_misses` unless `LOOKUP_NOSTATS` is set (`src/db.c:346-350`).

**Cases where the key is not deleted**
- **Replica:** if `masterhost != NULL` and `EXPIRE_FORCE_DELETE_EXPIRED` isn't set, it returns `KEY_EXPIRED` without deleting. GET still sees a miss and waits for the master's DEL (`src/db.c:3050-3053`). A client flagged `CLIENT_MASTER` sees the key as valid (`src/db.c:3051`).
- **Lazy-expire disabled:** `confAllowsExpireDel()` can return false, which gives `KEY_EXPIRED` with no delete (`src/db.c:3058-3059`). This only happens for nested commands that touch arbitrary keys, unless `lazyexpire_nested_arbitrary_keys` is set (`src/db.c:2958-2966`).
- **Expire action paused:** `isPausedActionsWithUpdate(PAUSE_ACTION_EXPIRE)` gives `KEY_EXPIRED` with no delete (`src/db.c:3070`).
- **Slot being trimmed:** in cluster slot trimming, the result can be `KEY_TRIMMED` (`src/db.c:3010-3019`).

I read the code but didn't run anything. I did not look at the `lookupKeyRead` wrapper itself, which sits just after `src/db.c:356`. The "no extra flags" claim relies on the flag mapping at `src/db.c:310-317`.