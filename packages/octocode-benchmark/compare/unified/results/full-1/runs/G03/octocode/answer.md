**The bug:** A client that a pool had taken out of rotation, after a connection error or when `clientTtl` expired, could still pick up queued requests from the pool. The PR body describes the symptoms. After a failed connect, a removed client picked up a queued request. The pool then reported zero connections while the socket was open. `pool.destroy()` neither aborted that request nor closed the socket.

The patch comments give the cause:
- **Pool and RoundRobinPool:** their `connectionError` handlers only spliced the client out of `kClients` (`lib/dispatcher/pool.js`, `lib/dispatcher/round-robin-pool.js`). The client was never closed, so its drain listener kept pulling requests from the pool queue onto it. `pool.destroy()` iterates `kClients`, so it could no longer reach that client.
- **`kRemoveClient` in `pool-base.js`:** it dropped the client from `kClients` and called `client.close(() => {})`. Nothing tracked the closing client, so `pool.close()` and `pool.destroy()` ignored it while it finished its in-flight requests. The test "close() waits for a client evicted by clientTtl" covers this case.

**The fix**, in `lib/dispatcher/pool-base.js` unless noted:
1. **New `kRetireClient(client)`.** It removes the client from `kClients` and skips clients that are already destroyed or already retiring. Otherwise it creates a promise and stores it in a new `kRetiring` map (client → "closed" promise). It then calls `client.close(cb)`, and the callback deletes the map entry and resolves the promise. It uses the callback form because custom `factory` dispatchers may not return a promise and may call back synchronously.
2. **A closed client stops taking queue work.** A closed client no longer takes requests from the pool queue (see `kOnDrain`, per the patch comment). It only finishes the requests it already has.
3. **`kRemoveClient` now calls `kRetireClient`,** instead of closing the client itself.
4. **Close and destroy see retiring clients.**
   - The duplicated close-all loops are replaced by a shared `closeClients(pool)`. It closes the live clients and also awaits every promise in `kRetiring`, so `pool.close()` waits for retired clients.
   - `[kDestroy]` also calls `destroy(err)` on every client in `kRetiring`, so `pool.destroy()` aborts their in-flight requests and closes their sockets.
5. **`pool.js` and `round-robin-pool.js`.** Their `connectionError` handlers now call `kRetireClient(target)` for targets still in `kClients`. If `this[kQueued] > 0`, they call `kDrainQueue(origin, targets.slice(1))` so queued requests go to another or a new client. In `pool.js` the old condition was `resumeQueued`, and it is now `resumeQueued || this[kQueued] > 0`. RoundRobinPool previously had no drain call at all.

The new `test/pool-retired-clients.js` runs against both `Pool` and `RoundRobinPool`. It covers a queued request going to a new client after a connect failure, `destroy()` aborting requests after a connection error, `destroy()` aborting in-flight requests on a client evicted by `clientTtl`, and `close()` waiting for an evicted client.

**Uncertainty:** I read the patch diffs and the PR description only, not the merged files. I did not check the `kOnDrain` code or the merge commit. The remark that a closed client stops draining the queue comes from the PR's own comments.