**The bug:** a client that a pool had removed from rotation could still pull queued requests, and `pool.destroy()` and `pool.close()` could no longer reach it. The PR (by jasnell, merge commit `da911f0`) describes the symptom this way. After a failed connect, the pool reported zero connections while a socket was still open, and `pool.destroy()` neither aborted the request nor closed the socket.

**Cause, as the diff shows:**
- On `connectionError`, `Pool` and `RoundRobinPool` only spliced the client out of `kClients` and did not close it. The old comment in `lib/dispatcher/pool.js` said "the client cannot be closed in this state". The removed client's drain listener therefore kept pulling requests from the shared pool queue onto it.
- `destroy()` and `close()` iterate only `kClients`, so they skipped that client. The same gap applied to clients evicted by `clientTtl`, because `kRemoveClient` closed them but kept no reference to them.

**The fix:**
- **Retiring clients (`lib/dispatcher/pool-base.js`):**
  - It adds `kRetireClient` and a `kRetiring` map of retired client to a promise that settles when its close finishes.
  - `kRetireClient` removes the client from `kClients`, skips it if it is already destroyed or already retiring, and calls `client.close(cb)`.
  - The callback form is used because factory-supplied dispatchers may not return a promise or may call back synchronously.
  - A closing client stops taking pool-queue requests. It still finishes the requests it already has.
  - `kRemoveClient` now delegates to `kRetireClient`.
- **Close and destroy:**
  - A new `closeClients()` helper closes the live clients and also waits on every `kRetiring` promise. `kClose` and both `kClosedResolve` paths use it.
  - `kDestroy` now also destroys the clients in `kRetiring`, so in-flight requests on retired clients are aborted.
- **Connection errors (`lib/dispatcher/pool.js`, `lib/dispatcher/round-robin-pool.js`):**
  - On `connectionError`, these now call `kRetireClient(target)` instead of just splicing.
  - Since the retired client no longer drains the queue, they call `kDrainQueue` when `kQueued > 0`. That hands the waiting requests to another or a new client. `RoundRobinPool` gains this drain call too.
- **Tests (`test/pool-retired-clients.js`):** they run for both `Pool` and `RoundRobinPool`. They cover queued requests going to a new client after a connect failure, `destroy()` aborting requests after a connect error or a `clientTtl` eviction, and `close()` waiting for an evicted client.

**Uncertainty:** I read the PR metadata and diff only. I did not check out the merged code or confirm that it matches the diff at the pinned commit. The line numbers I could cite are diff hunk positions, not lines in the merged files.