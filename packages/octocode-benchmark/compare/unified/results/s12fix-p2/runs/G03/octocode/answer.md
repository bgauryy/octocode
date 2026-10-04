**The bug.** A client that a pool had taken out of rotation could still pick up queued requests, and the pool then lost track of it. The PR body describes the symptoms: after a failed connect, a removed client picked up a queued request. The pool then reported zero connections while the socket was still open. `pool.destroy()` neither aborted the request nor closed the socket.

The same thing happened when `clientTtl` evicted a client.

**Cause**, from the diff:
- On `connectionError` (`Pool` and `RoundRobinPool`) and on `clientTtl` eviction (`Pool`), the code only spliced the client out of `kClients`. It did not close the client.
- `Pool` had a comment saying it avoided `kRemoveClient` because it thought the client couldn't be closed in that state.
- The client's drain listener was still attached, so it kept pulling requests from the pool queue.
- `close()` and `destroy()` only walked `kClients`, so they never reached the dropped client.

**The fix.** The changes are in `lib/dispatcher/pool-base.js`, `pool.js` and `round-robin-pool.js`:
- **Retire instead of drop.** `pool-base.js` gains `kRetireClient` and a `kRetiring` map.
  - `kRetireClient` removes the client from `kClients` and calls `client.close()`.
  - Once a client is closed, it stops taking requests from the pool queue. It still finishes the requests it already has.
  - It skips clients that are already destroyed or already retiring.
  - It records the client in `kRetiring`, mapped to a promise that settles when `close()` finishes.
  - It uses the callback form of `close`, because custom `factory` dispatchers may not return a promise.
  - `kRemoveClient` now calls `kRetireClient`.
- **`close()` and `destroy()` reach retired clients.**
  - A new `closeClients(pool)` helper replaces three duplicated blocks. It closes the live clients and also waits on the `kRetiring` promises.
  - `kDestroy` now also destroys every client in `kRetiring`. This is what lets `pool.destroy()` abort in-flight requests and close the sockets.
- **`pool.js` and `round-robin-pool.js`.** Both now call `kRetireClient(target)` on `connectionError`. `Pool` does the same for `clientTtl` eviction.
- **Re-dispatching queued work.** A retired client no longer drains the pool queue. So when `this[kQueued] > 0`, both pools call `kDrainQueue`, which hands the queued requests to another or new client. `Pool` does this alongside its existing `resumeQueued` condition. `RoundRobinPool` gained this call, and it didn't have one before.
- **Tests.** The new `test/pool-retired-clients.js` runs against both `Pool` and `RoundRobinPool`. It checks that:
  - queued requests go to a new client after a connect failure;
  - `destroy()` aborts requests after a connect error or a TTL eviction, with `UND_ERR_DESTROYED`;
  - `close()` waits for a TTL-evicted client to finish.

**Uncertainty.** I read the diff and PR body only. I did not run the tests or read the merged files. The diff view was trimmed, so a few unchanged lines were elided. The PR merged on 2026-09-28.