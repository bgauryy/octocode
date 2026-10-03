**The bug:** A client that a pool took out of rotation could still pick up queued requests. It could also be left running where `pool.close()` and `pool.destroy()` couldn't reach it. The PR author's repro was a failed connect. The pool reported zero connections while the socket was open. `pool.destroy()` neither aborted the request nor closed the socket (PR body).

**Cause, from the diff:**
- In `Pool` and `RoundRobinPool`, the `connectionError` handler only spliced the client out of `kClients`. The old comment in `lib/dispatcher/pool.js` said not to use `kRemoveClient` because the client couldn't be closed in that state.
- The client's drain listener stayed attached, so it kept pulling requests from the pool queue. The pool no longer tracked it, so `destroy()` couldn't abort those requests.
- Clients removed through `kRemoveClient`, for example when `clientTtl` expires, were closed with `client.close(() => {})`. Nothing tracked them while they finished their in-flight requests. `pool.close()` therefore didn't wait for them, and `pool.destroy()` didn't destroy them.

**How the fix works:**
1. **New `kRetireClient` (`lib/dispatcher/pool-base.js`).**
   - It removes the client from `kClients` and calls `client.close()`. A closed client no longer takes requests from the pool queue.
   - It records the client in a new `kRetiring` map, keyed by client, with a promise that settles when the close callback fires.
   - It returns early if the client is already destroyed or already retiring.
   - It uses the callback form of `close` because factory-supplied dispatchers may not return a promise or may call back synchronously.
   - `kRemoveClient` now calls `kRetireClient`.
2. **A shared `closeClients(pool)` helper.**
   - It replaces three duplicated close loops.
   - It closes the live clients and also awaits every promise in `kRetiring`. `pool.close()` therefore waits for evicted clients to finish their requests.
3. **`kDestroy`.** It now also destroys every client in `kRetiring`, so `destroy()` aborts their in-flight requests.
4. **`pool.js` and `round-robin-pool.js`.**
   - On `connectionError`, they now call `kRetireClient(target)` instead of only splicing.
   - If `kQueued > 0`, they call `kDrainQueue`. This hands queued requests to another, possibly new, client, because the retired client no longer drains the queue. `Pool` previously did this only when `resumeQueued` was set. `RoundRobinPool` did not do it at all.
5. **Tests.** `test/pool-retired-clients.js` covers both pool types. It checks that queued requests go to a new client after a connect failure, that `destroy()` aborts requests on a failed or TTL-evicted client (`UND_ERR_DESTROYED`), and that `close()` waits for a TTL-evicted client.

**Uncertainty:** The diff was returned with elided hunks, so I haven't seen every line. The description above is based on the visible patches. I didn't check the merge commit or how the PR was reviewed.