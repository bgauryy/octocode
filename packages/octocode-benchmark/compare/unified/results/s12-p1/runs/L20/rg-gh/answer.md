Changing how the default is computed means editing `CompilerOptions.GetResolveJsonModule()` at `tsc/internal/core/compileroptions.go:270-280`. Every consumer below goes through that getter, so they would all pick up the change. I read the code and did not run any tests.

**Current default logic**
- An explicit `ResolveJsonModule` value (a `Tristate`, field at `compileroptions.go:94`) wins (lines 271-272).
- Otherwise the default is true when the emit module kind is `Node20` or `NodeNext` (lines 274-277). A TODO at line 275 says Node16/Node18 should be added in 6.0.
- Otherwise the default is true when the module resolution kind is `Bundler` (line 279).
- Because of this, the default depends on `GetEmitModuleKind()` and `GetModuleResolutionKind()`, and through them on `Module` and `Target`.

**What the default feeds into**
- **Checker:** `internal/checker/checker.go:15666-15667` reports "Cannot find module… Consider using resolveJsonModule" (code 2732) for `.json` imports.
- **Module resolution:**
  - `internal/module/resolver.go:118` only tries JSON resolution when the option is on and it isn't a type-reference directive.
  - `internal/module/util.go:140-144` uses it to decide on the 7042 diagnostic, "resolved but resolveJsonModule is not used" (the check at line 174).
- **Include/file discovery:** `GetSupportedExtensionsWithJsonIfResolveJsonModule` (`internal/tsoptions/tsconfigparsing.go:2050-2051`) adds `.json` to the supported extensions. Its callers are:
  - tsconfig `include` expansion (`tsconfigparsing.go:1961-1971`)
  - `internal/compiler/program.go:249`
  - `internal/compiler/fileloader.go:159`, which stores the result at line 177 and uses it at line 681
  - `internal/ls/string_completions.go:1069`, so path completions offer or hide `.json`
- **`--showConfig`:** `internal/tsoptions/showconfig.go:47` computes the value through this getter. It declares dependencies on `ModuleResolution`, `Module` and `Target`, so changing which options the default reads means updating that list too.

**Not affected by the getter**
- `internal/project/project.go:218` hard-sets `ResolveJsonModule: core.TSTrue`, so it bypasses the default. The line is in the language-server project code; I did not check which config it builds.
- Parsing (`internal/tsoptions/parsinghelpers.go:471-472`) and the option declaration (`internal/tsoptions/declscompiler.go:969`) only handle explicit values.
- The diagnostics 5070 and 5071 (resolveJsonModule is invalid with classic resolution, or with module none/system/umd) exist in `internal/diagnostics/diagnostics_generated.go:2309-2311`. I did not find a validator that uses them in non-test code. A grep for the option name across non-test Go files found none.

**Tests**
I found tests that mention the option, for example `internal/fourslash/tests/completionsPathsJsonModuleWithoutResolveJsonModule_test.go` and `internal/execute/tsctests/tscbuild_test.go`. I did not open them. The baselines under `testdata`, such as showConfig and module-resolution output, would likely change if the default changes. I did not check those either.