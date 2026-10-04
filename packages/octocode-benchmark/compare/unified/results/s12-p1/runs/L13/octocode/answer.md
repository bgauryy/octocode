On a master, GET on an expired string key returns a null reply (`$-1` in RESP2, `_` in RESP3). The server also deletes the key on the spot (lazy expiration), counts a keyspace miss, and propagates a DEL. On a read-only replica the key is not deleted, but the reply is still null.

**Call path**
- `getCommand` calls `getGenericCommand` (`src/t_string.c:470-472`).
- That function calls `lookupKeyReadOrReply(c, c->argv[1], shared.null[c->resp])` and returns when the result is NULL (`src/t_string.c:459-460`).
- `lookupKeyReadOrReply` calls `lookupKeyRead`, which calls `lookupKey` with `LOOKUP_NONE`. If the result is NULL, it sends the null reply (`src/db.c:402-406`, `373-375`).
- Because the lookup is a read, `LOOKUP_WRITE` is not set, so `EXPIRE_FORCE_DELETE_EXPIRED` is not set either (`src/db.c:311-312`).

**Expiry check**
- `lookupKey` calls `expireIfNeeded` (`src/db.c:319`). If the status is not `KEY_VALID`, it sets `val = NULL` (`src/db.c:319-323`).
- `keyIsExpired` treats the key as expired when `now > when`. It always returns "not expired" while loading or when `allow_access_expired` is set (`src/db.c:2946-2954`).

**What `expireIfNeeded` does with an expired key** (`src/db.c:3004-3073`)
- **Master, default config:** it reaches the delete step. `deleteExpiredKeyAndPropagate` removes the key and returns `KEY_DELETED` (`src/db.c:3064-3072`).
  - That delete (`deleteKeyAndPropagate`, `src/db.c:2884-2891`) sends an expired keyspace notification and calls `propagateDeletion`, so replicas and the AOF get a DEL/UNLINK.
  - It also increments `stat_expiredkeys`.
- **Replica:** `src/db.c:3043-3046` returns `KEY_EXPIRED` without deleting the key. The replica waits for the master's DEL. If the command comes from the master connection, the key is treated as valid.
- **Other cases that return `KEY_EXPIRED` without deleting:**
  - `lazyexpire_nested_arbitrary_keys` is off and the call is a nested command that touches arbitrary keys (`src/db.c:3050-3051`).
  - The `EXPIRE_AVOID_DELETE_EXPIRED` flag is set (`src/db.c:3055-3056`).
  - Expire actions are paused (`src/db.c:3061`).
  - In all of these GET still replies null.

**Stats and events**
- When `lookupKey` gets NULL, it fires a `keymiss` keyspace event and increments `stat_keyspace_misses` (`src/db.c:346-350`).
- A GET on an expired key is therefore counted as a miss, not a hit.

**Not checked:** I did not read `deleteKeyAndPropagate` above line 2873 or `propagateDeletion`. The choice between DEL and UNLINK (lazy free) is made there, so I haven't confirmed which one is sent.