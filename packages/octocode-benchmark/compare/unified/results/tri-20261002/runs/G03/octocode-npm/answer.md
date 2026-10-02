**The bug:** A client that a pool had taken out of rotation could still pull requests off the pool's shared queue. It was also invisible to `pool.close()` and `pool.destroy()`.

The PR author describes seeing this after a failed connect: the pool reported zero connections while a socket stayed open, and `pool.destroy()` neither aborted the request nor closed the socket (PR body).

The diff shows two causes:

- **`Pool`'s `connectionError` handler only spliced the client out of `kClients`.** The old comment said not to use `kRemoveClient` because the client "cannot be closed in this state". The client was never closed, so its drain listener kept pulling queued requests from the pool queue onto it. It was no longer in `kClients`, so `destroy()` couldn't reach it (`lib/dispatcher/pool.js` diff).
- **`RoundRobinPool`'s `connectionError` handler did the same splice** (`lib/dispatcher/round-robin-pool.js` diff).
- **The `clientTtl` path has a related problem.** A client evicted by TTL was closed via `client.close(() => {})`, and the pool forgot it. `pool.close()` and `pool.destroy()` then ignored it while it finished its in-flight requests. The new tests cover this case.

**How the fix works** (`lib/dispatcher/pool-base.js`):

- **New `kRetireClient(client)`.** It removes the client from `kClients` and calls `client.close()` with a callback. That makes the client stop taking queued requests but still finish the ones it already has.
- **New `kRetiring` map.** It maps each retiring client to a promise that settles once the client's close callback fires. The callback form is used because factory-supplied dispatchers may not return promises, or may call back synchronously. A guard skips clients that are already destroyed or already retiring.
- **`kRemoveClient` now calls `kRetireClient`.**
- **New `closeClients(pool)` helper.** It replaces three duplicated blocks. It closes the live clients and also awaits every promise in `kRetiring`, so `pool.close()` waits for retired clients.
- **`kDestroy` now also destroys the clients in `kRetiring`.** `pool.destroy()` therefore aborts their in-flight requests and closes their sockets.
- **`Pool` and `RoundRobinPool` call `kRetireClient`.** Their `connectionError` handlers use it instead of the bare splice.
- **Queued requests are handed to another client.** Since the retired client no longer drains the queue, both pools call `kDrainQueue(origin, targets.slice(1))` if `kQueued > 0`. `Pool` does this in addition to its existing `resumeQueued` check. A replacement client can then pick the requests up.

**Tests:** `test/pool-retired-clients.js` runs against both `Pool` and `RoundRobinPool`. It checks four things:

- A queued request goes to a new client after a connect failure.
- `destroy()` aborts requests with `UND_ERR_DESTROYED` and closes all sockets.
- `destroy()` works on a client evicted by `clientTtl`.
- `close()` waits for a TTL-evicted client.

**Uncertainty:** I read the PR description and diff but not the merged files at the merge commit. The diff context was abbreviated, so I haven't seen the exact line numbers.