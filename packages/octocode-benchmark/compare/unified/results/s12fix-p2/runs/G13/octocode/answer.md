The repository is **prometheus/node_exporter** (https://github.com/prometheus/node_exporter). At v1.8.2 (commit f1e0e83) its defaults are:

- **Listen address:** `:9100`. `node_exporter.go:179` calls `kingpinflag.AddFlags(kingpin.CommandLine, ":9100")`. That helper registers `--web.listen-address` with the passed address as its default (exporter-toolkit `web/kingpinflag/flag.go:40-43`, read on its default branch rather than at the version node_exporter pins).
- **Metrics path:** `/metrics`. This is the `--web.telemetry-path` default (`node_exporter.go:160-163`).
- **Max parallel scrape requests:** `40`. This is the `--web.max-requests` default, and `0` disables the limit (`node_exporter.go:168-171`).

**Uncertainty:** I read the exporter-toolkit file from its default branch, not the version v1.8.2 pins. The `:9100` default itself comes from node_exporter's own call at the v1.8.2 tag. The flag-registration code could differ slightly in the pinned toolkit version.