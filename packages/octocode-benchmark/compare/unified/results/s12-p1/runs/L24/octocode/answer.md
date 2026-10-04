Six non-test functions in the scrape package start goroutines. The first four below are in `scrape/scrape.go` and the last two are in `scrape/manager.go`. I found them with a regex search for `go` statements and `errgroup` `.Go(` calls. The search covered only the `scrape` directory, not subpackages, and I excluded `*_test.go`.

**`scrape/scrape.go`**
1. **`(*scrapePool).stop`** starts one goroutine per scrape loop (`scrape.go:281-284`). Each goroutine calls `l.stop()` and then `wg.Done()`. `stop` waits on the `WaitGroup` before it cancels the pool context and closes idle connections. This stops all loops in parallel.
2. **`(*scrapePool).restartLoops`**, called from `reload`, starts one goroutine per target (`scrape.go:399-406`). Each goroutine:
   - stops the old loop with `oldLoop.stop()` and calls `wg.Done()`;
   - sets the forced error and failure logger on the new loop;
   - runs the new loop with `newLoop.run(nil)`. That call blocks, so the goroutine lives for the new loop's lifetime.
3. **`(*scrapePool).sync`** starts two kinds of goroutines:
   - For targets that have disappeared, it starts `go func(l loop){ l.stop(); wg.Done() }` (`scrape.go:554-557`). It waits for these at the end with `wg.Wait()` (line 589).
   - For newly added targets, it starts `go l.run(nil)` (`scrape.go:582`). This is the scrape loop for that target. It is not tracked by the `WaitGroup`.

**`scrape/manager.go`**
4. **`(*Manager).Run`** starts `go m.reloader()` (`manager.go:224`). The `reloader` goroutine (`manager.go:249ff`) ticks every `DiscoveryReloadInterval` (default 5s). On each tick it checks `triggerReload` and then calls `m.reload()`. It exits when `m.graceShut` is closed. I did not read the middle of the loop, lines 276-334.
5. **`(*Manager).reload`** starts a goroutine per target set (`manager.go:310-313`). Each one calls `sp.Sync(groups)` and then `wg.Done()`, so pools sync in parallel. The function then waits on `wg.Wait()`.
6. **`(*Manager).reload`** also appears to contain a second set of goroutines at `manager.go:434-458`. I did not read the enclosing function signature, so the name is inferred from context. Each goroutine handles one existing scrape pool, and a `canReload` channel capped at `GOMAXPROCS` limits how many run at once.
   - If the pool's config is gone, it calls `sp.stop()` and records the pool in `toDelete`.
   - Otherwise it updates the failure logger. If the config differs (`reflect.DeepEqual`), it calls `sp.reload(cfg)` and sets `failed` on error.

**`(*Manager).Stop`** also starts goroutines, through `errgroup.Group.Go` (`manager.go:346-353`) rather than a `go` statement. Each calls `sp.stop()` for one pool, and the group is limited to `GOMAXPROCS` concurrent goroutines. `Stop` then waits on `g.Wait()` and closes `graceShut`.

**Uncertainty:** The text search would not catch goroutines started through other helpers or wrappers. I did not read lines 301-379 and 426-534 of `scrape.go`, or lines 229-289 and 326-414 of `manager.go`.