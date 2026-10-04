Changing how the default is computed means changing `CompilerOptions.GetResolveJsonModule()`. Everything below reads the option through that getter, so it picks up the change. I did not run any tests.

**The default itself** is `tsc/internal/core/compileroptions.go:270-280`:
- An explicit `ResolveJsonModule` (anything other than `TSUnknown`) wins.
- Otherwise it returns true when the emit module kind is `Node20` or `NodeNext`. A TODO in the code says to add Node16/Node18 in 6.0.
- Otherwise it returns true when the module resolution kind is `Bundler`.
- Anything else is false.

**Callers affected** (all in `tsc/internal`):
- **Module resolution** (`module/resolver.go:118`): when the getter is true and the lookup isn't a type-reference directive, `extensionsJson` is added to the allowed extensions. Changing the default changes whether `.json` files resolve at all.
- **Checker** (`checker/checker.go:15666-15667`): when the getter is false and the module reference ends in `.json`, it reports "Cannot find module… Consider using resolveJsonModule".
- **Resolution diagnostics** (`module/util.go:140-144`, used at line 174): the `needResolveJsonModule` helper returns "Module '{0}' was resolved to '{1}' but '--resolveJsonModule' is not used" when the getter is false.
- **tsconfig include/files expansion** (`tsoptions/tsconfigparsing.go:2050-2051`, used at 1961 and 1971): `GetSupportedExtensionsWithJsonIfResolveJsonModule` decides whether `.json` files are matched by `include` globs.
- **`--showConfig` output** (`tsoptions/showconfig.go:47`): `ResolveJsonModule` is an "implied option" computed from this getter. Its declared dependencies are `ModuleResolution`, `Module` and `Target`. If the new logic reads other options, that dependency list must be updated, or the showConfig output will be wrong.

**Not driven by the getter:**
- `tsoptions/declscompiler.go:969-975` declares the option with `DefaultValueDescription: false`. This is the help and docs text. It already doesn't mention the Node/Bundler conditions, so it should be updated if the default changes.
- `tsoptions/parsinghelpers.go:471-472` only parses the explicit value into a `Tristate`.
- `project/project.go:218` hard-codes `ResolveJsonModule: core.TSTrue` for inferred projects, so it ignores the default.

**Tests:** many baseline cases under `tsc/testdata/tests/cases/compiler/` use `resolveJsonModule`, for example `requireOfJsonFile*.ts` and `modulePreserve5.ts`. I only confirmed they mention the option, not which ones depend on the default. Any change to the default could shift their baselines. There is also `tsoptions/tsconfigparsing_test.go:572`.

**Uncertainty:** I assumed "the Go-based compiler" means the `tsc/` Go module in this checkout, not the TypeScript-source compiler in `packages/typescript`. I didn't check `GetEmitModuleKind` or `GetModuleResolutionKind`, which the default depends on. I didn't find any reference in the Go code outside `tsc/internal`.