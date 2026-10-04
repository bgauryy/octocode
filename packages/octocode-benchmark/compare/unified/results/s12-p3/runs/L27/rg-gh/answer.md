**Answer:** The setting is parsed into `model.ValidationScheme`, which comes from `github.com/prometheus/common` at **v0.71.0**, package `model`. At that version it accepts `legacy` and `utf8`, and an empty string leaves the value unset (details below).

**Where it is pinned and used (in the checkout)**
- `go.mod:69` pins `github.com/prometheus/common v0.71.0`.
- `config/config.go:34` imports `github.com/prometheus/common/model`.
- `config/config.go:851` declares the scrape config field as `MetricNameValidationScheme model.ValidationScheme` with the yaml key `metric_name_validation_scheme`.
- `config/config.go:1002-1008` checks the parsed value. `UnsetValidation` inherits the global value. `LegacyValidation` and `UTF8Validation` are accepted. Anything else returns an error saying the value "must be either '', 'legacy' or 'utf8'".

**Accepted values at v0.71.0** (`model/metric.go` in prometheus/common, read via `gh api` at tag v0.71.0, since the module source isn't in the checkout)
- YAML parsing goes through `Set` (`model/metric.go:138-148`), which accepts three strings:
  - `""` leaves the value unchanged, so it stays `UnsetValidation` (the zero value). Prometheus then falls back to the global setting.
  - `"legacy"` sets `LegacyValidation`.
  - `"utf8"` sets `UTF8Validation`.
  - Any other string returns `unrecognized ValidationScheme`.
- `UnsetValidation` is documented as "should not be used in practice". Calling `IsValidMetricName` or `IsValidLabelName` on it panics.

**What each value allows in names** (`IsValidMetricName` at `model/metric.go:153`, `IsValidLabelName` at `:176`)
- **`legacy`:**
  - Metric names must be non-empty and satisfy `isValidLegacyRune`, i.e. the original `MetricNameRE`. I did not read `isValidLegacyRune` or `MetricNameRE` themselves.
  - Label names must be non-empty and match `[a-zA-Z_][a-zA-Z0-9_]*`. The code allows letters and `_` anywhere, and digits only when the index is greater than 0.
- **`utf8`:** Metric and label names must be non-empty and valid UTF-8 (`utf8.ValidString`). There are no other character restrictions.

**Uncertainty:** I read the v0.71.0 source from GitHub rather than from a local module cache. I did not check `go.sum` or any `replace` directive in `go.mod`, so I can't rule out an override of the pin.