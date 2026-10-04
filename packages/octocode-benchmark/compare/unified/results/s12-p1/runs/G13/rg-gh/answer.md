**Repository:** `prometheus/node_exporter` (https://github.com/prometheus/node_exporter). It is the official Prometheus exporter for host metrics. I did not check the repo description itself, so the "official" part rests on the `prometheus` org and what I know of the project.

**Defaults at v1.8.2:**
- **Listen address:** `:9100`. `node_exporter.go:179` calls `kingpinflag.AddFlags(kingpin.CommandLine, ":9100")`, which registers `--web.listen-address` with that default. The flag definition is in exporter-toolkit's `web/kingpinflag/flag.go`. I read that file from the repo's default branch, not at the version pinned in `go.mod`. The `go.mod` at v1.8.2 pins exporter-toolkit v0.11.0.
- **Metrics path:** `/metrics`. `node_exporter.go:161-163` sets the `web.telemetry-path` default.
- **Max parallel scrape requests:** `40`. `node_exporter.go:169-171` sets the `web.max-requests` default, and `0` disables the limit.

**Uncertainty:** the `:9100` default is passed in from `node_exporter.go`, so it holds for v1.8.2 regardless of the toolkit file I read.