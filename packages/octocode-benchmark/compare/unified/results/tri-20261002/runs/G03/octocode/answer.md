**The bug:** A client that the pool had taken out of rotation could still pull queued requests. This happened after a connection error or a `clientTtl` eviction. Such a client was dropped from `kClients`, but its drain listener stayed attached to the pool queue. In the PR author's words, "after a failed connect, a removed client could still pickup up queued request." In that state the pool reported zero connections while the socket was open. `pool.destroy()` could not reach the client, so it neither aborted the request nor closed the socket. `pool.close()` also ignored such a client and could resolve before its in-flight requests finished.

The PR comments in `lib/dispatcher/pool.js` and `round-robin-pool.js` say the same. In both, the `connectionError` handler only spliced the target out of `kClients`, and `Pool` had a comment saying not to use `kRemoveClient` there. The comment gave the reason: "the client cannot be closed in this state."

**How the fix works** (the diff is merged at `da911f0`; I read the patch, not the merged files):

1. **`lib/dispatcher/pool-base.js`** adds a `kRetiring` Map and a `kRetireClient(client)` method.
   - The Map holds clients that have left `kClients` but have not finished closing. Each maps to a promise that settles when its close callback fires.
   - `kRetireClient` removes the client from `kClients` and skips it if it is already destroyed or already retiring.
   - It then calls `client.close(cb)` with the callback form. Custom `factory` dispatchers need not return a promise and may call back synchronously.
   - A closed client no longer takes requests from the pool queue, but it finishes the requests it already has.
   - `kRemoveClient` now calls `kRetireClient`.
2. **Close and destroy now see retiring clients.**
   - A new `closeClients(pool)` helper closes the live clients and also awaits the promises in `kRetiring`. It replaces three duplicated inline loops, in `kClose` and the two `kClosedResolve` paths.
   - `kDestroy` additionally calls `destroy(err)` on every client in `kRetiring`.
3. **`lib/dispatcher/pool.js` and `lib/dispatcher/round-robin-pool.js`**
   - The `connectionError` handlers now call `kRetireClient(target)` instead of only splicing the client out of `kClients`.
   - Because the retired client no longer drains the queue, they call `kDrainQueue(origin, targets.slice(1))` when `this[kQueued] > 0`. `Pool` also still does this when `resumeQueued` is set. This hands queued requests to another, possibly new, client. `RoundRobinPool` gained this drain call and the `kDrainQueue` and `kQueued` imports.
4. **Tests:** the PR adds `test/pool-retired-clients.js`, which runs against both `Pool` and `RoundRobinPool`. It covers:
   - queued requests going to a new client after a connect failure;
   - `destroy()` aborting requests queued behind a connection error;
   - `destroy()` aborting in-flight requests on a client evicted by `clientTtl`;
   - `close()` waiting for an evicted client.

**Uncertainty:** I did not read the merged source files, only the PR patch. The PR is merged, with `mergedAt` 2026-09-28. The line numbers above are from the diff hunks, so I have not cited exact lines at the merge commit.