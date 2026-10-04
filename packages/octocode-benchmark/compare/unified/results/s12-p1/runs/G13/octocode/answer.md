The official host-metrics exporter is `node_exporter`, hosted at https://github.com/prometheus/node_exporter. In v1.8.2 (commit `f1e0e8360aa60b6cb5e5cc1560bed348fc2c1895`) the defaults are:

- **Listen address:** `:9100`. `node_exporter.go:179` calls `kingpinflag.AddFlags(kingpin.CommandLine, ":9100")`, which registers the `--web.listen-address` flag with that default. I did not open the exporter-toolkit source to confirm that flag name. It is the toolkit's standard flag, and `toolkitFlags` is passed to `web.ListenAndServe` at line 223.
- **Metrics path:** `/metrics`. The `web.telemetry-path` flag default is at `node_exporter.go:160-163`.
- **Max parallel scrape requests:** `40`. The `web.max-requests` flag default is at `node_exporter.go:168-171`, and `0` disables the limit.