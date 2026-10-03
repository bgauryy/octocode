The `moduleDetection` option is resolved to a kind, and that kind sets options for a per-file check of whether the file is a module. A file is a module when `file.ExternalModuleIndicator` ends up non-nil. All paths below are relative to the checkout's `typescript/` directory, which contains `tsc/internal/…`.

**1. Resolving the kind** (`tsc/internal/core/compileroptions.go:243-251`, `GetEmitModuleDetectionKind`)
- An explicit `moduleDetection` value is used as is. The accepted values are `auto`, `legacy` and `force` (`tsc/internal/tsoptions/enummaps.go:189-191`).
- If it is unset, the kind is `Force` when the emit module kind is between `Node16` and `NodeNext`. Otherwise it is `Auto`.

**2. Turning the kind into options** (`tsc/internal/ast/parseoptions.go:19-46`, `GetExternalModuleIndicatorOptions`)
- Declaration files (`.d.ts` and similar) always get empty options, so they are never forced.
- `Force` returns `{Force: true}`: every non-declaration file is a module.
- `Legacy` returns empty options: only the syntactic checks in step 3 apply.
- `Auto` returns two settings:
  - `JSX` is true when `jsx` is `react-jsx` or `react-jsxdev`.
  - `Force` is true when the file is forced to be a module by its format (`isFileForcedToBeModuleByFormat`, lines 49-56). That holds when the implied node format for emit is ESNext, or when the extension is `.cjs`, `.cts`, `.mjs` or `.mts`.
  - The comment says plain `.js` files outside a `type: module` scope are the only non-declaration files that aren't forced this way.

**3. The per-file decision** (`tsc/internal/ast/parseoptions.go:56-83`, `SetExternalModuleIndicator` and `getExternalModuleIndicator`)
The checks run in this order, and the first hit wins:
1. JSON files are never modules.
2. `isFileProbablyExternalModule` (lines 85-91) looks for a top-level statement that is one of these (`isAnExternalModuleIndicatorNode`, lines 95-99):
   - an `export` modifier,
   - `import x = require()`,
   - an import declaration,
   - an export assignment,
   - an export declaration.

   Failing that, it looks for `import.meta`, found by a tree walk that is guarded by the `PossiblyContainsImportMeta` flag.
3. Declaration files stop here and return nil, so they are scripts unless step 2 found something.
4. If `opts.JSX` is set, a JSX element or fragment anywhere in the file makes it a module.
5. If `opts.Force` is set, the file's own node is the indicator, so it is a module.
6. Otherwise the result is nil and the file is a script.

**Other consumers:** `tsc/internal/ls/autoimport/fix.go:915` checks whether the kind is `Force`. `tsc --init` writes `moduleDetection: force` (`tsc/internal/execute/tsc/init.go:199`).

**Uncertainty:** I did not trace where `GetExternalModuleIndicatorOptions` is called from, or the parser code that invokes `SetExternalModuleIndicator`. I also did not read the body of `GetImpliedNodeFormatForEmitWorker`.