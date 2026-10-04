The repository is **prometheus/node_exporter** (https://github.com/prometheus/node_exporter). At v1.8.2 its defaults are:

- **Listen address:** `:9100`, so it listens on all interfaces on port 9100. This comes from `kingpinflag.AddFlags(kingpin.CommandLine, ":9100")` at `node_exporter.go:~181`. That call registers the `--web.listen-address` flag from exporter-toolkit v0.11.0, which `go.mod` pins. The README at line 21 also says it "listens on HTTP port 9100 by default".
- **Metrics path:** `/metrics`, the default of the `web.telemetry-path` flag at `node_exporter.go:~161-164`.
- **Max parallel scrape requests:** `40`, the default of the `web.max-requests` flag at `node_exporter.go:~168-171`. Setting it to 0 disables the limit.

Line numbers marked `~` are approximate. The grep showed exact lines for the flag names, but I read the defaults from a `sed` range without line numbers. I did not open the exporter-toolkit source, so the flag name `--web.listen-address` comes from my knowledge of that library, not from anything I read here.