Changing how the default is computed would affect every place in the Go compiler that calls `GetResolveJsonModule()`. It would also change the `--showConfig` output and the `.json` file-extension logic. I searched non-test, non-generated `.go` files under `tsc/internal` and read only the default function itself. I did not read the bodies of the callers, and the test files are not covered below.

**The default lives in one place.** `tsc/internal/core/compileroptions.go:270-279` defines `GetResolveJsonModule()`:
- An explicit `ResolveJsonModule` setting wins.
- Otherwise it returns `true` for `ModuleKindNode20` and `ModuleKindNodeNext`, with a `TODO in 6.0: add Node16/Node18` comment.
- Otherwise it returns `true` when the module resolution kind is `Bundler`.

So the default depends on the emit module kind and the module resolution kind. The `Module` and `Target` dependencies listed in `showconfig.go` presumably come from how `GetEmitModuleKind()` and `GetModuleResolutionKind()` compute their own defaults.

**Everything that calls `GetResolveJsonModule()` would change:**
- **Module resolution:** `module/resolver.go:118-119` adds `extensionsJson` to the resolver state, so `.json` files can be resolved as modules.
- **Resolution diagnostics:** `module/util.go:140-141` builds the "need resolveJsonModule" message for `.json` files (case at `:174`).
- **Checker:** `checker/checker.go:15666-15667` reports "Cannot find module … Consider using resolveJsonModule" for `.json` imports when the option is off.
- **Supported extensions:** `tsoptions/tsconfigparsing.go:2050-2051` defines `GetSupportedExtensionsWithJsonIfResolveJsonModule`, which adds `.json` to the supported extensions. Its callers are:
  - `tsoptions/tsconfigparsing.go:1961`, for `include` and `exclude` file expansion, with JSON-only include handling around `:1971`.
  - `compiler/fileloader.go:159`, stored at `:177` and used by `isSupportedExtension` at `:681`.
  - `compiler/program.go:249`, for resolving file references.
  - `ls/string_completions.go:1069`, for path completions in Node-style module resolution.
- **`--showConfig` output:** `tsoptions/showconfig.go:47` computes the shown value via `GetResolveJsonModule`, with dependencies `ModuleResolution`, `Module` and `Target`. If the default depends on different options, this dependency list must be updated to match.

**Two places don't use the computed default:**
- `tsoptions/parsinghelpers.go:472-473` only parses the raw value into a `Tristate`.
- `project/project.go:218` hard-codes `ResolveJsonModule: core.TSTrue` in an inferred-project option set, so it would not follow a new default.

**Tests:** many tests mention `resolveJsonModule`, and the ones most likely to change are:
- `execute/tsctests/showconfig_test.go`
- `tsoptions/tsconfigparsing_test.go`
- `execute/tsctests/tsc_test.go`
- `execute/tsctests/tscbuild_test.go`
- the fourslash tests for JSON completions, find-all-references and file rename

I did not inspect them, and there may also be baseline files I didn't search.

**Not verified:** the diagnostics text in `diagnostics_generated.go` and the option declaration in `tsoptions/declscompiler.go` also mention the option. I did not read them.