**The bug:** a client that a pool had removed from rotation could still pull queued requests from the pool. After a failed connect, the pool reported zero connections while a socket was still open. `pool.destroy()` neither aborted the request nor closed that socket. The PR author describes this in the PR body (https://github.com/nodejs/undici/pull/5881).

**Why it happened:**
- **Client kept draining the pool queue.** In `Pool` and `RoundRobinPool`, the `connectionError` handler only spliced the client out of `kClients`. It did not close it. The client's drain listener stayed active, so it kept taking requests from the pool queue. The removed comment in `pool.js` says as much: "Do not use kRemoveClient here, as it will close the client…".
- **Client unreachable by destroy/close.** Once spliced out, the client was no longer in `kClients`. `pool.destroy()` and `pool.close()` only iterated `kClients`, so they never reached it. The same gap applied to clients evicted by `clientTtl`: `kRemoveClient` closed them but then forgot them.

**How the fix works** (merge commit `da911f0`):
1. **`lib/dispatcher/pool-base.js`: new `kRetireClient`.**
   - It removes the client from `kClients`.
   - It skips the client if it is already destroyed or already retiring.
   - Otherwise it records the client in a new `kRetiring` map (client → promise).
   - It then calls `client.close(cb)`. The callback removes the entry and resolves the promise. The callback form is used because custom `factory` dispatchers may not return a promise.
   - A closed client stops taking queued requests, but it still finishes the requests it already has.
   - `kRemoveClient` now calls `kRetireClient`.
2. **Retired clients stay reachable.**
   - `kDestroy` also destroys every client in `kRetiring`.
   - A new `closeClients(pool)` helper replaces three copy-pasted close loops. It closes live clients and also awaits the `kRetiring` promises, so `pool.close()` waits for retired clients to finish.
3. **`pool.js` and `round-robin-pool.js`: `connectionError` handlers.**
   - They now call `kRetireClient(target)` instead of only splicing.
   - If `this[kQueued] > 0`, they call `kDrainQueue(origin, targets.slice(1))`. This hands the queued requests to another, possibly new, client. `Pool` does this as well as its existing `resumeQueued` case. `RoundRobinPool` gains this call, along with the `kDrainQueue` and `kQueued` imports.
4. **Tests:** the new `test/pool-retired-clients.js` runs against both `Pool` and `RoundRobinPool`. It covers four cases:
   - A queued request goes to a new client after a connect failure.
   - `destroy()` aborts requests queued behind a connection error.
   - `destroy()` aborts in-flight requests on a client evicted by `clientTtl`.
   - `close()` waits for a client evicted by `clientTtl`.

**Uncertainty:** I read the PR diff and metadata through `gh`. I did not check out the merge commit, so I have not cited line numbers in the final source. I also did not run the tests.