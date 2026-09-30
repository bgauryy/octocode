**The bug:** A client that a pool had taken out of rotation could still pick up queued requests. It also became unreachable from `pool.close()` and `pool.destroy()`.

The PR body (author's audit-test findings) describes it this way: after a failed connect, a removed client could still pick up queued requests. The pool reported zero connections while the socket was open. `pool.destroy()` neither aborted the request nor closed the socket.

The patch comments give the cause. In `Pool`'s `connectionError` handler (`lib/dispatcher/pool.js`) and `RoundRobinPool`'s (`lib/dispatcher/round-robin-pool.js`), the failed client was only spliced out of `kClients`. It was not closed, because the old code assumed "the client cannot be closed in this state". Its drain listener kept pulling requests from the pool queue onto it. Because it was no longer in `kClients`, `pool.destroy()` couldn't reach it. The same gap applied to clients evicted by `clientTtl`, which are removed while still running in-flight requests. In that case `pool.close()` didn't wait for them either.

**The fix** (all in `lib/dispatcher/`):

1. **New retire state in `pool-base.js`.** `kRetireClient(client)` removes the client from `kClients` and calls `client.close(cb)`. A closed client stops taking requests from the pool queue via `kOnDrain`, but it finishes the requests it already has. The pool records the client in a new `kRetiring` map. The map value is a promise that resolves when the close callback fires, and the entry is deleted at that point. The callback form is used because factory-supplied dispatchers may not return a promise. `kRemoveClient` now delegates to `kRetireClient`.
2. **`close()` and `destroy()` cover retiring clients.** The duplicated close-all loops are merged into a `closeClients(pool)` helper. It closes the live clients and also awaits the `kRetiring` promises, so `pool.close()` waits for evicted clients to finish. `kDestroy` now also calls `destroy(err)` on the clients in `kRetiring`, so in-flight requests get `UND_ERR_DESTROYED` and the sockets close.
3. **`connectionError` handlers in `pool.js` and `round-robin-pool.js`.** They now call `kRetireClient(target)` instead of a bare splice. Because the retired client no longer drains the queue, they also call `kDrainQueue` when `this[kQueued] > 0`, so queued requests go to another or new client. `Pool` previously did this only when `resumeQueued` was set. `RoundRobinPool` gains the call.

**Tests:** `test/pool-retired-clients.js` covers `Pool` and `RoundRobinPool`. The cases are:
- A queued request goes to a new client after a connect failure.
- `destroy()` aborts requests queued behind a connection error.
- `destroy()` aborts in-flight requests on a client evicted by `clientTtl`.
- `close()` waits for a client evicted by `clientTtl`.

**Caveats:** I read the PR diff, not the merged tree, so the line numbers are not confirmed against the merge commit. The `kOnDrain` behavior is stated in the patch comments; I didn't open that code.