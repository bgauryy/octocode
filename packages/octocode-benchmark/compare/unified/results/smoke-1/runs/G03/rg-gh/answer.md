**The bug:** when a pool took a client out of rotation, either after a `connectionError` or when `clientTtl` expired, the client could still pick up queued requests. The PR description says a removed client could take a queued request after a failed connect. In that state the pool reported zero connections while a socket was open, and `pool.destroy()` neither aborted the request nor closed the socket. The removed client was no longer in `kClients`, so `destroy()` and `close()` couldn't reach it. Its drain listener kept pulling from the pool queue anyway.

In `lib/dispatcher/pool.js` and `round-robin-pool.js`, the `connectionError` handler only spliced the client out of `kClients`. The removed comment in `pool.js` said not to use `kRemoveClient` there because "the client cannot be closed in this state". Nothing closed the client, so it kept draining the queue.

**The fix** (merge commit `da911f0`, PR #5881, 4 files):

1. **New retire mechanism in `lib/dispatcher/pool-base.js`.**
   - `[kRetireClient](client)` splices the client out of `kClients`.
   - It skips clients that are already destroyed or already retiring.
   - Otherwise it records the client in a new `kRetiring` map, from client to a promise, and calls `client.close(cb)`.
   - The callback removes the client from `kRetiring` and resolves the promise.
   - It uses the callback form because custom `factory` dispatchers may not return a promise and may call back synchronously.
   - `kRemoveClient` now delegates to `kRetireClient`.
   - A closed client stops taking queued requests but finishes the ones it already has.

2. **`close()` and `destroy()` now reach retiring clients.**
   - A new `closeClients(pool)` helper replaces three duplicated loops. It closes the live clients and also awaits every promise in `kRetiring`, so `pool.close()` waits for evicted clients to finish.
   - `[kDestroy]` also calls `destroy(err)` on every client in `kRetiring`. That is what aborts in-flight requests and closes the socket.

3. **The `connectionError` handlers use it.**
   - In `pool.js` and `round-robin-pool.js`, the handler now calls `kRetireClient(target)` for clients still in `kClients`.
   - Because the retired client no longer drains the queue, the handlers call `this[kDrainQueue](origin, targets.slice(1))` when `kQueued > 0`. That hands queued requests to another client, possibly a new one. In `pool.js` this is added to the existing `resumeQueued` condition. `RoundRobinPool` previously had no such call.

4. **Tests.** The new `test/pool-retired-clients.js` runs against both `Pool` and `RoundRobinPool`. It covers these cases:
   - After a failed connect, the queued request goes to a new client.
   - `destroy()` aborts a request queued behind a connection error.
   - `destroy()` aborts in-flight requests on a client evicted by `clientTtl`.
   - `close()` waits for a client evicted by `clientTtl`.

**Uncertainty:** I read the diff and PR description, not the checked-out source, so I have no post-merge line numbers. The `clientTtl` eviction path goes through `kRemoveClient`, which now retires. I didn't inspect that code beyond the diff and the tests.