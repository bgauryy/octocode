The repository is **prometheus/node_exporter** (https://github.com/prometheus/node_exporter). At tag v1.8.2 its defaults are:

- **Listen address:** `:9100`. `node_exporter.go:179` calls `kingpinflag.AddFlags(kingpin.CommandLine, ":9100")`, which registers `--web.listen-address` with that default.
- **Metrics path:** `/metrics`. This is the `--web.telemetry-path` default at `node_exporter.go:161-163`.
- **Max parallel scrape requests:** `40`. This is the `--web.max-requests` default at `node_exporter.go:169-171`.

I did not open the exporter-toolkit source. The flag name `--web.listen-address` comes from my knowledge of that library, not from the file I read.