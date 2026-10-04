**Answer:** On a master, GET on an already-expired string key behaves as a miss. The key is lazily deleted, a DEL is propagated, and the client gets a null reply (`shared.null[c->resp]`). I traced this through `lookupKeyReadOrReply` into `lookupKey`. I did not open `lookupKeyReadOrReply` itself (it sits in the db.c lines I skipped), so the step from it to `lookupKey` is assumed from its name and its call in `getGenericCommand`.

**Evidence (at 20bb2cfc54):**
- `src/t_string.c:456-468`: `getGenericCommand` calls `lookupKeyReadOrReply(c, c->argv[1], shared.null[c->resp])`. If that returns NULL, it returns with the null reply and no type check. Otherwise it runs `checkType` and `addReplyBulk`.
- `src/db.c:319-323`: `lookupKey` calls `expireIfNeeded(...)`. Any result other than `KEY_VALID` sets `val = NULL`.
- `src/db.c:2946-2955`: `keyIsExpired` treats the key as expired when `now > when`, using `commandTimeSnapshot()`. It never reports expiry while `server.loading` or `server.allow_access_expired` is set.
- `src/db.c:3055-3072`: when none of the early-return conditions apply, `expireIfNeeded` calls `deleteExpiredKeyAndPropagate` and returns `KEY_DELETED`. The doc comment at `src/db.c:2981-2983` says this may propagate a DEL/UNLINK to the AOF and replicas.
- `src/db.c:346-350`: on the miss path, `lookupKey` fires the `keymiss` keyspace event and increments `stat_keyspace_misses`. GET is a read, so `LOOKUP_WRITE` and `LOOKUP_NOSTATS` are not set; I did not check which flags `lookupKeyReadOrReply` actually passes.

**Exceptions:**
- **Read-only replica** (`src/db.c:3043-3046`): the key is not deleted, because the master's DEL does that. `expireIfNeeded` returns `KEY_EXPIRED`, so GET still returns null. The exception is a command coming from the master client, where the key is treated as valid.
- **Cluster mode** (`src/db.c:3043-3044`): a command from the `CLIENT_MASTER` import client sees the key as valid.
- **Lazy-delete suppression:** `KEY_EXPIRED` is returned without deleting if `lazyexpire_nested_arbitrary_keys` and the nesting condition in `confAllowsExpireDel` block it (`src/db.c:3050-3051`), or if expire actions are paused (`src/db.c:3061`). GET still returns null in these cases.
- **Slot trimming** (`src/db.c:3011-3018`): a key in a slot being trimmed returns `KEY_TRIMMED`, which also makes GET reply null.

**Uncertainty:** I did not read `deleteExpiredKeyAndPropagate`, so I haven't checked the exact DEL/UNLINK choice or the `expired` keyspace notification.