**The bug:** a client that a pool had taken out of rotation could still pull queued requests from the pool queue, and `pool.destroy()` and `pool.close()` could no longer reach it. The PR description gives this scenario. After a failed connect, a removed client picked up a queued request. The pool then reported zero connections while a socket was open, and `pool.destroy()` neither aborted the request nor closed the socket. The tests also cover clients evicted by `clientTtl`, which had the same problem.

**Cause**, from the diff:
- In `lib/dispatcher/pool.js`, the `connectionError` handler only spliced the client out of `kClients`. The old comment said it avoided `kRemoveClient` because the client "cannot be closed in this state". The round-robin pool's `connectionError` handler did the same.
- The removed client was still a live `Client` with a drain listener, so it kept pulling requests off the pool queue.
- Once it was out of `kClients`, `destroy()` and `close()` never saw it.
- `kRemoveClient` had a related gap. It closed the client but kept no record of it, so `close()` didn't wait for it and `destroy()` didn't destroy it.

**The fix** (merge commit `da911f087c62952640fdacccf4e759b295ad300c`):
1. **Retiring state:** `lib/dispatcher/pool-base.js` gets a `kRetireClient` method and a `kRetiring` map. `kRetireClient` removes the client from `kClients` and calls `client.close()`. It records the client in `kRetiring`, mapped to a promise that resolves when the close callback fires, and deletes the entry then. It skips clients that are already destroyed or already retiring. It uses the callback form of `close` because factory-supplied dispatchers may not return a promise. `kRemoveClient` now delegates to it.
2. **`close()` waits for retiring clients:** the duplicated close-all code is replaced by a `closeClients(pool)` helper. It closes live clients and also awaits every promise in `kRetiring`. `kClose` and both `kClosedResolve` paths use it.
3. **`destroy()` reaches retiring clients:** `kDestroy` now also calls `client.destroy(err)` on every key in `kRetiring`.
4. **Connection errors retire the client:** in `pool.js` and `round-robin-pool.js`, the `connectionError` handler calls `kRetireClient(target)` instead of only splicing the client out.
5. **Queued requests move to another client:** the retired client no longer drains the pool queue. So both handlers now call `kDrainQueue(origin, targets.slice(1))` when `kQueued > 0`. In `pool.js` this is `resumeQueued || this[kQueued] > 0`. The queued request then goes to another, possibly new, client.

**Tests:** the PR adds `test/pool-retired-clients.js`, which runs for both `Pool` and `RoundRobinPool`. It covers:
- queued requests going to a new client after a connect error
- `destroy()` aborting requests queued behind a connect error (`UND_ERR_DESTROYED`)
- `destroy()` aborting in-flight requests on a client evicted by `clientTtl`
- `close()` waiting for an evicted client

**Uncertainty:** I read the PR body and diff through the API. I didn't run the tests or look at the review discussion.