Eight sites in non-test `scrape` code start goroutines. They are in seven functions, and `scrapePool.Sync` has two. Six use `go`. The seventh, in `Manager.Stop`, uses `errgroup.Group.Go`. Line numbers are at ea954809ce.

**`scrape/manager.go`**
- **`Manager.Run`** (`manager.go:224`): `go m.reloader()` starts a background loop. It runs until `m.graceShut` closes. It waits for the reload ticker (`DiscoveryReloadInterval`, default 5s) and then for a `triggerReload` signal, and calls `m.reload()` (`manager.go:249-281`). This keeps reloads from blocking the receipt of target-set updates. If `DiscoveryReloadOnStartup` is set, it also does an initial wait-and-reload before the loop.
- **`Manager.reload`** (`manager.go:310`): starts one goroutine per scrape pool. Each calls `sp.Sync(groups)` and then `wg.Done()`. The pools sync in parallel, and `reload` blocks on `wg.Wait()`.
- **`Manager.Stop`** (`manager.go:346-351`): `g.Go` from an `errgroup`, with the limit set to `GOMAXPROCS`. Each goroutine calls `sp.stop()` on one pool. `Stop` waits with `g.Wait()` and then closes `graceShut`.
- **`Manager.ApplyConfig`** (`manager.go:434`): starts one goroutine per existing pool, limited by the `canReload` channel (size `GOMAXPROCS`). If the pool's config is gone, the goroutine calls `sp.stop()` and marks the pool for deletion. Otherwise it sets the scrape-failure logger and reloads the pool with the new config (the reload call itself is past the lines I read).

**`scrape/scrape.go`**
- **`scrapePool.stop`** (`scrape.go:281`): one goroutine per loop. Each calls `l.stop()` and then `wg.Done()`, so the loops stop in parallel and `stop` waits for all of them.
- **`scrapePool.restartLoops`** (`scrape.go:399`; this is the function `reload` calls to rebuild loops): one goroutine per target. Each does the following:
  - stops the old loop with `oldLoop.stop()`;
  - signals `wg.Done()`;
  - sets the forced error and failure logger on the new loop;
  - runs `newLoop.run(nil)`, so the new loop's whole run happens in this goroutine.
- **`scrapePool.Sync`** has two sites:
  - `scrape.go:554`: for each target that has disappeared, a goroutine calls `l.stop()` and then `wg.Done()`.
  - `scrape.go:582`: `go l.run(nil)` starts each newly created scrape loop. `scrapeLoop.run` (`scrape.go:1404`) is the main scrape loop. It uses a ticker, applies an initial offset, scrapes and reports, and on exit handles scrape-on-shutdown and end-of-run staleness markers.

**Uncertainty:** I did not read the remainder of the `ApplyConfig` goroutine body or the full `scrapeLoop.run` body, so those descriptions are partial. The search covered `go func(...)` and `go name(...)` patterns plus `errgroup`. It would miss any other spawn style, but I saw none.