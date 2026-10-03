Changing the default computation means editing one Go method, `CompilerOptions.GetResolveJsonModule`. The change would then reach four consumers: module resolution, the checker, tsconfig file-extension handling, and `--showConfig` output. I found these with a lexical search, not LSP reference tracing, so I could have missed indirect uses. I did not look at tests or baselines.

The tool results gave paths relative to `<checkout>/tsc/internal/`. That layout is odd for this repo, so I could not confirm the real location. I'm citing the relative paths as returned.

**Where the default is computed:** `core/compileroptions.go:270-280`.
- If `ResolveJsonModule` is explicitly set (not `TSUnknown`), that value is returned.
- Otherwise it returns `true` for `ModuleKindNode20` and `ModuleKindNodeNext`. A TODO at line 275 says to add Node16/Node18 in 6.0.
- In every other case it returns whether the module resolution kind is `Bundler`.
- So the default depends on `GetEmitModuleKind()` and `GetModuleResolutionKind()`.

**Callers that would change behavior** (each calls `GetResolveJsonModule()`):
1. `module/resolver.go:118` adds `extensionsJson` to the allowed resolution extensions when it is not a type-reference-directive lookup. A different default changes whether `.json` files resolve.
2. `module/util.go:141` decides whether to report the "Module '{0}' was resolved to '{1}', but '--resolveJsonModule' is not used" diagnostic.
3. `checker/checker.go:15666` decides whether to report "Cannot find module … Consider using '--resolveJsonModule'" when the import path ends in `.json`.
4. `tsoptions/tsconfigparsing.go:2051` (`GetSupportedExtensionsWithJsonIfResolveJsonModule`) decides whether `.json` is added to the supported extensions, which affects tsconfig file matching and include expansion.
5. `tsoptions/showconfig.go:47` is the `--showConfig` entry for `ResolveJsonModule`. It declares dependencies `ModuleResolution`, `Module` and `Target`, and computes the value with `GetResolveJsonModule`. If the new default depends on different options, that dependency list must be updated. Otherwise `--showConfig` may print or omit the computed value wrongly.

**Uncertainty:** I did not run LSP reference tracing, so a caller that reads the `ResolveJsonModule` field directly instead of the getter would not appear here. I also did not check tests, baselines or the TypeScript-side implementation.