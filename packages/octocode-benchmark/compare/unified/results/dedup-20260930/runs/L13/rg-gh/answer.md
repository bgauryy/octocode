On a master, GET on an expired string key returns a null reply (nil), and the key is lazily deleted as a side effect. I traced this through the source and didn't run it.

**Call path**
- `getCommand` calls `getGenericCommand` (`src/t_string.c:470`, `:456`). That calls `lookupKeyReadOrReply(c, c->argv[1], shared.null[c->resp])` (`src/t_string.c:459`).
- `lookupKeyReadOrReply` calls `lookupKeyRead`, which calls `lookupKey` with no flags (`src/db.c:402`, `:375`). If the result is NULL, it sends the null reply (`src/db.c:403`).

**Expiry check**
- `lookupKey` finds the entry and calls `expireIfNeeded` (`src/db.c:319`). Any result other than `KEY_VALID` sets `val = NULL`.
- `keyIsExpired` says the key is expired when `now > when`, using `commandTimeSnapshot()` (`src/db.c:2950-2955`). It never reports expiry while loading or when `allow_access_expired` is set.

**Master**
- `expireIfNeeded` (`src/db.c:3004`) reaches `deleteExpiredKeyAndPropagate` and returns `KEY_DELETED` (`src/db.c:3064-3073`).
- `deleteKeyAndPropagate` (`src/db.c:2847-2895`) then does the following:
  - deletes the key with `dbGenericDelete`, lazily if `lazyfree_lazy_expire` is set (`:2879`);
  - fires the `expired` keyspace event (`:2883`);
  - calls `keyModified` (`:2884`);
  - propagates a `DEL`, or `UNLINK` if lazy, to the AOF and replicas (`:2885`);
  - increments `stat_expiredkeys` (`:2888`).
- Because the lookup returned NULL, `lookupKey` counts a keyspace miss and fires a `keymiss` event (`src/db.c:349-353`).
- The null reply is `shared.null[c->resp]`, so it is nil in both RESP2 and RESP3.

**Cases where the key isn't deleted**
- **Replica** (`masterhost != NULL`): it returns `KEY_EXPIRED` without deleting, so GET still returns nil. It waits for the master's `DEL` (`src/db.c:3039-3042`). Commands coming from the master itself treat the key as valid (`:3040`).
- **Deletion disallowed:** if `confAllowsExpireDel()` is false, or expire actions are paused, it returns `KEY_EXPIRED` and skips the delete (`src/db.c:3046-3055`). GET still returns nil.
- **Loading:** `keyIsExpired` returns 0, so the key is treated as valid.

**Uncertainty:** I did not read the body of `confAllowsExpireDel`. It is defined after `keyIsExpired` (`src/db.c:2958-2966`), and I only saw its tail.