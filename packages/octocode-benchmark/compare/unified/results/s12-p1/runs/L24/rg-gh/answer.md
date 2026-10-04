Five functions in non-test `scrape` code start goroutines. They are `Manager.Run`, `Manager.reload`, `Manager.ApplyConfig`, `scrapePool.stop`, `scrapePool.restartLoops` and `scrapePool.sync`, which makes six, not five. Together they contain 7 `go` statements. I found them by grepping for `go func` and `go name(` in `scrape/*.go`, excluding test files. I did not look at other packages.

**`scrape/manager.go`**
- **`Manager.Run`** (`manager.go:223`) starts `go m.reloader()` at line 224.
  - `reloader` (`manager.go:249`) is a background loop that triggers `m.reload()`.
  - It uses a ticker with `DiscoveryReloadInterval`, defaulting to 5s, and returns on `m.graceShut`.
  - If `DiscoveryReloadOnStartup` is set, it first waits for a `triggerReload` signal.
  - I read only the start of the function (up to line 268), not the full loop body.
  - The point is that target updates are received without being blocked by reloads.
- **`Manager.reload`** (`manager.go:283`) starts `go func(sp, groups)` at line 310.
  - It runs `sp.Sync(groups)` for one scrape pool, then `wg.Done()`.
  - There is one goroutine per pool, so pools sync in parallel. The code comment says syncs are slow and can fall behind under high load. `reload` then waits on `wg.Wait()`.
- **`Manager.ApplyConfig`** (`manager.go:365`) starts `go func(name, sp, cfg, ok)` at line 434.
  - It reloads one scrape pool in its own goroutine.
  - A `canReload` channel sized `GOMAXPROCS` limits how many run at once.
  - If the pool's config no longer exists (`!ok`), it calls `sp.stop()` and records the pool in `toDelete`.
  - Otherwise it first sets the pool's scrape-failure logger. I saw only that part of the goroutine body, and I assume it then reloads the pool with the new config.

**`scrape/scrape.go`**
- **`scrapePool.stop`** (`scrape.go:271`) starts `go func(l loop)` at line 281.
  - It calls `l.stop()` on each scrape loop, then `wg.Done()`. Loops are stopped concurrently, and `stop` waits for all of them.
- **`scrapePool.restartLoops`** (`scrape.go:355`) starts `go func(oldLoop, newLoop)` at line 399.
  - It calls `oldLoop.stop()` and then `wg.Done()`.
  - It then sets the forced error and failure logger on `newLoop` and runs `newLoop.run(nil)`.
  - The new loop therefore starts only after the old one has stopped. That goroutine keeps running as the new loop's run goroutine after `restartLoops` returns.
- **`scrapePool.sync`** (`scrape.go:485`) starts two kinds of goroutine:
  - `go func(l loop)` at line 554 runs `l.stop()` and then `wg.Done()`. It stops loops for targets that no longer exist.
  - `go l.run(nil)` at line 582 starts the scrape loop for each newly added target. It runs for the loop's lifetime.