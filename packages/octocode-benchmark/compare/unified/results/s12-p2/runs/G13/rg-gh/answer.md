**Repository:** `prometheus/node_exporter` (https://github.com/prometheus/node_exporter).

**Defaults at v1.8.2**, all in `node_exporter.go` at tag v1.8.2:
- **Listen address:** `:9100`. It is set by `kingpinflag.AddFlags(kingpin.CommandLine, ":9100")` at about line 180. That call registers the `--web.listen-address` flag from exporter-toolkit with `:9100` as its default. I did not open the toolkit source to confirm the flag name, because my lookup of that file returned a 404.
- **Metrics path:** `/metrics`. This is the default of `--web.telemetry-path`, at about lines 160–163.
- **Max parallel scrape requests:** `40`. This is the default of `--web.max-requests`, at about lines 168–171. A value of 0 disables the limit.

The line numbers are approximate: I read them from a decoded copy of the file, and the second excerpt had no line numbers.