The setting is parsed into `model.ValidationScheme` from `github.com/prometheus/common`, version **v0.71.0**.

**Where the type comes from**
- The scrape config field is `MetricNameValidationScheme model.ValidationScheme` with the YAML key `metric_name_validation_scheme` (`config/config.go:851`).
- `model` is imported from `github.com/prometheus/common/model` (`config/config.go:34`).
- The root `go.mod:69` pins `github.com/prometheus/common v0.71.0`. A search of `config/config.go` found no `replace` directive. I didn't search `go.mod` for one.
- Other `go.mod` files in the checkout pin other versions, but they are separate modules:
  - `compliance/go.mod`: v0.71.0 (indirect)
  - `documentation/examples/remote_storage/go.mod`: v0.70.1
  - `web/ui/mantine-ui/src/promql/tools/go.mod`: v0.70.1 (indirect)

**Accepted values at v0.71.0**
I read these in `prometheus/common` at tag v0.71.0, `model/metric.go`. The type is an int enum. Its YAML and JSON unmarshalling go through `Set` (`model/metric.go:108-150`), which accepts these strings:
- `""`: leaves the value unchanged. It stays `UnsetValidation` (`model/metric.go:69`), which is documented as "should not be used in practice". Prometheus's scrape config replaces it with the global value (`config/config.go:1005`).
- `"legacy"` (`LegacyValidation`, `model/metric.go:71-74`):
  - Names must conform to the original Prometheus character rules (`MetricNameRE` and `LabelNameRE`).
  - Label names must be non-empty and match `[a-zA-Z_][a-zA-Z0-9_]*`. A digit is not allowed in the first position (`model/metric.go:178-188`).
  - Metric names must be non-empty and pass `isValidLegacyRune` for each rune (`model/metric.go:155-164`). I didn't read that helper. The usual legacy rule for metric names is `[a-zA-Z_:][a-zA-Z0-9_:]*`, which also allows colons, but that is from memory and not confirmed here.
- `"utf8"` (`UTF8Validation`, `model/metric.go:76-78`): names only need to be non-empty, valid UTF-8 strings (`utf8.ValidString`, `model/metric.go:165-169` for metrics, `189-193` for labels).
- Any other string returns the error `unrecognized ValidationScheme` (`model/metric.go:146-147`).

Prometheus's own scrape config check allows only `''`, `legacy` or `utf8` (`config/config.go:1002-1008`).