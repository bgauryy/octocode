Changing the default would affect every caller of `CompilerOptions.GetResolveJsonModule()`. That method is where the Go compiler computes the default, and everything below is under `tsc/internal/` in the checkout.

**Where the default is computed**
- `core/compileroptions.go:270-280` returns the explicit value if `ResolveJsonModule != TSUnknown`.
- Otherwise it returns true for `ModuleKindNode20` and `ModuleKindNodeNext`. A TODO at line 275 says to add Node16/Node18 in 6.0.
- Failing that, it returns true when `GetModuleResolutionKind() == ModuleResolutionKindBundler`.
- The default therefore depends on `GetEmitModuleKind()` and `GetModuleResolutionKind()`. Changing it changes how `module`, `moduleResolution` and (via those two getters) `target` interact.

**Callers that would change behavior**
- **Checker:** `checker/checker.go:15666` decides whether to report an error for importing a `.json` module reference when `resolveJsonModule` is off.
- **Module diagnostics:** `module/util.go:140-141` and `:174` build the "need resolveJsonModule" diagnostic. If the default flipped, this message would appear or vanish.
- **Module resolver:** `module/resolver.go:118` only tries JSON resolution for non-type-reference-directive lookups when the option is on.
- **Supported extensions:** `tsoptions/tsconfigparsing.go:2050-2051` (`GetSupportedExtensionsWithJsonIfResolveJsonModule`) adds `.json` to the extension list only when the option is on. Its users are:
  - `tsconfigparsing.go:1961` and `:1971`, where tsconfig `include` expansion with `ReadDirectory` would start or stop matching `.json` files.
  - `compiler/fileloader.go:159`, `:177` and `:681`, which affect which files the loader treats as supported.
  - `compiler/program.go:249`.
  - `ls/string_completions.go:1069`, which affects path completions. The fourslash test `completionsPathsJsonModuleWithoutResolveJsonModule_test.go` covers this.
- **`--showConfig`:** `tsoptions/showconfig.go:47` computes the displayed value through `GetResolveJsonModule`. Its dependencies are listed as `ModuleResolution`, `Module` and `Target`. The listed dependencies would need to match any new logic, and the shown output would change.

**Not affected by the default**
- `tsoptions/parsinghelpers.go:472` only parses the explicit tristate value.
- `project/project.go:218` sets `ResolveJsonModule: core.TSTrue` explicitly, so it bypasses the default.
- The `ResolveJsonModule` field at `core/compileroptions.go:94` is stored as a `Tristate`, so the default is never stored in it.

**Tests**
I only confirmed that `execute/tsctests/tscbuild_test.go` and the fourslash test above mention `ResolveJsonModule`. I didn't read them or search for baselines, so I can't say which tests or baselines would need updating.