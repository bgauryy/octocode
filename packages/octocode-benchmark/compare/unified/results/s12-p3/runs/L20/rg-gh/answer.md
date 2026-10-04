**Answer:** In the Go compiler, the default is computed in one place, `CompilerOptions.GetResolveJsonModule()` at `tsc/internal/core/compileroptions.go:270-280`. A change there would affect every caller of that getter, which are module resolution, checker diagnostics, file discovery and loading, `--showConfig`, and language-service completions.

**The current logic** (`compileroptions.go:270-280`):
- An explicit `ResolveJsonModule` value wins (`:271-272`).
- Otherwise it returns true when `GetEmitModuleKind()` is `Node20` or `NodeNext` (`:275-277`).
- Otherwise it returns true when `GetModuleResolutionKind()` is `Bundler` (`:279`).
- A TODO at `:274` says Node16/Node18 are to be added in 6.0.

The field is `ResolveJsonModule Tristate` (`compileroptions.go:94`). It is set from user config at `tsoptions/parsinghelpers.go:472`, and the option is declared at `tsoptions/declscompiler.go:969`.

**Direct callers of `GetResolveJsonModule()`:**
- **Module resolution:**
  - `module/resolver.go:118` includes JSON extensions in resolution, except for type reference directives.
  - `module/util.go:141` is `needResolveJsonModule`, which chooses the diagnostic message when a JSON import can't be resolved (it is used at `:174`).
- **Checker:** `checker/checker.go:15666` reports the missing-`resolveJsonModule` error for imports of `.json` files.
- **Supported extensions:** `tsoptions/tsconfigparsing.go:2050-2051` has `GetSupportedExtensionsWithJsonIfResolveJsonModule`, which adds `.json` to the supported extensions. It feeds several places:
  - tsconfig `include` expansion, in `ReadDirectory` at `tsconfigparsing.go:1961-1971`;
  - `compiler/program.go:249`;
  - `compiler/fileloader.go:159`, `:177` and `:681`, where the loader matches files against the extension groups;
  - `ls/string_completions.go:1069`, for path completions.
- **`--showConfig`:** `tsoptions/showconfig.go:47` computes the displayed value with `GetResolveJsonModule`. Its declared dependencies are `ModuleResolution`, `Module` and `Target`. If the new default depends on other options, that list would need updating.

**Related but not default-dependent:**
- `project/project.go:218` hardcodes `ResolveJsonModule: core.TSTrue` in the language-service project's inferred options. Changing the default would not touch it.
- The tests I saw that reference the option are `fourslash/tests/completionsPathsJsonModuleWithoutResolveJsonModule_test.go` and `execute/tsctests/tscbuild_test.go`. Other tests or baselines, such as `--showConfig` output, may also change. I did not search for those.

**Uncertainty:** I searched only non-test `.go` files for `ResolveJsonModule`. I did not search for string-based uses of `"resolveJsonModule"` or check the testdata baselines. I also did not verify that `GetEmitModuleKind` and `GetModuleResolutionKind` have no other side effects.