**Answer:** The scrape config's `metric_name_validation_scheme` is parsed into `model.ValidationScheme`. That type comes from `github.com/prometheus/common/model`, which the checkout pins at **v0.71.0**.

**Where the type is used and pinned**
- `config/config.go:851` declares the scrape config field as `MetricNameValidationScheme model.ValidationScheme` with the YAML tag `metric_name_validation_scheme`. The global config has the same field at `config/config.go:528`.
- `config/config.go:34` imports `github.com/prometheus/common/model`.
- `go.mod:69` pins `github.com/prometheus/common v0.71.0`.
- I only searched `config/config.go` for `replace`, and I did not search `go.mod` for one. I can't rule out a `replace` directive redirecting the module.
- Other `go.mod` files in the checkout pin different versions. `documentation/examples/remote_storage` and the mantine-ui tools module use v0.70.1, and `compliance` uses v0.71.0. These aren't the root module, so I left them out.

**Values at v0.71.0**
I read `model/metric.go` at the `v0.71.0` tag. The type is `type ValidationScheme int`, defined at lines 62–79.

| Value | YAML string | What it allows |
|---|---|---|
| `UnsetValidation` | `""` | Undefined and "should not be used in practice" (lines 67–69). Setting `""` leaves the value unchanged (`Set`, lines 140–141). `IsValidMetricName` and `IsValidLabelName` panic on it (lines 170–171, 194–195). |
| `LegacyValidation` | `"legacy"` | Names must match the original Prometheus character rules (`MetricNameRE` and `LabelNameRE`, lines 71–74). |
| `UTF8Validation` | `"utf8"` | Names only need to be non-empty valid UTF-8 (lines 76–78, 165–169, 189–193). |

- **Legacy metric names:** non-empty, with every rune accepted by `isValidLegacyRune` (lines 155–164). I didn't read that helper, so I can't say exactly which characters it accepts. The legacy metric-name pattern is normally `[a-zA-Z_:][a-zA-Z0-9_:]*`.
- **Legacy label names:** non-empty, with letters, `_`, and digits only after the first character (lines 178–188). That is `[a-zA-Z_][a-zA-Z0-9_]*`.
- **UTF-8 names:** `IsValidMetricName` and `IsValidLabelName` both require a non-empty string for which `utf8.ValidString` is true.

`Set` accepts only `""`, `"legacy"` and `"utf8"`. Anything else returns `unrecognized ValidationScheme` (lines 138–148).

Prometheus's own scrape-config validation at `config/config.go:1002-1008` rejects any value other than those three. Its error message is "must be either '', 'legacy' or 'utf8'".