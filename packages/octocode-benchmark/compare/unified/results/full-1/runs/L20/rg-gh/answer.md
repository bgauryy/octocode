Changing the default computation means editing `CompilerOptions.GetResolveJsonModule()` at `tsc/internal/core/compileroptions.go:270`. That method is the only place the default is computed. Every effect below comes from its callers. Paths are relative to `tsc/internal/`, and I found the callers by searching the non-test Go code.

**Where the default is computed**
- The method returns the explicit `ResolveJsonModule` tristate if it is set (`core/compileroptions.go:271-272`).
- Otherwise it returns `true` when `GetEmitModuleKind()` is `Node20` or `NodeNext`, with a `TODO in 6.0: add Node16/Node18` (`:275-278`).
- Otherwise it returns `true` when `GetModuleResolutionKind()` is `Bundler` (`:279`).
- The default therefore depends on `module` and `moduleResolution`, and indirectly on `target`, because those getters compute their own defaults. `tsoptions/showconfig.go:47` declares those three dependencies.

**What a change would affect**
- **Module resolution:** `module/resolver.go:118` adds `extensionsJson` to the resolver state when the getter is true (and it isn't a type-reference directive). This decides whether `.json` files resolve at all.
- **Resolution diagnostics:** `module/util.go:140-144` uses it to choose the "Module '{0}' was resolved to '{1}', but '--resolveJsonModule' is not used" message (TS7042).
- **Checker:** `checker/checker.go:15666-15667` reports "Cannot find module … Consider using '--resolveJsonModule'" (TS2732) for unresolved `.json` imports when the getter is false.
- **Supported extensions:** `tsoptions.GetSupportedExtensionsWithJsonIfResolveJsonModule` (`tsoptions/tsconfigparsing.go:2050`) adds `.json` when the getter is true. Its callers are:
  - `compiler/fileloader.go:159`, which stores the result and uses it at `:681` when loading files.
  - `compiler/program.go:249`.
  - `tsoptions/tsconfigparsing.go:1961-1971`, so tsconfig `include` matching via `ReadDirectory` can pick up `.json` files.
  - `ls/string_completions.go:1069`, which affects path completions in the language service.
- **`--showConfig`:** `tsoptions/showconfig.go:47` prints the computed value, so the output would change.
- **Inferred projects:** `project/project.go:218` sets `ResolveJsonModule: core.TSTrue` explicitly. It is unaffected by a change to the default.
- **Option definition:** `tsoptions/declscompiler.go:969-975` declares the option with `DefaultValueDescription: false` and `AffectsModuleResolution: true`. This is a static description, so it would be out of sync if the computed default changes.

**Tests**
- Test files reference the option in many places: `tsoptions/tsconfigparsing_test.go`, `execute/tsctests/*`, and many `fourslash/tests/*` (for example `completionsPathsJsonModule*`, `getEditsForFileRename_resolveJsonModule`). I did not inspect them. Baselines that rely on the current default would likely change.

**Uncertainty**
- I did not find where TS5070 and TS5071 (`resolveJsonModule` with `classic` resolution, or with `module` none/system/umd) are emitted in the non-generated, non-test Go code. Only their message definitions turned up, so I can't say whether validation exists or interacts with a new default.
- I did not trace every caller of the getter's dependencies, `GetEmitModuleKind` and `GetModuleResolutionKind`.