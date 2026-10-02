Changing the default would affect everything that calls `CompilerOptions.GetResolveJsonModule()`, plus the `--showConfig` output. The default is computed in one place, and every consumer reads it through that getter. All paths below are under `tsc/internal/`.

**Where the default lives:** `core/compileroptions.go:270-280`.
- An explicit `ResolveJsonModule` value wins.
- Otherwise it returns true when the emit module kind is `Node20` or `NodeNext`. A TODO on that case says to add Node16/Node18 in 6.0.
- Otherwise it returns true when the module resolution kind is `Bundler`.
- The field itself is `core/compileroptions.go:94`, and `tsoptions/parsinghelpers.go:472` sets it from the parsed option.

**Callers of `GetResolveJsonModule()`:**
- **Module resolution:** `module/resolver.go:118` adds `extensionsJson` to the resolver's extensions. This changes whether `.json` imports resolve at all.
- **Diagnostics:** `module/util.go:140-141` decides whether to report "Module '{0}' was resolved to '{1}', but '--resolveJsonModule' is not used".
- **Checker:** `checker/checker.go:15666` handles a `.json` module reference when the option is off.
- **Supported extensions:** `tsoptions/tsconfigparsing.go:2050-2051` (`GetSupportedExtensionsWithJsonIfResolveJsonModule`) adds `.json` to the supported extensions. Its callers are:
  - Include/exclude file expansion in `tsoptions/tsconfigparsing.go:1961`, which determines which `.json` files `include` globs pick up.
  - The file loader in `compiler/fileloader.go:159`, with the result used at `:681`.
  - `compiler/program.go:249`.
  - Import-path completions in `ls/string_completions.go:1069`.
- **`--showConfig`:** `tsoptions/showconfig.go:47` lists `ResolveJsonModule` as an implied option with dependencies `ModuleResolution`, `Module` and `Target`. If the new computation depends on other options, that dependency list must be updated, or the printed config goes stale.

**Not affected:** `project/project.go:218` sets `ResolveJsonModule: core.TSTrue` explicitly in its inferred-project options, so it bypasses the default.

**Tests:** I only grepped for the option name, so I can't say which baselines would change. Two hits are likely affected: `execute/tsctests/tscbuild_test.go:3197` (`TestBuildResolveJsonModule`) and `fourslash/tests/completionsPathsJsonModuleWithoutResolveJsonModule_test.go`. Any `--showConfig` or module-resolution baselines would also change.

I did not check whether other code reads the raw `options.ResolveJsonModule` field directly. My grep for `ResolveJsonModule` in `.go` files showed no such reads outside the getter and the parser.