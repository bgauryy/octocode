**The bug:** A client that a pool had removed could still pull queued requests, and `pool.destroy()` couldn't reach it. In the PR's words, after a failed connect "a removed client could still pickup up queued request. The pool was reporting zero connections while the socket was open, and pool.destroy() did not abort the request nor close the socket."

There were two causes:
- In `Pool` and `RoundRobinPool`, the `connectionError` handler only spliced the client out of `kClients`. The client was never closed, so its drain listener kept pulling requests from the pool queue. Nothing in the pool tracked it any more, so `destroy()` and `close()` skipped it. The code comment in the patch says this directly.
- The same gap applied to clients evicted by `clientTtl`. `kRemoveClient` called `client.close(() => {})` and dropped the reference. `pool.close()` therefore didn't wait for the evicted client's in-flight requests, and `pool.destroy()` didn't abort them.

**The fix** (merge commit `da911f087c62952640fdacccf4e759b295ad300c`). All line references below are to the PR diff, not to a checked-out tree.
- **`lib/dispatcher/pool-base.js`:**
  - It adds a `kRetireClient` method and a `kRetiring` map, which holds clients that are out of rotation but still closing.
  - `kRetireClient` removes the client from `kClients` and calls `client.close()` using the callback form, because factory-supplied dispatchers may not return a promise. It stores a promise in `kRetiring` that settles when the close finishes. It skips clients that are already destroyed or already retiring.
  - `kRemoveClient` now calls `kRetireClient`.
  - A new `closeClients()` helper replaces three duplicated blocks. It closes the live clients and also awaits every promise in `kRetiring`, so `pool.close()` waits for retiring clients.
  - `[kDestroy]` now also destroys every client in `kRetiring`.
- **`lib/dispatcher/pool.js`:** the `connectionError` handler now calls `kRetireClient(target)` instead of only splicing the client out. It also calls `kDrainQueue` when `this[kQueued] > 0`, so queued requests go to another or a new client.
- **`lib/dispatcher/round-robin-pool.js`:** the same two changes as in `pool.js`. The handler retires the client and re-drains the queue, and `kRetireClient` and `kDrainQueue` are newly imported.
- **Tests:** `test/pool-retired-clients.js` is new and runs for both `Pool` and `RoundRobinPool`. The cases cover:
  - A queued request goes to a new client after a connection error.
  - `destroy()` aborts a request that was queued behind the failed connect, with `UND_ERR_DESTROYED`.
  - `destroy()` aborts in-flight requests on a client evicted by `clientTtl`.
  - `close()` waits for a `clientTtl`-evicted client.

**Uncertainty:** I read the PR diff and description through the GitHub API. I did not check out the merged tree or run the tests. The "drain listener" mechanism is taken from the code comments in the diff, so I did not trace `kOnDrain` myself.