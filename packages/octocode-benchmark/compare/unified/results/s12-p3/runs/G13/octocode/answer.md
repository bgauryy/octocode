The repository is **prometheus/node_exporter** (https://github.com/prometheus/node_exporter). In v1.8.2 (tag resolved to commit f1e0e83), all three defaults are set in `node_exporter.go`:

- **Listen address:** `:9100`. `node_exporter.go:179` calls `kingpinflag.AddFlags(kingpin.CommandLine, ":9100")`. That adds the `--web.listen-address` flag from exporter-toolkit, so the default is passed in rather than declared in this file.
- **Metrics path:** `/metrics`, from the `--web.telemetry-path` flag (`node_exporter.go:161-163`).
- **Max parallel scrape requests:** `40`, from `--web.max-requests` (`node_exporter.go:169-171`). The flag help says "Use 0 to disable."

I did not read exporter-toolkit's own source at the version v1.8.2 pins. The `:9100` value comes only from the `node_exporter.go` call.