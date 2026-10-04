The repository is **prometheus/node_exporter** (https://github.com/prometheus/node_exporter). At tag v1.8.2 its defaults are:

- **Listen address:** `:9100`. `node_exporter.go:179` calls `kingpinflag.AddFlags(kingpin.CommandLine, ":9100")`. That helper comes from exporter-toolkit and registers `--web.listen-address`. `README.md:21` also says it listens on HTTP port 9100 by default.
- **Metrics path:** `/metrics`. This is the default of `--web.telemetry-path` at `node_exporter.go:161-163`.
- **Max parallel scrape requests:** `40`. This is the default of `--web.max-requests` at `node_exporter.go:169-171`.

I did not open the exporter-toolkit source. That `--web.listen-address` is the flag the helper registers is my knowledge of that library, not something I read here.