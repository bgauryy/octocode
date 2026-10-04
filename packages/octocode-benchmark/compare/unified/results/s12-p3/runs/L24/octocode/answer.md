Seven `go` statements in non-test code in `scrape/`, spread across six functions (`restartLoops` and `sync` hold one or two each). They are in `scrape.go` and `manager.go`. I matched `go func…` and `go x.y(…)` with a text search, so a goroutine started some other way would not appear. I read each goroutine's body. Where I say a function is the enclosing one, I inferred it from the function-start line numbers I listed.

**manager.go**
- `Manager.Run` (`manager.go:223`) starts `go m.reloader()` at line 224. `Run` itself then loops receiving target-set updates. Its comment says reloading happens in the background so it doesn't block receiving updates. I did not read the body of `reloader` (`manager.go:249`).
- `Manager.reload` (`manager.go:283`) starts a goroutine per target set at lines 310–313. It calls `sp.Sync(groups)` on that set's scrape pool and then `wg.Done()`. The comment says syncs run in parallel because they are slow and can't catch up under high load. `reload` waits on the `WaitGroup` at line 316.
- `Manager.ApplyConfig` (`manager.go:365`) starts a goroutine per scrape pool at lines 434–455 and beyond. Concurrency is capped by the `canReload` buffered channel, sized `GOMAXPROCS`.
  - If the pool has no config any more, it calls `sp.stop()` and records the pool in `toDelete`.
  - Otherwise it sets the scrape failure logger and calls `sp.reload(cfg)` if the config changed. On error it logs and sets `failed`. I saw this up to line 455; the rest of the body was not shown.
  - A deferred function calls `wg.Done()` and releases the `canReload` slot.

**scrape.go**
- `scrapePool.stop` (`scrape.go:271`) starts one goroutine per loop at lines 281–284. Each calls `l.stop()` and then `wg.Done()`. The function then does `wg.Wait()` and cancels the pool context.
- `scrapePool.restartLoops` (`scrape.go:355`) starts a goroutine for each replaced loop at lines 399–406. It does the following in order:
  - It calls `oldLoop.stop()` and then `wg.Done()`.
  - It sets the forced error and the scrape failure logger on the new loop.
  - It calls `newLoop.run(nil)`, so the new loop runs in that goroutine.
- `scrapePool.sync` (`scrape.go:485`) starts two kinds of goroutine:
  - At lines 554–557, one goroutine per target that has disappeared calls `l.stop()` and then `wg.Done()`. `sync` waits on these at line 589 so old scrapers finish before it returns.
  - At line 582, `go l.run(nil)` starts each new scrape loop. These are not tracked by the `WaitGroup`.

**Not verified:** I did not check `scrapeLoop.run` or what it starts internally. Its goroutines, if any, did not turn up in my search.