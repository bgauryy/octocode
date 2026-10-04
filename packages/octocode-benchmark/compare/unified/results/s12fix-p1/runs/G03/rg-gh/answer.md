**The bug:** a client that a pool had removed from rotation could still pull queued requests, and `pool.destroy()` couldn't reach it. The PR description (jasnell) reports it after a failed connect: the pool showed zero connections while a socket was open. `pool.destroy()` neither aborted the request nor closed the socket. The same thing happened to clients evicted by `clientTtl`.

**Why it happened**, from the diff in `lib/dispatcher/pool.js` and `round-robin-pool.js`:
- On `connectionError`, both pools only spliced the client out of `kClients`. The old comment in `pool.js` says it deliberately avoided `kRemoveClient` because "the client cannot be closed in this state".
- The removed client was never closed, so its drain listener kept pulling requests from the pool queue.
- The pool no longer tracked that client, so `destroy()` and `close()` skipped it.

**How the fix works:**
- **`lib/dispatcher/pool-base.js`:**
  - It adds `kRetireClient` and a `kRetiring` map. The map holds clients removed from `kClients` that haven't finished closing, each mapped to a promise that settles when it does.
  - `kRetireClient` splices the client out of `kClients`, records it in `kRetiring`, and calls `client.close(cb)`. It uses the callback form because factory-made dispatchers may not return a promise. When the callback fires, the client is deleted from `kRetiring` and its promise resolves.
  - `kRemoveClient` now delegates to `kRetireClient`.
  - `kDestroy` also destroys everything in `kRetiring`.
  - A new `closeClients()` helper replaces three duplicated blocks. It closes live clients and also awaits the retiring promises, so `pool.close()` waits for evicted clients to finish.
- **`pool.js` and `round-robin-pool.js`:** the `connectionError` handlers now call `kRetireClient(target)` instead of just splicing. A retired client no longer drains the pool queue, so if `kQueued > 0` they call `kDrainQueue(origin, targets.slice(1))`. This hands the queued requests to another client, possibly a new one. `round-robin-pool.js` previously had no such call, so it now imports `kDrainQueue`.
- **Tests:** the new `test/pool-retired-clients.js` covers both `Pool` and `RoundRobinPool`. It checks that:
  - queued requests go to a new client after a connect failure;
  - `destroy()` aborts queued and in-flight requests with `UND_ERR_DESTROYED`;
  - `close()` waits for a client evicted by `clientTtl`.

**Uncertainty:** I read only the PR body and diff via the API. I did not check the checkout or run the tests. The merge commit SHA is `da911f087c62952640fdacccf4e759b295ad300c`. I didn't cite line numbers because I only saw diff hunks, not the file at the pinned commit.