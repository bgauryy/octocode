**The bug:** after a failed connect, a client that had been removed from the pool's `kClients` list could still pull queued requests off the pool queue. The PR author (jasnell) says that during audit tests the pool reported zero connections while a socket was open. `pool.destroy()` then neither aborted the request nor closed the socket. The cause was that the removed client was still draining the shared queue, but `destroy()` and `close()` could no longer reach it because it was no longer in `kClients`.

Both `Pool` and `RoundRobinPool` had this problem:
- In `Pool`, the `connectionError` handler only spliced the client out of `kClients`. The old comment said not to use `kRemoveClient`, because the client "cannot be closed in this state".
- In `RoundRobinPool`, the `connectionError` handler also only spliced the client out.
- Clients evicted by `clientTtl` also kept running their in-flight requests while being invisible to `close()` and `destroy()`. The PR's tests cover this case.

**The fix** (PR #5881, merge commit `da911f08`):
1. **`lib/dispatcher/pool-base.js`:**
   - A new `kRetireClient` method removes the client from `kClients` and calls `client.close(cb)`. A closed client no longer takes requests from the pool queue, but it finishes the requests it already has.
   - It records the client in a new `kRetiring` map (client → promise that resolves when the close callback fires). The callback form is used because factory-supplied dispatchers may not return promises.
   - `kRemoveClient` now calls `kRetireClient`.
   - A new `closeClients()` helper replaces three duplicated blocks. It closes the live clients and also awaits the retiring ones, so `pool.close()` waits for them.
   - `kDestroy` now also destroys the clients in `kRetiring`, so `pool.destroy()` aborts their requests and closes their sockets.
2. **`lib/dispatcher/pool.js`:** in the `connectionError` handler, the manual splice is replaced by `this[kRetireClient](target)`. It now calls `kDrainQueue` when `resumeQueued || this[kQueued] > 0`, so queued requests are handed to another or new client.
3. **`lib/dispatcher/round-robin-pool.js`:** the same change as in `pool.js`. It retires the failed clients and calls `kDrainQueue` if `kQueued > 0`.
4. **`test/pool-retired-clients.js`** (new file) runs against both pool types. It checks three things:
   - A queued request goes to a new client after a connect error.
   - `destroy()` aborts requests with `UND_ERR_DESTROYED` and the sockets close.
   - `close()` waits for a client evicted by `clientTtl`.

**Uncertainty:** I read the PR metadata and diff through the API. I did not check the code at the merged commit or confirm that the tests pass. The mechanism is taken from the diff and the PR description.