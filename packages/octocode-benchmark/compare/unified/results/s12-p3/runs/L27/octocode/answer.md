**Answer:** The scrape config's `metric_name_validation_scheme` is parsed into `model.ValidationScheme`. That type comes from the module `github.com/prometheus/common`, pinned at **v0.71.0**.

**Pin and parsing site**
- `go.mod:69` pins `github.com/prometheus/common v0.71.0`. I saw no `replace` for it in the `go.mod` matches, but I only searched for the string `prometheus/common`, so I didn't scan the whole file for a `replace`.
- The scrape config field is declared at `config/config.go:851` as `MetricNameValidationScheme model.ValidationScheme` with the yaml tag `metric_name_validation_scheme`.
- The global config has the same field at `config/config.go:528`.

**Values at v0.71.0** (`model/metric.go`, read at tag v0.71.0, tag SHA 9a4aff03c1)
- The type is `type ValidationScheme int` (line 64).
- **`UnsetValidation`** (line 69, YAML string `""`): an undefined scheme that "should not be used in practice". Setting `""` leaves the value unchanged (`Set`, line 140). Prometheus replaces it with the global config's value (`config/config.go:1005`).
- **`LegacyValidation`** (line 74, YAML string `"legacy"`): metric and label names must match the original Prometheus character rules, `MetricNameRE` and `LabelNameRE`.
- **`UTF8Validation`** (line 78, YAML string `"utf8"`): names only need to be valid UTF-8 strings.
- `Set` (lines 138–149) accepts only `""`, `"legacy"` and `"utf8"`. Any other string returns `unrecognized ValidationScheme`.

**Uncertainty:** The `MetricNameRE` and `LabelNameRE` patterns are only named in the comment I read. I didn't open their definitions, so I can't quote the exact character set.