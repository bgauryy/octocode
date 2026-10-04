**Short answer:** PR #5881 (merged 2026-09-28) fixes a bug where a client taken out of a pool's rotation could still pull queued requests. The pool then lost track of that client, so `pool.destroy()` couldn't abort its requests or close its socket, and `pool.close()` didn't wait for it. The fix closes such clients explicitly and tracks them until they finish closing. This comes from the PR body and diff only; I didn't read the merged source files.

**The bug** (PR body, plus code comments in the diff)
- The author saw that after a failed connect, a removed client could still pick up queued requests.
- The pool reported zero connections while the socket was open.
- `pool.destroy()` neither aborted the request nor closed the socket.
- The cause was in `Pool` and `RoundRobinPool`. On `connectionError`, both only spliced the client out of `kClients`, and `Pool` did the same on `clientTtl` eviction. The code comments say the removed client's drain listener kept pulling requests from the pool queue.
- Because the client was no longer in `kClients`, `destroy()` and `close()` never reached it.

**How the fix works**
- **Tracking retiring clients** (`lib/dispatcher/pool-base.js`):
  - A new `kRetiring` Map holds clients that have left `kClients` but haven't finished closing. Each maps to a promise that settles when the close completes.
  - A new `kRetireClient(client)` removes the client from `kClients` and calls `client.close(cb)`, then records it in `kRetiring`. It uses the callback form because factory-supplied dispatchers may not return a promise.
  - A closed client stops taking requests from the pool queue, but it finishes the requests it already has.
  - `kRemoveClient` now calls `kRetireClient`.
- **`close()` and `destroy()` reach retiring clients** (`lib/dispatcher/pool-base.js`):
  - A new `closeClients(pool)` helper replaces three copies of the close-all logic. It closes the live clients and also awaits every promise in `kRetiring`, so `pool.close()` waits for evicted clients to finish.
  - `kDestroy` also calls `destroy(err)` on each client in `kRetiring`.
- **Connection-error and TTL paths** (`lib/dispatcher/pool.js`, `lib/dispatcher/round-robin-pool.js`):
  - Both pools now call `kRetireClient(target)` instead of splicing the client out of `kClients`.
  - The old `Pool` comment said `kRemoveClient` couldn't be used here because the client can't be closed in this state. The PR replaces that code anyway.
  - Since the retired client no longer drains the queue, both pools call `kDrainQueue` when `kQueued > 0`. In `Pool` this is `resumeQueued || this[kQueued] > 0`. This hands queued requests to another or new client.
- **Tests** (`test/pool-retired-clients.js`), run for both `Pool` and `RoundRobinPool`:
  - After a connection error, the queued request goes to a new client, and the failed client is closed.
  - `destroy()` aborts requests queued behind a connection error.
  - `destroy()` aborts in-flight requests on a client evicted by `clientTtl`.
  - `close()` waits for a client evicted by `clientTtl`.

**Uncertainty:** I didn't fetch the merged files, so I can't give exact line numbers. The diff hunks above are the only evidence I saw.