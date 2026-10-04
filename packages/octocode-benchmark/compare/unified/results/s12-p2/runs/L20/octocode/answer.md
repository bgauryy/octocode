The default is computed in one place, `CompilerOptions.GetResolveJsonModule()` at `tsc/internal/core/compileroptions.go:270-280`. Changing it would change the result for every caller of that getter. The Go compiler lives under `tsc/` in this checkout. I found these callers with a text search and did not trace transitive callers or the tests.

**Current logic**
- It returns the explicit `ResolveJsonModule` value if it is set (lines 271-272).
- Otherwise it returns true when the emit module kind is `Node20` or `NodeNext` (lines 274-277).
- Otherwise it returns true when the module resolution kind is `Bundler` (line 279).
- A TODO at line 275 notes that `Node16`/`Node18` should be added in 6.0.

**Direct callers that would change behavior**
- **Checker:** `tsc/internal/checker/checker.go:15666` decides whether a `.json` module reference gets the resolveJsonModule error.
- **Module resolution:** `tsc/internal/module/resolver.go:118` only tries JSON resolution when the option is on. `tsc/internal/module/util.go:141` and `:174` use it to choose which diagnostic message to report (`needResolveJsonModule`).
- **Supported extensions:** `GetSupportedExtensionsWithJsonIfResolveJsonModule` (`tsc/internal/tsoptions/tsconfigparsing.go:2050-2051`) adds `.json` to the supported extensions when the option is on. Its users are:
  - `tsconfigparsing.go:1961` and `:1971`, where `ReadDirectory` expands include/exclude specs into files.
  - `tsc/internal/compiler/fileloader.go:159`, `:177` and `:681`.
  - `tsc/internal/compiler/program.go:249`.
  - `tsc/internal/ls/string_completions.go:1069`, which affects path completions.
- **`--showConfig`:** `tsc/internal/tsoptions/showconfig.go:47` computes the printed value with this getter. It lists `ModuleResolution`, `Module` and `Target` as dependencies, so the dependency list there would need to match any new inputs.

**Not affected**
- Parsing the explicit value (`tsc/internal/tsoptions/parsinghelpers.go:472`) writes only the raw tristate field, so it does not use the default.
- `tsc/internal/project/project.go:218` sets `ResolveJsonModule: core.TSTrue` explicitly, so the default does not apply there.

**Tests likely to break**
- Baselines under `tsc/testdata/baselines/` that cover JSON imports or `--showConfig` output.
- `TestBuildResolveJsonModule` (`tsc/internal/execute/tsctests/tscbuild_test.go:3197`).
- The fourslash test `completionsPathsJsonModuleWithoutResolveJsonModule_test.go`.

I did not check whether other code reads the raw `ResolveJsonModule` field directly.