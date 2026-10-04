Changing how the default is computed means editing `(*CompilerOptions).GetResolveJsonModule()` in `tsc/internal/core/compileroptions.go:270-280`. Every consumer of that method, listed below, would change behavior. In this checkout the Go compiler lives under `tsc/internal/`, not `internal/`.

**Current default logic**
- An explicit `ResolveJsonModule` value wins (`compileroptions.go:271-273`).
- Otherwise it is `true` when the emit module kind is `Node20` or `NodeNext` (`:274-277`). A `TODO in 6.0` comment there says to add `Node16`/`Node18`.
- Otherwise it is `true` when the module resolution kind is `Bundler` (`:279`).
- So the default depends on `GetEmitModuleKind()` and `GetModuleResolutionKind()`, and any change to those inputs also shifts the default.

**Affected callers of `GetResolveJsonModule()`**
- **Module resolution** (`module/resolver.go:118`): adds `extensionsJson` to the resolver's extensions unless it is a type reference directive. This determines whether `.json` imports resolve at all.
- **Resolution failure messages** (`module/util.go:140-144`, `:173-174`): decides whether a resolved `.json` file gets the "Module '{0}' was resolved to '{1}', but '--resolveJsonModule' is not used" diagnostic.
- **Checker** (`checker/checker.go:15666`): decides whether a `.json` import gets the "Cannot find module… Consider using resolveJsonModule" error.
- **Supported extensions** (`GetSupportedExtensionsWithJsonIfResolveJsonModule`, `tsoptions/tsconfigparsing.go:2050-2051`) feeds four places:
  - **tsconfig file expansion** (`tsconfigparsing.go:1961`, `:1971`): `.json` files matched by `include` get picked up. The `jsonOnlyIncludeMatchers` logic is also involved.
  - **File loader** (`compiler/fileloader.go:53`, `:159`, `:177`, `:681`): `isSupportedExtension` decides which files can be loaded.
  - **Triple-slash reference handling** (`compiler/program.go:249`).
  - **Import path completions** (`ls/string_completions.go:1069`): extensions are offered when the resolution kind uses node_modules.
- **`--showConfig` output** (`tsoptions/showconfig.go:47`): `ResolveJsonModule` is a computed option there with dependencies `ModuleResolution`, `Module` and `Target`. If the new logic reads different inputs, that dependency list would need updating. Printed configs would also change.

**Not affected**
- `tsoptions/parsinghelpers.go:472` only parses the explicit value.
- `project/project.go:218` hard-sets `ResolveJsonModule: core.TSTrue` for inferred projects, so it bypasses the default computation.

**Tests**
- Baselines are likely to change. Candidates include `TestBuildResolveJsonModule` (`execute/tsctests/tscbuild_test.go:3197`) and the fourslash test `completionsPathsJsonModuleWithoutResolveJsonModule_test.go:11`. I did not search the baselines or the other tests that rely on the default.

**Uncertainty**
- I searched only `*.go` files for the `ResolveJsonModule` identifier. Any other Go file that uses the option indirectly, for example through a copied struct, was not checked.
- I did not look at the JS-based compiler.