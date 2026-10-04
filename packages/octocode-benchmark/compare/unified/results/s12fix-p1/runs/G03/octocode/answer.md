**Short answer:** PR #5881 (merged 2026-09-28) fixes a bug where a client the pool had taken out of rotation could still pull queued requests from the pool queue. Those requests then ran on a client that `pool.close()` and `pool.destroy()` couldn't reach. Line numbers aren't cited because I only read the PR diff, not the files at the merge commit.

**The bug**
- The author's report: after a failed connect, "a removed client could still pickup up queued request." The pool reported zero connections while the socket was open. `pool.destroy()` neither aborted the request nor closed the socket.
- In `lib/dispatcher/pool.js`, the `connectionError` handler only spliced the failed client out of `kClients`. `round-robin-pool.js` did the same. The removed client's drain listener stayed attached, so it kept pulling requests from the pool queue.
- The same happened to clients evicted by `clientTtl`. A client removed from `kClients` was invisible to `kClose` and `kDestroy`, which only iterate `kClients`.
  - `pool.destroy()` skipped it, so its in-flight requests weren't aborted.
  - `pool.close()` didn't wait for it.

**The fix**
- **`pool-base.js`:** adds a `kRetiring` map and a `kRetireClient(client)` method.
  - `kRetireClient` removes the client from `kClients` and calls `client.close()`. A closed client no longer takes pool-queue requests, but it finishes the ones it already has.
  - It records a promise in `kRetiring` that resolves when the close callback fires. It skips clients that are already destroyed or already retiring.
  - It uses the callback form of `close`, because custom `factory` dispatchers aren't required to return a promise.
  - `kRemoveClient` now delegates to `kRetireClient`.
- **Close and destroy:** a new `closeClients(pool)` helper replaces three duplicated close loops. It closes the live clients and also awaits every promise in `kRetiring`. `kDestroy` now destroys the retiring clients too.
- **`pool.js` and `round-robin-pool.js`:** the `connectionError` handler now calls `kRetireClient(target)` instead of splicing. Previously the client was only dropped from `kClients`, and a code comment said it "cannot be closed in this state". Because the retired client no longer drains the queue, the handler now also calls `kDrainQueue` when `kQueued > 0`. That hands waiting requests to another or a new client. For `Pool` this extends the existing `resumeQueued` condition. For `RoundRobinPool` it's new.
- **Tests:** `test/pool-retired-clients.js` covers both pool types in four cases:
  - a queued request goes to a new client after a connection error;
  - `destroy()` aborts requests queued behind a connection error;
  - `destroy()` aborts in-flight requests on a client evicted by `clientTtl`;
  - `close()` waits for a client evicted by `clientTtl`.

**Uncertainty:** I didn't open the merged files at the merge commit, so everything above comes from the diff and PR description.