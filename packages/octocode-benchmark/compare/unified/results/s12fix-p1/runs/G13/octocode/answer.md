The repository is **https://github.com/prometheus/node_exporter**. At tag v1.8.2 (commit `f1e0e8360aa60b6cb5e5cc1560bed348fc2c1895`), the defaults in `node_exporter.go` are:

- **Listen address:** `:9100`. It is set at `node_exporter.go:179` with `kingpinflag.AddFlags(kingpin.CommandLine, ":9100")`. That call defines the `--web.listen-address` flag.
- **Metrics path:** `/metrics`, from the `--web.telemetry-path` flag (`node_exporter.go:161-163`).
- **Maximum parallel scrape requests:** `40`, from `--web.max-requests` (`node_exporter.go:168-171`). Per the flag's help text, 0 disables the limit.

I didn't open the toolkit's flag code at the version v1.8.2 pins. I read the current `web/kingpinflag/flag.go` in `prometheus/exporter-toolkit`. It shows `--web.listen-address` takes its default from the address passed in. The `:9100` value itself comes from the v1.8.2 source above.