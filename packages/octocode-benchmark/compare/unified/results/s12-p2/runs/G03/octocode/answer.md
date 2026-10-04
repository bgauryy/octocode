**The bug.** A client that a pool had taken out of rotation could still pick up queued requests from the pool. This happened after a failed connect, and it also happened for clients evicted by `clientTtl`. The PR description reports the symptoms: the pool showed zero connections while a socket was open, and `pool.destroy()` neither aborted the request nor closed the socket. It also says the cause was spread across a few places.

I got this from the PR body and patch only. I didn't read the post-merge source or check the drain-listener explanation against it.

**Cause, from the code comments in the diff.**
- In `Pool`'s `connectionError` handler (`lib/dispatcher/pool.js`), the failed client was only spliced out of `kClients`. The old comment said `kRemoveClient` couldn't be used because it closes the client, and the client couldn't be closed in that state.
- `RoundRobinPool` did the same splice (`lib/dispatcher/round-robin-pool.js`).
- The client's drain listener stayed attached. It kept pulling requests from the pool queue onto a client that `pool.destroy()` could no longer reach, because destroy only walked `kClients`.
- `pool.close()` also ignored clients that were removed but still finishing requests.

**The fix** (PR #5881, merged 2026-09-28).
1. **`lib/dispatcher/pool-base.js`**
   - It adds `kRetireClient` and a `kRetiring` map. The map holds clients that have been removed but haven't finished closing, each mapped to a promise that settles when it has.
   - `kRetireClient` removes the client from `kClients` and calls `client.close(cb)`. A closed client stops taking requests from the pool queue, but it finishes the ones it already has. It uses the callback form because factory-supplied dispatchers may not return a promise.
   - `kRemoveClient` now calls `kRetireClient`.
   - A new `closeClients()` helper replaces three copies of the close-all code. It closes the live clients and also waits on every promise in `kRetiring`, so `pool.close()` waits for retired clients.
   - `kDestroy` now also destroys the clients in `kRetiring`, so `pool.destroy()` aborts their in-flight requests and closes their sockets.
2. **`lib/dispatcher/pool.js`**: the `connectionError` handler now calls `kRetireClient(target)` instead of splicing. It also redistributes the queue with `kDrainQueue` when `resumeQueued || this[kQueued] > 0`, so queued requests go to another (possibly new) client.
3. **`lib/dispatcher/round-robin-pool.js`**: the same retire-and-redrain change in its `connectionError` handler.
4. **Tests**: `test/pool-retired-clients.js` runs against both `Pool` and `RoundRobinPool`. It covers:
   - a queued request going to a new client after a connection error;
   - `destroy()` aborting requests after a connection error;
   - `destroy()` aborting requests on a client evicted by `clientTtl`;
   - `close()` waiting for a client evicted by `clientTtl`.