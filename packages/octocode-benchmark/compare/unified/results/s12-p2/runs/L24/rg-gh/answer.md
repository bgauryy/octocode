Five non-test functions in `scrape` start goroutines: `Manager.Run`, `Manager.reload`, `Manager.Stop`, `scrapePool.stop`, `scrapePool.reload` and `scrapePool.sync`. That is six, and I counted six below. I grepped for `go` statements and `errgroup`'s `.Go(` in the non-test files. I read the code around each hit but did not run anything. Line numbers are from the checkout, and I did not confirm it is at `ea954809ce`, because `git` failed in this sandbox.

**`scrape/manager.go`**
- **`Manager.Run`** (`manager.go:224`) runs `go m.reloader()`. The goroutine (`manager.go:249`) is a background loop. It ticks every `DiscoveryReloadInterval`, or 5s if that is unset. On each tick it waits for a `triggerReload` signal and then calls `m.reload()`. It exits when `graceShut` is closed. If `DiscoveryReloadOnStartup` is set, it first waits for one trigger and reloads once.
- **`Manager.reload`** has two goroutine sites.
  - `manager.go:310` starts one goroutine per scrape pool, and each calls `sp.Sync(groups)` and then `wg.Done()`. The comment says the syncs run in parallel because they are slow and can't keep up under high load. The function waits on the `WaitGroup` before returning.
  - `manager.go:434` is in a separate function that I did not identify by name. It sits between the `Manager.reloader` loop (`manager.go:249`) and `Manager.Stop` (`manager.go:338`), and it is not `Manager.reload` at `manager.go:283`. The code there is "reload each scrape pool in a dedicated goroutine", so it is probably the `ApplyConfig` path. Each goroutine does one of two things.
    - If the pool's config was removed, it calls `sp.stop()` and records the name for deletion.
    - Otherwise it sets the scrape-failure logger and calls `sp.reload(cfg)` if the config changed.

    The number running at once is capped at `GOMAXPROCS` by the `canReload` channel (`manager.go:420`).
- **`Manager.Stop`** (`manager.go:346-352`) uses an `errgroup.Group` limited to `GOMAXPROCS`. `g.Go` runs `sp.stop()` for each pool in parallel. After `g.Wait()` it closes `graceShut`.

**`scrape/scrape.go`**
- **`scrapePool.stop`** (`scrape.go:281`) starts one goroutine per loop. Each calls `l.stop()` and then `wg.Done()`. The function waits for all of them before it cancels the pool's context.
- **`scrapePool.reload`** (`scrape.go:399`) starts one goroutine per replaced loop. It calls `oldLoop.stop()` and then `wg.Done()`. It then sets the forced error and failure logger on the new loop and runs `newLoop.run(nil)`. That goroutine therefore becomes the new scrape loop for as long as it runs.
- **`scrapePool.sync`** has two sites.
  - `scrape.go:554` starts one goroutine per target that has gone away. It calls `l.stop()` and then `wg.Done()`.
  - `scrape.go:582` is `go l.run(nil)` for each new loop. This is the long-running scrape loop (`scrapeLoop.run`, `scrape.go:1404`).

**Uncertainty**
- I did not read `scrapeLoop.run` itself, only that it is what these goroutines call.
- I did not search for other ways of starting goroutines, such as a function passed to a helper that spawns it.
- I used the sed output to infer which function contains `scrape.go:399` and `scrape.go:554`. They fall in `reload` and `sync` by line range, since `reload` starts at 316 and `sync` at 485.