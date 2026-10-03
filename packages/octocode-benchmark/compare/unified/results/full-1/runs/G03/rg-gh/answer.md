**The bug:** when a pool client was taken out of rotation, it could still pick up queued requests, and the pool lost track of it. This happened after a failed connect (`connectionError`) and when a client was evicted by `clientTtl`. The PR text says the pool reported zero connections while the socket was open. `pool.destroy()` neither aborted the request nor closed the socket (PR #5881 body).

**Root cause** (from the diff, `lib/dispatcher/pool.js` and `round-robin-pool.js`): the `connectionError` handlers only spliced the client out of `kClients`. The client was not closed. Its drain listener kept pulling requests from the pool queue onto it. `destroy()` and `close()` iterate only `kClients`, so they could not reach it. The old `kRemoveClient` called `client.close(() => {})` and forgot the client, so `close()` and `destroy()` also missed a client that was still finishing requests.

**The fix (merge commit `da911f087c62952640fdacccf4e759b295ad300c`):**
1. **`lib/dispatcher/pool-base.js`** adds `kRetireClient` and a `kRetiring` map. The map holds clients removed from `kClients` that haven't finished closing, each with a promise that settles when it does.
   - `kRetireClient` splices the client out of `kClients` and registers it in `kRetiring`. It then calls `client.close(cb)`, which deletes the entry and resolves the promise. The callback form is used because custom `factory` dispatchers may not return a promise.
   - `kRemoveClient` now delegates to `kRetireClient`.
   - A shared `closeClients()` helper also awaits the retiring clients, so `pool.close()` waits for them.
   - `[kDestroy]` also destroys the clients in `kRetiring`, so `pool.destroy()` aborts their in-flight requests.
2. **`pool.js`** (`connectionError` handler) now calls `kRetireClient(target)` instead of only splicing. The old comment said the client couldn't be closed in that state. The queue is now re-drained if `resumeQueued || this[kQueued] > 0`, so queued requests go to another or new client.
3. **`round-robin-pool.js`** gets the same change: it retires clients on `connectionError` and calls `kDrainQueue` if `kQueued > 0`.
4. **Tests:** the new `test/pool-retired-clients.js` runs for both `Pool` and `RoundRobinPool`. It covers three cases:
   - Queued requests go to a new client after a connect failure.
   - `destroy()` aborts in-flight requests on a retired client, both after a connection error and after `clientTtl` eviction.
   - `close()` waits for a client evicted by `clientTtl`.

**Uncertainty:** I read the diff and PR body only. I didn't run the tests or check the code at the merge commit. The `clientTtl` eviction path goes through `kRemoveClient`, whose code isn't in the diff. I inferred that path from the tests and the `kRemoveClient` change.