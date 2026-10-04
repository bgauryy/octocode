**Answer:** The type is `model.ValidationScheme` from `github.com/prometheus/common`, pinned at **v0.71.0** (`go.mod:69`). The scrape config field is `MetricNameValidationScheme model.ValidationScheme` with yaml tag `metric_name_validation_scheme` (`config/config.go:851`). The global config has the same field (`config/config.go:528`).

I read the type's source from GitHub at tag `v0.71.0`, `model/metric.go`. The checkout has no vendor directory and no local module cache. I did not check that the tag matches the module's go.sum hash.

**Accepted YAML values.** `UnmarshalYAML` reads a string and passes it to `Set` (`model/metric.go:108-114`, `138-150`).
- `""` leaves the value unchanged, so it stays `UnsetValidation`.
- `"legacy"` gives `LegacyValidation`.
- `"utf8"` gives `UTF8Validation`.
- Any other string returns the error `unrecognized ValidationScheme`.

The constants are declared at `model/metric.go:66-79`. `String()` returns `"unset"`, `"legacy"` and `"utf8"` (`model/metric.go:82-93`). `UnsetValidation` is the zero value and is marked "should not be used in practice."

**What each value allows** (`IsValidMetricName` at `model/metric.go:153-173`, `IsValidLabelName` at `176-197`):

| Value | Metric names | Label names |
|---|---|---|
| `legacy` | Non-empty. Each character must be `[a-zA-Z_:]`, or a digit that is not first (`isValidLegacyRune`, `548-550`). | Non-empty. Each character must be `[a-zA-Z_]`, or a digit that is not first. Colons are not allowed (`184`). |
| `utf8` | Any non-empty, valid UTF-8 string (`165-169`). | Any non-empty, valid UTF-8 string (`189-193`). |
| `unset` | Both methods panic with "Invalid name validation scheme requested" (`170-171`, `194-195`). | Same panic. |

**Prometheus-side handling of `unset`.** In `config/config.go`, `GlobalConfig` replaces an unset value with the default (`:649-652`). The default is `model.UTF8Validation` (`:191`). `:730` also compares the scrape config's value to `model.UnsetValidation`. I did not read that code, so I haven't checked how it resolves an unset scrape-level value.