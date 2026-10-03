**Short answer:** In the Go compiler under `tsc/`, the default is computed in one place, `CompilerOptions.GetResolveJsonModule()`. Changing it would affect module resolution, file discovery, the checker's JSON-import error, `--showConfig` output, and completions. I found one consumer that doesn't use the getter and would not follow the change. I searched non-test Go files only. I didn't open every consumer, so the effects below come from the call sites and not from tracing the full behavior.

**Where the default is computed**
- `tsc/internal/core/compileroptions.go:270-280` holds `GetResolveJsonModule()`.
  - If the option is set explicitly, that value wins (`:271-272`).
  - Otherwise it returns true for emit module kind `Node20` or `NodeNext` (`:274-277`).
  - Otherwise it returns true when module resolution is `Bundler` (`:279`).
  - The comment at `:275` says "TODO in 6.0: add Node16/Node18".
- The raw option is the `ResolveJsonModule Tristate` field at `:94`. It is parsed in `tsoptions/parsinghelpers.go:471-472` and declared in `tsoptions/declscompiler.go:969`.

**Callers of the getter**
- **Module resolution:** `module/resolver.go:118-119` adds `extensionsJson` to the resolver's extensions, except for type-reference directives. This decides whether `.json` imports resolve at all.
- **Resolution diagnostics:** `module/util.go:140-144, 174` reports `Module_0_was_resolved_to_1_but_resolveJsonModule_is_not_used` (TS7042) when the getter returns false.
- **Checker:** `checker/checker.go:15666-15667` reports TS2732 ("Consider using resolveJsonModule…") when the getter is false and the module reference ends in `.json`.
- **Supported extensions helper:** `tsoptions/tsconfigparsing.go:2050-2051`, `GetSupportedExtensionsWithJsonIfResolveJsonModule`, adds `.json` to the supported extensions. It has three callers:
  - tsconfig `include` expansion at `tsconfigparsing.go:1961, 1971`
  - program file loading at `compiler/fileloader.go:159, 177, 681` and `compiler/program.go:249`
  - language-service path completions at `ls/string_completions.go:1069`
- **`--showConfig`:** `tsoptions/showconfig.go:47` computes `ResolveJsonModule` from the getter. Its declared dependencies are `ModuleResolution`, `Module` and `Target`. If the new default depends on other options, that dependency list needs updating too.

**Not affected by the getter**
- `project/project.go:218` hard-codes `ResolveJsonModule: core.TSTrue`. This is probably the language server's inferred-project options. It would not change when the default changes.

**Tests that may need updating**
- Files that mention the option:
  - `tsoptions/tsconfigparsing_test.go`
  - `execute/tsctests/showconfig_test.go`
  - `execute/tsctests/tsc_test.go`
  - `execute/tsctests/tscbuild_test.go`
  - fourslash tests such as `completionsPathsJsonModule_test.go` and `getEditsForFileRename_resolveJsonModule_test.go`
- I did not inspect them to see which depend on the current default.

**Uncertainty**
- I did not check the baseline reference files under the tests directory.
- I did not check the TypeScript-side implementation in `src/`. It may need to stay in sync.
- I did not check `ls/` beyond `string_completions.go`, because only that file references the option.