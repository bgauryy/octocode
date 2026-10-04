Seven goroutine starts in non-test code of `scrape` at ea954809ce, in six functions. They are all in `scrape.go` and `manager.go`. I searched for `go func` and `go x.y(` with a regex, so a goroutine started in some unusual form could have been missed. I read the surrounding code for every hit.

**scrape/scrape.go**
- **`(*scrapePool).stop`** (`scrape.go:271`) starts one goroutine per loop at `:281`. Each calls `l.stop()` and then `wg.Done()`. The function waits on `wg` before it cancels the pool context and closes idle connections.
- **`(*scrapePool).restartLoops`** (`scrape.go:355-414`) starts one goroutine per target at `:399`. It stops the old loop with `oldLoop.stop()` and then calls `wg.Done()`. It then sets the forced error and the failure logger on the new loop, and runs it with `newLoop.run(nil)`. That last call blocks for the loop's lifetime, but `wg.Done()` has already run.
- **`(*scrapePool).sync`** (`scrape.go:485-590`) starts goroutines in two places:
  - At `:554`, one goroutine per target that has disappeared. It calls `l.stop()` and then `wg.Done()`. The function waits on `wg` at the end so old scrapers finish before it returns.
  - At `:582`, `go l.run(nil)` runs the scrape loop for each new unique target. These are not waited on.

**scrape/manager.go**
- **`(*Manager).Run`** (`manager.go:223`) starts `go m.reloader()` at `:224`. This is a background goroutine, and its body (`:249-281`) is where reloads get triggered. I did not read its body. `Run` says reloading happens in the background so it doesn't block receiving target updates.
- **`(*Manager).reload`** (`manager.go:283-317`) starts a goroutine at `:310` for each scrape pool. It calls `sp.Sync(groups)` and then `wg.Done()`. The comment says the syncs run in parallel because they take a while and can fall behind under high load.
- **`(*Manager).ApplyConfig`** (`manager.go:365-471`) starts one goroutine per scrape pool at `:434`, with concurrency limited to `GOMAXPROCS` by the `canReload` channel. Each goroutine does one of three things:
  - If the pool's config is gone, it calls `sp.stop()` and records the pool in `toDelete`.
  - Otherwise it updates the scrape failure logger.
  - If the config changed, it calls `sp.reload(cfg)` and sets `failed` on error.

  On exit it calls `wg.Done()` and frees its `canReload` slot.