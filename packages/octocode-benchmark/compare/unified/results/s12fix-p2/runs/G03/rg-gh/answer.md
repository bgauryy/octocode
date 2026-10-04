**Short answer:** When a pool removed a client without closing it, for example after a failed connect or when `clientTtl` expired, that client could still pull queued requests from the pool queue. The pool no longer tracked it, so `pool.destroy()` and `pool.close()` could not reach it. The fix adds a "retiring" state. A removed client is closed, so it stops taking queued work, and the pool keeps tracking it until its close finishes. I read the PR diff and description only. I did not run the tests or look at the code at the merge commit (`da911f087c62952640fdacccf4e759b295ad300c`), so I have no line numbers.

**The bug**
- The PR description (by jasnell) says that after a failed connect, "a removed client could still pickup up queued request."
- It also says the pool reported zero connections while the socket was open. `pool.destroy()` neither aborted the request nor closed the socket.
- In `lib/dispatcher/pool.js` and `round-robin-pool.js`, the `connectionError` handler only spliced the client out of `kClients`. The old comment said not to use `kRemoveClient` because "the client cannot be closed in this state".
- The new comment explains the failure: the client's drain listener kept pulling requests from the pool queue onto a client that `pool.destroy()` can't reach.

**The fix**
- **`lib/dispatcher/pool-base.js`**
  - New `kRetireClient` method. It removes the client from `kClients` and records it in a new `kRetiring` map (client → promise). It then calls `client.close(cb)`, and the callback deletes the map entry and resolves the promise. It skips clients that are already destroyed or already retiring. It uses the callback form because custom `factory` dispatchers may not return a promise.
  - `kRemoveClient` now calls `kRetireClient`.
  - `kDestroy` also destroys every client in `kRetiring`.
  - A new `closeClients(pool)` helper replaces three duplicated close loops. It closes the live clients and also waits on the promises of retiring clients, so `pool.close()` waits for them.
- **`lib/dispatcher/pool.js` and `round-robin-pool.js`**
  - The `connectionError` handler now calls `kRetireClient(target)` instead of only splicing.
  - Because the retired client no longer drains the queue, the handler calls `kDrainQueue` when `this[kQueued] > 0`. That hands queued requests to another client, possibly a new one. `pool.js` previously did this only when `resumeQueued` was set. `round-robin-pool.js` gains the call.
- **Tests:** `test/pool-retired-clients.js` covers both `Pool` and `RoundRobinPool`. The cases are:
  - Queued requests go to a new client after a connect error.
  - `destroy()` aborts requests queued behind a connect error.
  - `destroy()` aborts in-flight requests on a client evicted by `clientTtl`.
  - `close()` waits for a `clientTtl`-evicted client.

**Uncertainty:** The PR body's "Bug Fixes" section just says N/A. The bug description above comes from the "Rationale" section and the code comments.