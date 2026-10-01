Changing how the default is computed means editing `CompilerOptions.GetResolveJsonModule()` in `core/compileroptions.go:270-279`. That getter is the only place the default is computed, so every consumer listed below picks up the change. All paths are under `tsc/internal/`.

**Current default logic** (`core/compileroptions.go:270-279`): an explicit `ResolveJsonModule` value wins. Otherwise the default is `true` for `ModuleKindNode20` or `ModuleKindNodeNext`, and also when `GetModuleResolutionKind() == Bundler`. A `TODO in 6.0` comment at line 275 notes that Node16/Node18 are not yet included.

**Callers that would change behavior:**
- **Module resolution.** `module/resolver.go:118` adds `extensionsJson` to the resolver's extension set when the getter returns true, for non-type-reference lookups.
- **Resolution diagnostics.** `module/util.go:140-144` uses the getter to decide whether to report `Module_0_was_resolved_to_1_but_resolveJsonModule_is_not_used`.
- **Checker.** `checker/checker.go:15666-15667` reports "Cannot find module … Consider using resolveJsonModule" for `.json` imports when the getter returns false.
- **Supported extensions and file discovery.** `tsoptions/tsconfigparsing.go:2050-2051` (`GetSupportedExtensionsWithJsonIfResolveJsonModule`) appends `.json` when the getter returns true. It is used in these places:
  - `tsoptions/tsconfigparsing.go:1961-1971`, for include/`ReadDirectory` file matching in tsconfig.
  - `compiler/program.go:249`.
  - `compiler/fileloader.go:159`, stored at line 177 and used at line 681.
  - `ls/string_completions.go:1069`, for path completions in the language service.
- **`--showConfig`.** `tsoptions/showconfig.go:47` computes `ResolveJsonModule` as an implied option. Its declared dependencies are `ModuleResolution`, `Module` and `Target`. If the new default depends on other options, add them there, or the printed config can be wrong.

**Related places that are not derived from the getter:**
- `tsoptions/declscompiler.go:969-975` declares the option with `DefaultValueDescription: false`. That is a static description, so it would now be out of date.
- `project/project.go:218` hard-codes `ResolveJsonModule: core.TSTrue` for inferred projects, so they would not be affected.
- `tsoptions/parsinghelpers.go:471-472` only parses the explicit value.
- `diagnosticMessages.json:4623-4627` holds the messages for the option conflicting with `moduleResolution: classic` or `module: none/system/umd`. I only confirmed the messages exist. I did not look for where they are raised, so I can't say whether a new default would interact with that validation.

**Tests:** I only checked that the `*_test.go` files that mention the option include `execute/tsctests/tscbuild_test.go`, `showconfig_test.go`, `tsconfigparsing_test.go`, and fourslash tests for JSON completions and file renames. I didn't read them, so I can't say which would need updating or what output they expect.

I searched only the Go code for this. I did not check whether the TypeScript-side default differs.