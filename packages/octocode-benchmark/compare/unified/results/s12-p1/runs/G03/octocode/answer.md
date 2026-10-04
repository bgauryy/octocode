**Summary:** PR #5881 (merged 2026-09-28) fixes a bug where a client that a pool had removed from rotation could still take queued requests. Those requests then ran on a client that `pool.destroy()` and `pool.close()` could no longer reach. I'm working from the PR body and diff. I didn't read the merged files separately or run the tests.

**The bug**
- The PR body describes it: after a failed connect, a removed client could still pick up queued requests. The pool reported zero connections while the socket was open. `pool.destroy()` neither aborted the request nor closed the socket.
- Cause in `Pool`: on `connectionError`, the handler in `lib/dispatcher/pool.js` only spliced the client out of `kClients`. A code comment in the diff says it avoided `kRemoveClient` because "the client cannot be closed in this state".
  - The client was never closed, so its drain listener kept pulling requests from the pool queue.
  - It was no longer in `kClients`, so `destroy()` didn't reach it.
- `RoundRobinPool` had the same splice-only handling in `lib/dispatcher/round-robin-pool.js`.
- The same untracked-client problem applied to clients evicted by `clientTtl`. After `kRemoveClient` called `client.close(() => {})`, the client was untracked while it finished its in-flight requests. `pool.close()` therefore didn't wait for it, and `pool.destroy()` didn't abort it.

**How the fix works**
1. **`lib/dispatcher/pool-base.js`**
   - It adds a `kRetireClient` method and a `kRetiring` Map of client to a "closed" promise.
   - `kRetireClient` removes the client from `kClients` and calls `client.close(cb)`. It uses the callback form because factory dispatchers may not return a promise.
   - The client stays in `kRetiring` until the close callback fires. A closed client no longer takes requests from the pool queue, but it finishes the requests it already has.
   - `kRemoveClient` now delegates to `kRetireClient`.
   - A new `closeClients(pool)` helper replaces three duplicated close loops. It closes the live clients and also awaits every promise in `kRetiring`, so `pool.close()` waits for retired clients.
   - `kDestroy` also calls `destroy(err)` on every client in `kRetiring`, so in-flight requests on retired clients are aborted.
2. **`lib/dispatcher/pool.js` and `lib/dispatcher/round-robin-pool.js`**
   - The `connectionError` handlers now call `kRetireClient(target)` instead of splicing the client out.
   - Because the retired client no longer drains the pool queue, they call `kDrainQueue(origin, targets.slice(1))` when `kQueued > 0`. The `Pool` handler also does this when `resumeQueued` is set. This hands the queued requests to another, possibly new, client.
3. **`test/pool-retired-clients.js`** (new) runs each case for both `Pool` and `RoundRobinPool`:
   - A queued request goes to a new client after a connection error.
   - `destroy()` aborts requests queued behind a connection error.
   - `destroy()` aborts in-flight requests on a client evicted by `clientTtl`.
   - `close()` waits for a client evicted by `clientTtl`.