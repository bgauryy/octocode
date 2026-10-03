Changing how the default is computed would affect every code path that calls `GetResolveJsonModule()`, plus `tsc --showConfig` output. I read the function and searched all Go usages under `tsc/internal`. I did not run anything or check the test expectations.

**The default today**
- `GetResolveJsonModule()` is at `tsc/internal/core/compileroptions.go:270-280`.
- If the option is explicitly set (`ResolveJsonModule != TSUnknown`), it returns that value.
- Otherwise it returns true for emit module kinds `Node20` or `NodeNext`. A `TODO in 6.0` comment there says to add Node16/Node18.
- Failing that, it returns true when module resolution is `Bundler`.
- The field itself is a `Tristate` at line 94. The default is computed only in this getter, not stored, so changing it there changes all callers at once.

**Callers that would change behavior**
- **Checker:** `checker/checker.go:15666` reports the "needs resolveJsonModule" error when importing a `.json` file with the option off.
- **Module resolution:** `module/resolver.go:118` decides whether `.json` is tried as a resolution extension. `module/util.go:141` (in `needResolveJsonModule`, used around line 174) picks the diagnostic message.
- **Supported extensions:** `tsoptions/tsconfigparsing.go:2050-2051` (`GetSupportedExtensionsWithJsonIfResolveJsonModule`) adds `.json` when the option is on. It is called from:
  - `tsconfigparsing.go:1961`, where it feeds tsconfig `include` file matching (used at line 1971);
  - `compiler/fileloader.go:159`, stored at `:177` and used at `:681` when loading files;
  - `compiler/program.go:249`;
  - `ls/string_completions.go:1069`, for path completions in the language service.
- **`--showConfig`:** `tsoptions/showconfig.go:47` computes the displayed value through this getter. It declares `ModuleResolution`, `Module` and `Target` as dependencies, so a change to the inputs of the default should be reflected in that dependency list.

**Not affected by the default**
- `tsoptions/parsinghelpers.go:472` only parses the user's explicit value.
- `project/project.go:218` sets `ResolveJsonModule: core.TSTrue` explicitly, so it bypasses the default.

**Tests likely to be affected**
- `execute/tsctests/tscbuild_test.go:3197` (`TestBuildResolveJsonModule`).
- `fourslash/tests/completionsPathsJsonModuleWithoutResolveJsonModule_test.go`.
- Probably any baselines that depend on module kind or resolution. I didn't check these.

**Uncertainty**
- I did not search `testdata` or baselines, so I can't say which baseline files would change.
- I only found `GetResolveJsonModule`'s direct callers. Indirect effects, such as program-level diagnostics for JSON files, may exist beyond those listed.